//go:build linux && (amd64 || arm64)

// Package tap loads the TLS plaintext eBPF program and attaches it to processes.
package tap

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"os"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/cilium/ebpf"
	"github.com/cilium/ebpf/link"
	"github.com/cilium/ebpf/ringbuf"
	"github.com/cilium/ebpf/rlimit"
	"golang.org/x/sys/unix"

	"github.com/dotnetpower/agent-tokenomics/profiler/ebpf/internal/inspect"
)

const (
	DirWrite = 0
	DirRead  = 1
	hdrSize  = 56

	// PlainFlag marks the connection id of a cleartext socket (the top bit; the rest is the file descriptor).
	PlainFlag = uint64(1) << 63

	// DebugFlag marks a debug sample (see Options.DebugUnmatched).
	DebugFlag = uint64(1) << 62

	// MaxCapture is the most bytes of one TLS call the kernel side copies (16 chunks of 16 KiB).
	MaxCapture = 16 * 16384
)

// Event is one chunk of plaintext moved by a TLS read or write call.
type Event struct {
	PID, TID uint32
	SSL      uint64
	Dir      uint32
	Total    uint32 // bytes moved by the call (may exceed what was captured)
	Off      uint32
	Comm     string
	Time     time.Time // when the call returned, from the kernel clock
	Data     []byte
}

type Tap struct {
	objs   tlsObjects
	reader *ringbuf.Reader
	mu     sync.Mutex
	links  map[int]map[string][]link.Link
	once   sync.Once
	tps    []link.Link // syscall tracepoints for plain HTTP
	base   time.Time   // wall-clock time of CLOCK_MONOTONIC == 0
}

// Options tune what the kernel side watches.
type Options struct {
	WatchAll bool // observe plain HTTP in every process (otherwise only those added with Watch)
	NoPlain  bool // do not attach the syscall tracepoints at all
	// DebugUnmatched reports the first bytes of writes that did not start an HTTP POST (testing only).
	DebugUnmatched bool
}

// New loads the program into the kernel. It needs CAP_BPF/CAP_PERFMON (or root).
func New(opts Options) (*Tap, error) {
	if err := rlimit.RemoveMemlock(); err != nil {
		return nil, fmt.Errorf("raise memlock limit: %w", err)
	}
	var ts unix.Timespec
	_ = unix.ClockGettime(unix.CLOCK_MONOTONIC, &ts)
	t := &Tap{base: time.Now().Add(-time.Duration(ts.Nano())), links: map[int]map[string][]link.Link{}}
	spec, err := loadTls()
	if err != nil {
		return nil, fmt.Errorf("load eBPF spec: %w", err)
	}
	_ = spec.Variables["self_tgid"].Set(uint32(os.Getpid()))
	if opts.DebugUnmatched {
		_ = spec.Variables["debug_unmatched"].Set(uint32(1))
	}
	if opts.WatchAll {
		_ = spec.Variables["watch_all"].Set(uint32(1))
	}
	var st syscall.Stat_t
	if err := syscall.Stat("/proc/self/ns/pid", &st); err == nil {
		_ = spec.Variables["ns_dev"].Set(uint64(st.Dev))
		_ = spec.Variables["ns_ino"].Set(st.Ino)
	}
	if err := spec.LoadAndAssign(&t.objs, nil); err != nil {
		return nil, fmt.Errorf("load eBPF objects: %w", err)
	}
	r, err := ringbuf.NewReader(t.objs.Events)
	if err != nil {
		t.objs.Close()
		return nil, fmt.Errorf("open ring buffer: %w", err)
	}
	t.reader = r
	if !opts.NoPlain {
		if err := t.attachSyscalls(); err != nil {
			t.Close()
			return nil, err
		}
	}
	return t, nil
}

func (t *Tap) attachSyscalls() error {
	pairs := []struct {
		name string
		prog *ebpf.Program
	}{
		{"sys_enter_write", t.objs.ScEnterWrite}, {"sys_exit_write", t.objs.ScExitWrite},
		{"sys_enter_writev", t.objs.ScEnterWritev}, {"sys_exit_writev", t.objs.ScExitWritev},
		{"sys_enter_sendto", t.objs.ScEnterSendto}, {"sys_exit_sendto", t.objs.ScExitSendto},
		{"sys_enter_read", t.objs.ScEnterRead}, {"sys_exit_read", t.objs.ScExitRead},
		{"sys_enter_recvfrom", t.objs.ScEnterRecvfrom}, {"sys_exit_recvfrom", t.objs.ScExitRecvfrom},
		{"sys_enter_close", t.objs.ScEnterClose},
	}
	for _, p := range pairs {
		l, err := link.Tracepoint("syscalls", p.name, p.prog, nil)
		if err != nil {
			return fmt.Errorf("attach tracepoint %s: %w", p.name, err)
		}
		t.tps = append(t.tps, l)
	}
	return nil
}

// Watch makes the plain-HTTP syscall probes look at one process.
func (t *Tap) Watch(pid int) { _ = t.objs.Watched.Put(uint32(pid), uint8(1)) }

// Unwatch stops looking at a process (used after it exits).
func (t *Tap) Unwatch(pid int) { _ = t.objs.Watched.Delete(uint32(pid)) }

// ClearPlain forgets that a socket carries an HTTP exchange, so its next write is judged afresh.
func (t *Tap) ClearPlain(pid uint32, fd uint32) {
	type plainKey struct{ Pid, Fd uint32 }
	_ = t.objs.PlainConns.Delete(plainKey{pid, fd})
}

// Attach installs probes in every TLS provider of the process that is not
// attached yet, so it can be called again when a library is loaded later.
// Each provider file is all-or-nothing: on failure its partial probes are removed.
func (t *Tap) Attach(p inspect.Process) (int, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.links[p.PID] == nil {
		t.links[p.PID] = map[string][]link.Link{}
	}
	added := 0
	var firstErr error
	for _, pr := range p.Providers {
		if _, done := t.links[p.PID][pr.File]; done {
			continue
		}
		ls, err := t.attachFile(p.PID, pr)
		if err != nil {
			if firstErr == nil {
				firstErr = err
			}
			continue
		}
		t.links[p.PID][pr.File] = ls
		added++
	}
	if len(t.links[p.PID]) == 0 {
		delete(t.links, p.PID)
	}
	if added == 0 && firstErr == nil {
		return 0, nil
	}
	return added, firstErr
}

func (t *Tap) attachFile(pid int, pr inspect.Provider) ([]link.Link, error) {
	var ls []link.Link
	fail := func(err error) ([]link.Link, error) {
		for _, l := range ls {
			l.Close()
		}
		return nil, err
	}
	ex, err := link.OpenExecutable(pr.File)
	if err != nil {
		return fail(fmt.Errorf("%s: %w", pr.File, err))
	}
	opts := &link.UprobeOptions{PID: pid}
	for _, sym := range pr.Syms {
		prog := t.enterProg(sym)
		if prog == nil {
			continue
		}
		up, err := ex.Uprobe(sym, prog, opts)
		if err != nil {
			return fail(fmt.Errorf("uprobe %s in %s: %w", sym, pr.File, err))
		}
		ls = append(ls, up)
		ur, err := ex.Uretprobe(sym, t.objs.SslLeave, opts)
		if err != nil {
			return fail(fmt.Errorf("uretprobe %s in %s: %w", sym, pr.File, err))
		}
		ls = append(ls, ur)
	}
	if len(ls) == 0 {
		return nil, errors.New("no attachable symbols")
	}
	return ls, nil
}

func (t *Tap) enterProg(sym string) *ebpf.Program {
	switch sym {
	case "SSL_write":
		return t.objs.SslWriteEnter
	case "SSL_write_ex":
		return t.objs.SslWriteExEnter
	case "SSL_read":
		return t.objs.SslReadEnter
	case "SSL_read_ex":
		return t.objs.SslReadExEnter
	}
	return nil
}

// Detach removes the probes of one process (used after it exits).
func (t *Tap) Detach(pid int) {
	t.mu.Lock()
	defer t.mu.Unlock()
	closeAll(t.links[pid])
	delete(t.links, pid)
}

// IsAttached reports whether the process has at least one probe installed.
func (t *Tap) IsAttached(pid int) bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return len(t.links[pid]) > 0
}

// Attached lists the PIDs that currently have probes.
func (t *Tap) Attached() []int {
	t.mu.Lock()
	defer t.mu.Unlock()
	out := make([]int, 0, len(t.links))
	for pid := range t.links {
		out = append(out, pid)
	}
	return out
}

// Read blocks for the next event; it returns ringbuf.ErrClosed after Close.
func (t *Tap) Read() (Event, error) {
	rec, err := t.reader.Read()
	if err != nil {
		return Event{}, err
	}
	b := rec.RawSample
	if len(b) < hdrSize {
		return Event{}, fmt.Errorf("short event (%d bytes)", len(b))
	}
	le := binary.LittleEndian
	n := le.Uint32(b[28:32])
	if int(n) > len(b)-hdrSize {
		n = uint32(len(b) - hdrSize)
	}
	comm := b[32:48]
	if i := bytes.IndexByte(comm, 0); i >= 0 {
		comm = comm[:i]
	}
	return Event{
		SSL:   le.Uint64(b[0:8]),
		PID:   le.Uint32(b[8:12]),
		TID:   le.Uint32(b[12:16]),
		Dir:   le.Uint32(b[16:20]),
		Total: le.Uint32(b[20:24]),
		Off:   le.Uint32(b[24:28]),
		Comm:  string(comm),
		Time:  t.base.Add(time.Duration(le.Uint64(b[48:56]))),
		Data:  append([]byte(nil), b[hdrSize:hdrSize+int(n)]...),
	}, nil
}

// Dropped returns how many events the kernel side could not queue.
func (t *Tap) Dropped() uint64 {
	var v uint64
	_ = t.objs.Dropped.Lookup(uint32(0), &v)
	return v
}

// Close removes every probe and frees the program. Safe to call once.
func (t *Tap) Close() {
	t.once.Do(t.close)
}

func (t *Tap) close() {
	// Detaching waits for kernel grace periods, which add up when done one by one; overlap them.
	var wg sync.WaitGroup
	closeAsync := func(l link.Link) {
		wg.Add(1)
		go func() {
			defer wg.Done()
			l.Close()
		}()
	}
	for _, l := range t.tps {
		closeAsync(l)
	}
	t.mu.Lock()
	for pid, files := range t.links {
		for _, ls := range files {
			for _, l := range ls {
				closeAsync(l)
			}
		}
		delete(t.links, pid)
	}
	t.mu.Unlock()
	wg.Wait()
	t.reader.Close()
	t.objs.Close()
}

func closeAll(files map[string][]link.Link) {
	for _, ls := range files {
		for _, l := range ls {
			l.Close()
		}
	}
}

// LoadedPrograms returns how many tokprof TLS programs the kernel currently holds,
// whichever process loaded them. It needs the same privileges as loading.
func LoadedPrograms() (int, error) {
	n := 0
	var id ebpf.ProgramID
	for {
		next, err := ebpf.ProgramGetNextID(id)
		if err != nil {
			if errors.Is(err, os.ErrNotExist) {
				return n, nil
			}
			return n, err
		}
		id = next
		p, err := ebpf.NewProgramFromID(id)
		if err != nil {
			continue
		}
		info, err := p.Info()
		p.Close()
		if err == nil && (strings.HasPrefix(info.Name, "ssl_") || strings.HasPrefix(info.Name, "sc_")) {
			n++
		}
	}
}

// Counters returns the kernel-side debug counters: syscalls seen on watched processes, completed ones,
// sockets marked as HTTP POST, chunks emitted, writes that were not a POST, exits without an entry, and exits of unwatched tasks.
func (t *Tap) Counters() [16]uint64 {
	var out [16]uint64
	for i := range out {
		_ = t.objs.Counters.Lookup(uint32(i), &out[i])
	}
	return out
}
