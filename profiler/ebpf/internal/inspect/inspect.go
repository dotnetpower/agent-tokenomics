// Package inspect finds which running processes expose a TLS library we could observe.
//
// It needs no privileges beyond reading /proc entries the current user may read. It answers a
// feasibility question for the eBPF tap: for each process, is there a mapped ELF file that
// defines SSL_write and SSL_read (as an exported symbol or in the full symbol table)?
package inspect

import (
	"bufio"
	"debug/elf"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

var sslSymbols = []string{"SSL_write", "SSL_read", "SSL_write_ex", "SSL_read_ex"}

// Provider describes one mapped ELF file that defines TLS entry points.
type Provider struct {
	File string   `json:"file"`
	Kind string   `json:"kind"` // "exported" (dynsym) | "local" (symtab only, e.g. statically linked)
	Syms []string `json:"symbols"`
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
		p.Attach, p.Note = "go-only", "Go crypto/tls: needs a Go-specific probe (not implemented)"
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
		for _, s := range sym {
			if isTLS(s.Name) && s.Section != elf.SHN_UNDEF && elf.ST_TYPE(s.Info) == elf.STT_FUNC {
				if _, ok := found[s.Name]; !ok {
					found[s.Name] = "local"
				}
			}
			if s.Name == "crypto/tls.(*Conn).Write" {
				fi.goTLS = true
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

func isTLS(name string) bool {
	for _, n := range sslSymbols {
		if name == n {
			return true
		}
	}
	return false
}
