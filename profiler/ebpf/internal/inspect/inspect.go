// Package inspect finds which running processes expose a TLS library we could observe.
//
// It needs no privileges beyond reading /proc entries the current user may read. It answers a
// feasibility question for the eBPF tap: for each process, is there a mapped ELF file that
// defines SSL_write and SSL_read (as an exported symbol or in the full symbol table)?
package inspect

import (
	"bufio"
	"debug/elf"
	"encoding/binary"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

var sslSymbols = []string{"SSL_write", "SSL_read", "SSL_write_ex", "SSL_read_ex"}

const (
	GoTLSWriteSym = "crypto/tls.(*Conn).Write"
	GoTLSReadSym  = "crypto/tls.(*Conn).Read"
)

// Provider describes one mapped ELF file that defines TLS entry points.
type Provider struct {
	File       string              `json:"file"`
	Kind       string              `json:"kind"` // "exported" (dynsym) | "local" (symtab) | "go-tls" (Go crypto/tls)
	Syms       []string            `json:"symbols"`
	RetOffsets map[string][]uint64 `json:"ret_offsets,omitempty"`
}

// Process is the result for one PID.
type Process struct {
	PID       int        `json:"pid"`
	Comm      string     `json:"comm"`
	Exe       string     `json:"exe"`
	Cmdline   string     `json:"cmdline"`
	Providers []Provider `json:"providers,omitempty"`
	GoTLS     bool       `json:"go_tls,omitempty"`
	Attach    string     `json:"attach"` // "yes" | "go-only" | "no" | "unreadable"
	Note      string     `json:"note,omitempty"`
}

type fileInfo struct {
	provider *Provider
	goTLS    bool
	err      error
}

// Scan inspects every process under procRoot (normally /proc).
func Scan(procRoot string) ([]Process, error) {
	entries, err := os.ReadDir(procRoot)
	if err != nil {
		return nil, err
	}
	cache := map[string]*fileInfo{}
	var out []Process
	for _, e := range entries {
		pid, err := strconv.Atoi(e.Name())
		if err != nil {
			continue
		}
		out = append(out, scanProcess(procRoot, pid, cache))
	}
	sort.Slice(out, func(i, j int) bool { return out[i].PID < out[j].PID })
	return out, nil
}

// Scanner inspects processes incrementally and caches per-file results, so a
// long-running collector can look at new PIDs cheaply.
type Scanner struct {
	procRoot string
	cache    map[string]*fileInfo
}

func NewScanner(procRoot string) *Scanner {
	return &Scanner{procRoot: procRoot, cache: map[string]*fileInfo{}}
}

// PIDs lists the numeric entries under the proc root.
func (s *Scanner) PIDs() []int {
	entries, err := os.ReadDir(s.procRoot)
	if err != nil {
		return nil
	}
	var pids []int
	for _, e := range entries {
		if pid, err := strconv.Atoi(e.Name()); err == nil {
			pids = append(pids, pid)
		}
	}
	return pids
}

// Signature is a cheap fingerprint (name and executable) that changes when a
// forked child execs into another program or a PID is reused.
func (s *Scanner) Signature(pid int) string {
	base := filepath.Join(s.procRoot, strconv.Itoa(pid))
	comm, _ := os.ReadFile(filepath.Join(base, "comm"))
	exe, _ := os.Readlink(filepath.Join(base, "exe"))
	return strings.TrimSpace(string(comm)) + "|" + exe
}

// Process inspects one PID.
func (s *Scanner) Process(pid int) Process {
	return scanProcess(s.procRoot, pid, s.cache)
}

func scanProcess(procRoot string, pid int, cache map[string]*fileInfo) Process {
	base := filepath.Join(procRoot, strconv.Itoa(pid))
	p := Process{PID: pid, Attach: "no"}
	if b, err := os.ReadFile(filepath.Join(base, "comm")); err == nil {
		p.Comm = strings.TrimSpace(string(b))
	}
	p.Exe, _ = os.Readlink(filepath.Join(base, "exe"))
	if b, err := os.ReadFile(filepath.Join(base, "cmdline")); err == nil {
		s := strings.ReplaceAll(string(b), "\x00", " ")
		if len(s) > 120 {
			s = s[:120] + "..."
		}
		p.Cmdline = strings.TrimSpace(s)
	}

	f, err := os.Open(filepath.Join(base, "maps"))
	if err != nil {
		p.Attach, p.Note = "unreadable", "cannot read /proc/<pid>/maps (other user or restricted)"
		return p
	}
	defer f.Close()

	seen := map[string]bool{}
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1<<20), 1<<20)
	for sc.Scan() {
		fields := strings.Fields(sc.Text())
		if len(fields) < 6 || !strings.Contains(fields[1], "x") {
			continue
		}
		path := strings.Join(fields[5:], " ")
		if !strings.HasPrefix(path, "/") || strings.HasSuffix(path, "(deleted)") || seen[path] {
			continue
		}
		seen[path] = true
		fi, ok := cache[path]
		if !ok {
			fi = inspectFile(path)
			cache[path] = fi
		}
		if fi.provider != nil {
			p.Providers = append(p.Providers, *fi.provider)
		}
		if fi.goTLS {
			p.GoTLS = true
		}
	}
	switch {
	case len(p.Providers) > 0:
		p.Attach = "yes"
	case p.GoTLS:
		p.Attach, p.Note = "go-only", "Go crypto/tls: RET offsets not found (unsupported ABI or architecture)"
	}
	return p
}

func inspectFile(path string) *fileInfo {
	fi := &fileInfo{}
	ef, err := elf.Open(path)
	if err != nil {
		fi.err = err
		return fi
	}
	defer ef.Close()

	found := map[string]string{} // symbol -> kind
	if dyn, err := ef.DynamicSymbols(); err == nil {
		for _, s := range dyn {
			if isTLS(s.Name) && s.Section != elf.SHN_UNDEF && elf.ST_TYPE(s.Info) == elf.STT_FUNC {
				found[s.Name] = "exported"
			}
		}
	}
	if sym, err := ef.Symbols(); err == nil {
		var goWrite, goRead *elf.Symbol
		for i := range sym {
			s := &sym[i]
			if isTLS(s.Name) && s.Section != elf.SHN_UNDEF && elf.ST_TYPE(s.Info) == elf.STT_FUNC {
				if _, ok := found[s.Name]; !ok {
					found[s.Name] = "local"
				}
			}
			switch s.Name {
			case GoTLSWriteSym:
				fi.goTLS = true
				goWrite = s
			case GoTLSReadSym:
				goRead = s
			}
		}
		if found["SSL_write"] == "" && goWrite != nil && goRead != nil {
			wRets := findGoRetOffsets(ef, *goWrite)
			rRets := findGoRetOffsets(ef, *goRead)
			if len(wRets) > 0 && len(rRets) > 0 {
				fi.provider = &Provider{
					File:       path,
					Kind:       "go-tls",
					Syms:       []string{GoTLSWriteSym, GoTLSReadSym},
					RetOffsets: map[string][]uint64{GoTLSWriteSym: wRets, GoTLSReadSym: rRets},
				}
			}
		}
	}
	if found["SSL_write"] != "" && found["SSL_read"] != "" {
		kind := "exported"
		var names []string
		for n, k := range found {
			names = append(names, n)
			if k == "local" {
				kind = "local"
			}
		}
		sort.Strings(names)
		fi.provider = &Provider{File: path, Kind: kind, Syms: names}
	}
	return fi
}

// findGoRetOffsets locates the RET instructions in a Go function so uprobes can be
// placed at function return without using uretprobe (which rewrites stack return
// addresses and breaks Go stack growth / scanning).
func findGoRetOffsets(ef *elf.File, s elf.Symbol) []uint64 {
	if s.Size == 0 || int(s.Section) >= len(ef.Sections) {
		return nil
	}
	sec := ef.Sections[s.Section]
	if s.Value < sec.Addr {
		return nil
	}
	buf := make([]byte, s.Size)
	if _, err := sec.ReadAt(buf, int64(s.Value-sec.Addr)); err != nil {
		return nil
	}
	var out []uint64
	switch ef.Machine {
	case elf.EM_X86_64:
		for i := 5; i < len(buf); i++ {
			// Go amd64 frame epilogue: add $imm8/$imm32, %rsp; pop %rbp (0x5d); ret (0xc3)
			if buf[i] == 0xc3 && buf[i-1] == 0x5d &&
				((buf[i-5] == 0x48 && buf[i-4] == 0x83 && buf[i-3] == 0xc4) ||
					(i >= 8 && buf[i-8] == 0x48 && buf[i-7] == 0x81 && buf[i-6] == 0xc4)) {
				out = append(out, uint64(i))
			}
		}
	case elf.EM_AARCH64:
		for i := 0; i+4 <= len(buf); i += 4 {
			if binary.LittleEndian.Uint32(buf[i:i+4]) == 0xd65f03c0 {
				out = append(out, uint64(i))
			}
		}
	}
	return out
}

func isTLS(name string) bool {
	for _, n := range sslSymbols {
		if name == n {
			return true
		}
	}
	return false
}
