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
	objs      tlsObjects
	reader    *ringbuf.Reader
	mu        sync.Mutex
	fileLinks map[string][]link.Link
	pidFiles  map[int]map[string]bool
	once      sync.Once
	tps       []link.Link // syscall tracepoints for plain HTTP
	base      time.Time   // wall-clock time of CLOCK_MONOTONIC == 0
}

// Options tune what the kernel side watches.
type Options struct {
	WatchAll  bool   // observe plain HTTP in every process (otherwise only those added with Watch)
	NoPlain   bool   // do not attach the syscall tracepoints at all
	IgnorePID uint32 // extra PID (such as the tokprof gateway) to ignore
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
	t := &Tap{
		base:      time.Now().Add(-time.Duration(ts.Nano())),
		fileLinks: map[string][]link.Link{},
		pidFiles:  map[int]map[string]bool{},
	}
	spec, err := loadTls()
	if err != nil {
		return nil, fmt.Errorf("load eBPF spec: %w", err)
	}
	_ = spec.Variables["self_tgid"].Set(uint32(os.Getpid()))
	if opts.IgnorePID != 0 {
		_ = spec.Variables["ignore_tgid"].Set(opts.IgnorePID)
	}
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
	for _, rt := range []struct {
		name string
		prog *ebpf.Program
	}{{"sys_enter", t.objs.RawSysEnter}, {"sys_exit", t.objs.RawSysExit}} {
		l, err := link.AttachRawTracepoint(link.RawTracepointOptions{Name: rt.name, Program: rt.prog})
		if err != nil {
			return fmt.Errorf("attach raw tracepoint %s: %w", rt.name, err)
		}
		t.tps = append(t.tps, l)
	}
	return nil
}

// Watch makes the probes look at one process.
func (t *Tap) Watch(pid int) { _ = t.objs.Watched.Put(uint32(pid), uint8(1)) }

// WatchCgroup makes the probes look at every process in a container cgroup (including short-lived docker exec children).
func (t *Tap) WatchCgroup(cgroupID uint64) {
	if cgroupID != 0 {
		_ = t.objs.WatchedCgroups.Put(cgroupID, uint8(1))
	}
}

// Unwatch stops looking at a process (used after it exits).
func (t *Tap) Unwatch(pid int) { _ = t.objs.Watched.Delete(uint32(pid)) }

// ClearPlain forgets that a socket carries an HTTP exchange, so its next write is judged afresh.
func (t *Tap) ClearPlain(pid uint32, fd uint32) {
	type plainKey struct{ Pid, Fd uint32 }
	_ = t.objs.PlainConns.Delete(plainKey{pid, fd})
}

func inodeKey(path string) string {
	var st syscall.Stat_t
	if err := syscall.Stat(path, &st); err == nil {
		return fmt.Sprintf("%d:%d", st.Dev, st.Ino)
	}
	return path
}

// Attach installs probes in every TLS provider of the process whose underlying
// file inode is not attached yet. Uprobes are attached once per unique inode
// (PID 0) and filtered in BPF by watching(&tgid), which avoids hundreds of
// duplicate perf_event attachments across processes that share libssl or python.
func (t *Tap) Attach(p inspect.Process) (int, error) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.pidFiles[p.PID] == nil {
		t.pidFiles[p.PID] = map[string]bool{}
	}
	added := 0
	var firstErr error
	for _, pr := range p.Providers {
		ikey := inodeKey(pr.File)
		if t.pidFiles[p.PID][ikey] {
			continue
		}
		if _, ok := t.fileLinks[ikey]; !ok {
			ls, err := t.attachFile(0, pr)
			if err != nil {
				if firstErr == nil {
					firstErr = err
				}
				continue
			}
			t.fileLinks[ikey] = ls
		}
		t.pidFiles[p.PID][ikey] = true
		added++
	}
	if len(t.pidFiles[p.PID]) == 0 {
		delete(t.pidFiles, p.PID)
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
	if pr.Kind == "go-tls" {
		for _, g := range []struct {
			sym        string
			enter, ret *ebpf.Program
		}{
			{inspect.GoTLSWriteSym, t.objs.GoTlsWriteEnter, t.objs.GoTlsWriteRet},
			{inspect.GoTLSReadSym, t.objs.GoTlsReadEnter, t.objs.GoTlsReadRet},
		} {
			rets := pr.RetOffsets[g.sym]
			addr := pr.Addresses[g.sym]
			if len(rets) == 0 {
				continue
			}
			up, err := ex.Uprobe(g.sym, g.enter, &link.UprobeOptions{PID: pid, Address: addr})
			if err != nil {
				return fail(fmt.Errorf("uprobe %s in %s: %w", g.sym, pr.File, err))
			}
			ls = append(ls, up)
			for _, off := range rets {
				rp, err := ex.Uprobe(g.sym, g.ret, &link.UprobeOptions{PID: pid, Address: addr, Offset: off})
				if err != nil {
					return fail(fmt.Errorf("uprobe %s+%#x in %s: %w", g.sym, off, pr.File, err))
				}
				ls = append(ls, rp)
			}
		}
		if len(ls) == 0 {
			return nil, errors.New("no attachable Go crypto/tls symbols")
		}
		return ls, nil
	}
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

// Detach removes a process from the tracked set after it exits.
func (t *Tap) Detach(pid int) {
	t.mu.Lock()
	defer t.mu.Unlock()
	delete(t.pidFiles, pid)
}

// IsAttached reports whether the process has at least one probe installed.
func (t *Tap) IsAttached(pid int) bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return len(t.pidFiles[pid]) > 0
}

// Attached lists the PIDs that currently have probes.
func (t *Tap) Attached() []int {
	t.mu.Lock()
	defer t.mu.Unlock()
	out := make([]int, 0, len(t.pidFiles))
	for pid := range t.pidFiles {
		out = append(out, pid)
	}
	return out
}

// AttachedFiles returns the number of unique TLS binaries/libraries currently hooked.
func (t *Tap) AttachedFiles() int {
	t.mu.Lock()
	defer t.mu.Unlock()
	return len(t.fileLinks)
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

// CloseReader stops the ring-buffer reader so the consumer loop can drain and flush
// output immediately before the slower kernel uprobe detach begins.
func (t *Tap) CloseReader() {
	_ = t.reader.Close()
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
	for k, ls := range t.fileLinks {
		for _, l := range ls {
			closeAsync(l)
		}
		delete(t.fileLinks, k)
	}
	t.pidFiles = map[int]map[string]bool{}
	t.mu.Unlock()
	wg.Wait()
	t.reader.Close()
	t.objs.Close()
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
		if err == nil && (strings.HasPrefix(info.Name, "ssl_") || strings.HasPrefix(info.Name, "go_tls_") || strings.HasPrefix(info.Name, "raw_sys_")) {
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
