//go:build linux && (amd64 || arm64)

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
	crashAfter := fs.Duration("crash-after", 0, "testing only: SIGKILL this process after the delay, to check that the kernel removes the probes")
	noPlain := fs.Bool("no-plain", false, "only observe TLS libraries; skip the syscall probes for cleartext HTTP")
	debugUnmatched := fs.Bool("debug-unmatched", false, "testing only: log the first bytes of writes that do not start an HTTP POST")
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

	t, err := tap.New(tap.Options{WatchAll: sel.all, NoPlain: *noPlain, DebugUnmatched: *debugUnmatched})
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

	asm.OnDone = func(pid uint32, ssl uint64) {
		if ssl&tap.PlainFlag != 0 {
			t.ClearPlain(pid, uint32(ssl&0xffffffff))
		}
	}
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
	if *crashAfter > 0 {
		go func() { time.Sleep(*crashAfter); _ = syscall.Kill(os.Getpid(), syscall.SIGKILL) }()
	}
	var deadline <-chan time.Time
	if *duration > 0 {
		deadline = time.After(*duration)
	}

	scanner := inspect.NewScanner("/proc")
	self := os.Getpid()
	type pidState struct {
		sig     string
		tries   int
		done    bool
		watched bool // plain-HTTP syscall probes look at this process
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
			attached := t.IsAttached(pid) || st.watched
			if st.done && !(attached && scans%5 == 0) {
				continue
			}
			p := scanner.Process(pid)
			if !attached && !sel.match(p) {
				st.done = true
				continue
			}
			if !*noPlain && !sel.all && !st.watched {
				t.Watch(pid)
				st.watched = true
			}
			if p.Attach != "yes" {
				if st.watched || sel.all {
					if !st.done {
						logf("watching pid %d (%s) for plain HTTP (no TLS library visible)", pid, p.Comm)
					}
					st.done = true
					continue
				}
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
				t.Unwatch(pid)
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
			if ev.SSL&(1<<61) != 0 && ev.SSL&tap.PlainFlag == 0 {
				logf("enter sample pid=%d comm=%s syscall=%d dir=%d fd=%d len=%d first=%q", ev.PID, ev.Comm, (ev.SSL>>32)&0xffff, ev.Dir, ev.SSL&0xffffffff, ev.Total, firstBytes(ev.Data, 24))
				continue
			}
			if ev.SSL&tap.DebugFlag != 0 && ev.SSL&tap.PlainFlag == 0 {
				logf("unmatched write pid=%d comm=%s fd=%d len=%d first=%q", ev.PID, ev.Comm, ev.SSL&0xffffffff, ev.Total, firstBytes(ev.Data, 32))
				continue
			}
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
	if *verbose {
		c := t.Counters()
		logf("plain-HTTP probe counters: seen=%d completed=%d marked=%d emitted=%d not_post=%d exit_unwatched=%d head_unreadable=%d", c[0], c[1], c[2], c[3], c[4], c[6], c[7])
	}
	closeStart := time.Now()
	t.Close()
	if *verbose {
		logf("probes detached in %s", time.Since(closeStart).Round(time.Millisecond))
	}
	// The reader goroutine may be blocked sending to a full channel, so keep consuming until it has stopped.
drain:
	for {
		select {
		case ev := <-events:
			asm.Feed(ev.PID, ev.Comm, ev.SSL, int(ev.Dir), ev.Data, ev.Time)
		case <-readerDone:
			break drain
		}
	}
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

func logf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "tokprof-ebpf %s: "+format+"\n", append([]any{time.Now().Format("15:04:05.000")}, a...)...)
}

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

func runStatus() {
	n, err := tap.LoadedPrograms()
	if err != nil {
		fatal("%v (needs root or CAP_SYS_ADMIN/CAP_BPF)", err)
	}
	fmt.Printf("tokprof eBPF programs loaded in the kernel: %d\n", n)
}
