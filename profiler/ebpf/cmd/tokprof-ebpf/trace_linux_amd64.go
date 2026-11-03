//go:build linux && amd64

package main

import (
	"bufio"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/cilium/ebpf"
	"github.com/cilium/ebpf/ringbuf"

	"github.com/dotnetpower/agent-tokenomics/profiler/ebpf/internal/httpasm"
	"github.com/dotnetpower/agent-tokenomics/profiler/ebpf/internal/inspect"
	"github.com/dotnetpower/agent-tokenomics/profiler/ebpf/internal/tap"
)

const maxAttachTries = 120 // rescans (about a minute) before giving up on a selected process

type selector struct {
	pids  map[int]bool
	comms []string
	all   bool
}

func (s selector) match(p inspect.Process) bool {
	if s.all || s.pids[p.PID] {
		return true
	}
	hay := strings.ToLower(p.Comm + " " + p.Exe + " " + p.Cmdline)
	for _, c := range s.comms {
		if strings.Contains(hay, c) {
			return true
		}
	}
	return false
}

type outLine struct {
	Source   string  `json:"source"`
	TS       float64 `json:"ts"`
	PID      uint32  `json:"pid"`
	Comm     string  `json:"comm"`
	Host     string  `json:"host"`
	Path     string  `json:"path"`
	Request  string  `json:"request"`
	Response string  `json:"response"`
	SSE      bool    `json:"sse"`
	Status   int     `json:"status"`
	TTFBMs   float64 `json:"ttfb_ms"`
	Latency  float64 `json:"latency_ms"`
}

func runTrace(args []string) {
	fs := flag.NewFlagSet("trace", flag.ExitOnError)
	pidList := fs.String("pid", "", "comma-separated PIDs to watch")
	commList := fs.String("comm", "", "comma-separated substrings matched against process name, path and command line")
	all := fs.Bool("all", false, "watch every process the helper can attach to (captures all TLS traffic it can parse; see the privacy notes)")
	duration := fs.Duration("duration", 0, "stop after this long (0 = until interrupted)")
	idle := fs.Duration("idle", 2*time.Second, "treat a response as finished after this much silence")
	exitOnEOF := fs.Bool("exit-on-stdin-eof", false, "exit (and remove all probes) when stdin closes; used by the Python launcher")
	debug := fs.Bool("debug", false, "print the full eBPF verifier log when the program fails to load")
	verbose := fs.Bool("verbose", false, "log connections that were ignored and why")
	dump := fs.Bool("dump-events", false, "log every captured chunk (debugging; prints the first bytes of plaintext)")
	_ = fs.Parse(args)

	sel := selector{pids: map[int]bool{}, all: *all}
	for _, f := range strings.Split(*pidList, ",") {
		if f = strings.TrimSpace(f); f != "" {
			n, err := strconv.Atoi(f)
			if err != nil {
				fatal("invalid --pid %q", f)
			}
			sel.pids[n] = true
		}
	}
	for _, f := range strings.Split(strings.ToLower(*commList), ",") {
		if f = strings.TrimSpace(f); f != "" {
			sel.comms = append(sel.comms, f)
		}
	}
	if !sel.all && len(sel.pids) == 0 && len(sel.comms) == 0 {
		fatal("choose what to watch: --pid, --comm or --all")
	}

	t, err := tap.New()
	if err != nil {
		if *debug {
			var ve *ebpf.VerifierError
			if errors.As(err, &ve) {
				fmt.Fprintf(os.Stderr, "%+v\n", ve) // full verifier log
			}
		}
		if errors.Is(err, syscall.EPERM) || errors.Is(err, syscall.EACCES) || strings.Contains(err.Error(), "operation not permitted") {
			fatal("cannot load the eBPF program: %v\nrun it as root (for example through sudo) or grant CAP_BPF, CAP_PERFMON and CAP_SYS_RESOURCE", err)
		}
		fatal("%v", err)
	}
	out := bufio.NewWriter(os.Stdout)
	enc := json.NewEncoder(out)
	asm := httpasm.New(func(e httpasm.Exchange) {
		_ = enc.Encode(outLine{
			Source: "ebpf", TS: float64(e.Time.UnixNano()) / 1e9, PID: e.PID, Comm: e.Comm, Host: e.Host, Path: e.Path,
			Request: e.Request, Response: e.Response, SSE: e.SSE, Status: e.Status, TTFBMs: e.TTFBMs, Latency: e.LatencyMs,
		})
		_ = out.Flush()
	}, *idle)

	if *verbose {
		asm.Debug = logf
	}
	events := make(chan tap.Event, 4096)
	readerDone := make(chan struct{})
	go func() {
		defer close(readerDone)
		for {
			ev, err := t.Read()
			if err != nil {
				if errors.Is(err, ringbuf.ErrClosed) {
					return
				}
				continue
			}
			events <- ev
		}
	}()

	stop := make(chan struct{})
	sigs := make(chan os.Signal, 1)
	signal.Notify(sigs, os.Interrupt, syscall.SIGTERM)
	go func() { <-sigs; close(stop) }()
	if *exitOnEOF {
		go func() {
			_, _ = io.Copy(io.Discard, os.Stdin)
			select {
			case <-stop:
			default:
				close(stop)
			}
		}()
	}
	var deadline <-chan time.Time
	if *duration > 0 {
		deadline = time.After(*duration)
	}

	scanner := inspect.NewScanner("/proc")
	self := os.Getpid()
	type pidState struct {
		sig   string
		tries int
		done  bool
	}
	states := map[int]*pidState{}
	maxTries := maxAttachTries
	if sel.all {
		maxTries = 10
	}
	scans := 0
	rescan := func() {
		scans++
		live := map[int]bool{}
		for _, pid := range scanner.PIDs() {
			live[pid] = true
			if pid == self {
				continue
			}
			sig := scanner.Signature(pid)
			st := states[pid]
			if st == nil || st.sig != sig { // new process, or it exec'd into something else
				st = &pidState{sig: sig}
				states[pid] = st
			}
			attached := t.IsAttached(pid)
			if st.done && !(attached && scans%5 == 0) {
				continue
			}
			p := scanner.Process(pid)
			if !attached && !sel.match(p) {
				st.done = true
				continue
			}
			if p.Attach != "yes" {
				if st.tries++; st.tries >= maxTries {
					st.done = true
					logf("pid %d (%s): not attachable: %s", pid, p.Comm, firstNonEmpty(p.Note, "no TLS library found"))
				}
				continue
			}
			added, err := t.Attach(p)
			if err != nil {
				if st.tries++; st.tries >= maxTries {
					st.done = true
					logf("pid %d (%s): attach failed: %v", pid, p.Comm, err)
				}
			}
			if added > 0 {
				logf("attached pid %d (%s)", pid, p.Comm)
			}
			if t.IsAttached(pid) {
				st.done = true // re-checked every few scans for libraries loaded later
			}
		}
		for pid := range states {
			if !live[pid] {
				t.Detach(pid)
				asm.ForgetPID(uint32(pid))
				delete(states, pid)
			}
		}
	}

	rescan()
	logf("ready")
	tick := time.NewTicker(time.Second)
	defer tick.Stop()
	for running := true; running; {
		select {
		case ev := <-events:
			if *dump {
				logf("ev pid=%d tid=%d ssl=%#x dir=%d total=%d off=%d len=%d %q", ev.PID, ev.TID, ev.SSL, ev.Dir, ev.Total, ev.Off, len(ev.Data), firstBytes(ev.Data, 20))
			}
			asm.Feed(ev.PID, ev.Comm, ev.SSL, int(ev.Dir), ev.Data, ev.Time)
			if ev.Total > tap.MaxCapture && ev.Off+uint32(len(ev.Data)) >= tap.MaxCapture {
				asm.MarkDamaged(ev.PID, ev.SSL, int(ev.Dir))
			}
		case <-tick.C:
			if len(events) == 0 { // while behind, silence in the stream is not a finished response
				asm.Sweep(time.Now())
			}
			rescan()
		case <-stop:
			running = false
		case <-deadline:
			running = false
		}
	}
	dropped := t.Dropped()
	t.Close()
	<-readerDone
	for len(events) > 0 {
		ev := <-events
		asm.Feed(ev.PID, ev.Comm, ev.SSL, int(ev.Dir), ev.Data, ev.Time)
	}
	asm.Close()
	s := asm.Stats
	logf("stopped: exchanges=%d ignored=%d non_http=%d incomplete=%d unreadable=%d kernel_dropped=%d",
		s.Exchanges, s.Ignored, s.NonHTTP, s.Incomplete, s.Unreadable, dropped)
}

func firstNonEmpty(a, b string) string {
	if a != "" {
		return a
	}
	return b
}

func logf(format string, a ...any) { fmt.Fprintf(os.Stderr, "tokprof-ebpf: "+format+"\n", a...) }

func fatal(format string, a ...any) {
	logf(format, a...)
	os.Exit(1)
}

func firstBytes(b []byte, n int) []byte {
	if len(b) > n {
		return b[:n]
	}
	return b
}
