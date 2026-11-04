// Package inspect finds which running processes expose a TLS library we could observe.
//
// It needs no privileges beyond reading /proc entries the current user may read. It answers a
// feasibility question for the eBPF tap: for each process, is there a mapped ELF file that
// defines SSL_write and SSL_read (as an exported symbol or in the full symbol table)?
package inspect

import (
	"bufio"
	"debug/elf"
	"debug/gosym"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
)

var (
	sslSymbols  = []string{"SSL_write", "SSL_read", "SSL_write_ex", "SSL_read_ex"}
	containerRe = regexp.MustCompile(`(?:docker[-/]|containers/|-id\s+)([0-9a-f]{64})`)
)

const (
	GoTLSWriteSym = "crypto/tls.(*Conn).Write"
	GoTLSReadSym  = "crypto/tls.(*Conn).Read"
)

// Provider describes one mapped ELF file that defines TLS entry points.
type Provider struct {
	File       string              `json:"file"`
	Kind       string              `json:"kind"` // "exported" (dynsym) | "local" (symtab) | "go-tls" (Go crypto/tls)
	Syms       []string            `json:"symbols"`
	Addresses  map[string]uint64   `json:"addresses,omitempty"`
	RetOffsets map[string][]uint64 `json:"ret_offsets,omitempty"`
}

// Process is the result for one PID.
type Process struct {
	PID       int        `json:"pid"`
	Comm      string     `json:"comm"`
	Exe       string     `json:"exe"`
	Cwd       string     `json:"cwd,omitempty"`
	Container string     `json:"container,omitempty"`
	CgroupID  uint64     `json:"cgroup_id,omitempty"`
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
	s := NewScanner(procRoot)
	var out []Process
	for _, e := range entries {
		pid, err := strconv.Atoi(e.Name())
		if err != nil {
			continue
		}
		out = append(out, s.Process(pid))
	}
	sort.Slice(out, func(i, j int) bool { return out[i].PID < out[j].PID })
	return out, nil
}

// Scanner inspects processes incrementally and caches per-file results, so a
// long-running collector can look at new PIDs cheaply.
type Scanner struct {
	procRoot   string
	cache      map[string]*fileInfo
	containers map[string]string
}

func NewScanner(procRoot string) *Scanner {
	return &Scanner{procRoot: procRoot, cache: map[string]*fileInfo{}, containers: map[string]string{}}
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
	return scanProcess(s.procRoot, pid, s.cache, s.containers)
}

func scanProcess(procRoot string, pid int, cache map[string]*fileInfo, containers map[string]string) Process {
	base := filepath.Join(procRoot, strconv.Itoa(pid))
	p := Process{PID: pid, Attach: "no"}
	if b, err := os.ReadFile(filepath.Join(base, "comm")); err == nil {
		p.Comm = strings.TrimSpace(string(b))
	}
	p.Exe, _ = os.Readlink(filepath.Join(base, "exe"))
	p.Cwd, _ = os.Readlink(filepath.Join(base, "cwd"))
	if b, err := os.ReadFile(filepath.Join(base, "cmdline")); err == nil {
		s := strings.ReplaceAll(string(b), "\x00", " ")
		if len(s) > 1024 {
			s = s[:1024] + "..."
		}
		p.Cmdline = strings.TrimSpace(s)
	}
	p.Container, p.CgroupID = detectContainer(base, p.Cmdline, containers)

	f, err := os.Open(filepath.Join(base, "maps"))
	if err != nil {
		p.Attach, p.Note = "unreadable", "cannot read /proc/<pid>/maps (other user or restricted)"
		return p
	}
	defer f.Close()

	seen := map[string]bool{}
	selfMnt, _ := os.Readlink(filepath.Join(procRoot, "self", "ns", "mnt"))
	procMnt, _ := os.Readlink(filepath.Join(base, "ns", "mnt"))
	diffMnt := selfMnt != "" && procMnt != "" && selfMnt != procMnt
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1<<20), 1<<20)
	for sc.Scan() {
		fields := strings.Fields(sc.Text())
		if len(fields) < 6 || !strings.Contains(fields[1], "x") {
			continue
		}
		path := strings.Join(fields[5:], " ")
		if !strings.HasPrefix(path, "/") || strings.HasSuffix(path, "(deleted)") || seen[path] || isNonHTTPTLSLib(path) {
			continue
		}
		seen[path] = true
		openPath := resolveProcPath(base, path, diffMnt)
		ck := fileCacheKey(openPath)
		fi, ok := cache[ck]
		if !ok {
			fi = inspectFile(openPath)
			cache[ck] = fi
		}
		if fi.provider != nil {
			pr := *fi.provider
			pr.File = openPath
			p.Providers = append(p.Providers, pr)
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

func fileCacheKey(path string) string {
	if dev, ino, ok := statIno(path); ok {
		return fmt.Sprintf("%d:%d", dev, ino)
	}
	return path
}

func detectContainer(procBase, cmdline string, cache map[string]string) (string, uint64) {
	var cid string
	var cgID uint64
	if b, err := os.ReadFile(filepath.Join(procBase, "cgroup")); err == nil {
		if m := containerRe.FindSubmatch(b); m != nil {
			cid = string(m[1])
		}
		for _, line := range strings.Split(string(b), "\n") {
			if i := strings.Index(line, "::/"); i >= 0 {
				rel := strings.TrimSpace(line[i+2:])
				if _, ino, ok := statIno(filepath.Join("/sys/fs/cgroup", rel)); ok {
					cgID = ino
				}
				break
			}
		}
	}
	if cid == "" {
		if m := containerRe.FindStringSubmatch(cmdline); m != nil {
			cid = m[1]
		}
	}
	if cid == "" {
		return "", 0
	}
	if cgID == 0 {
		for _, cand := range []string{
			filepath.Join("/sys/fs/cgroup/system.slice", "docker-"+cid+".scope"),
			filepath.Join("/sys/fs/cgroup/docker", cid),
		} {
			if _, ino, ok := statIno(cand); ok {
				cgID = ino
				break
			}
		}
	}
	if name, ok := cache[cid]; ok {
		return name, cgID
	}
	name := cid[:12]
	if b, err := os.ReadFile(filepath.Join("/var/lib/docker/containers", cid, "config.v2.json")); err == nil {
		var cfg struct {
			Name string `json:"Name"`
		}
		if json.Unmarshal(b, &cfg) == nil && cfg.Name != "" {
			name = strings.TrimPrefix(cfg.Name, "/") + " (" + cid[:12] + ")"
		}
	} else if out, err := exec.Command("docker", "inspect", "--format", "{{.Name}}", cid).Output(); err == nil {
		if s := strings.TrimSpace(strings.TrimPrefix(string(out), "/")); s != "" {
			name = s + " (" + cid[:12] + ")"
		}
	}
	cache[cid] = name
	return name, cgID
}

// resolveProcPath returns a host-openable path for a file mapped inside pid, falling back to
// /proc/<pid>/root/<path> when the process runs in another mount namespace (such as a container).
func resolveProcPath(procBase, path string, diffMnt bool) string {
	rootPath := filepath.Join(procBase, "root", strings.TrimPrefix(path, "/"))
	if diffMnt {
		if _, err := os.Stat(rootPath); err == nil {
			return rootPath
		}
	}
	if _, err := os.Stat(path); err == nil {
		return path
	}
	if _, err := os.Stat(rootPath); err == nil {
		return rootPath
	}
	return path
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
	var goWrite, goRead *elf.Symbol
	if sym, err := ef.Symbols(); err == nil {
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
	}
	if goWrite == nil || goRead == nil {
		if w, r := findGoSymbolsPCLN(ef); w != nil && r != nil {
			fi.goTLS = true
			goWrite, goRead = w, r
		}
	}
	if found["SSL_write"] == "" && goWrite != nil && goRead != nil {
		wRets := findGoRetOffsets(ef, *goWrite)
		rRets := findGoRetOffsets(ef, *goRead)
		wAddr := fileOffset(ef, goWrite.Value)
		rAddr := fileOffset(ef, goRead.Value)
		if len(wRets) > 0 && len(rRets) > 0 && wAddr > 0 && rAddr > 0 {
			fi.provider = &Provider{
				File:       path,
				Kind:       "go-tls",
				Syms:       []string{GoTLSWriteSym, GoTLSReadSym},
				Addresses:  map[string]uint64{GoTLSWriteSym: wAddr, GoTLSReadSym: rAddr},
				RetOffsets: map[string][]uint64{GoTLSWriteSym: wRets, GoTLSReadSym: rRets},
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

// findGoSymbolsPCLN recovers crypto/tls.(*Conn).Write and Read from .gopclntab when a Go binary was
// built with -ldflags="-s -w" (no .symtab).
func findGoSymbolsPCLN(ef *elf.File) (*elf.Symbol, *elf.Symbol) {
	textIdx := -1
	var textSec, pclnSec *elf.Section
	for i, s := range ef.Sections {
		switch s.Name {
		case ".text":
			textIdx, textSec = i, s
		case ".gopclntab", ".data.rel.ro.gopclntab":
			pclnSec = s
		}
	}
	if textSec == nil || pclnSec == nil || pclnSec.Size == 0 || pclnSec.Size > 64<<20 {
		return nil, nil
	}
	pcln, err := pclnSec.Data()
	if err != nil {
		return nil, nil
	}
	tab, err := gosym.NewTable(nil, gosym.NewLineTable(pcln, textSec.Addr))
	if err != nil {
		return nil, nil
	}
	sym := func(name string) *elf.Symbol {
		fn := tab.LookupFunc(name)
		if fn == nil || fn.End <= fn.Entry {
			return nil
		}
		return &elf.Symbol{
			Name:    name,
			Value:   fn.Entry,
			Size:    fn.End - fn.Entry,
			Section: elf.SectionIndex(textIdx),
		}
	}
	return sym(GoTLSWriteSym), sym(GoTLSReadSym)
}

func fileOffset(ef *elf.File, va uint64) uint64 {
	for _, prog := range ef.Progs {
		if prog.Type == elf.PT_LOAD && (prog.Flags&elf.PF_X) != 0 && prog.Vaddr <= va && va < prog.Vaddr+prog.Memsz {
			return va - prog.Vaddr + prog.Off
		}
	}
	return 0
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

// isNonHTTPTLSLib filters out embedded OpenSSL copies inside database, image, or
// crypto-primitive wheels that never carry HTTP(S) client traffic.
func isNonHTTPTLSLib(path string) bool {
	for _, sub := range []string{
		"pyarrow/libarrow",
		"opencv_python.libs/",
		"psycopg_binary.libs/",
		"cryptography/hazmat/bindings/_rust",
		"@esbuild/",
	} {
		if strings.Contains(path, sub) {
			return true
		}
	}
	return false
}
