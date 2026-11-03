// Command tokprof-ebpf is the Linux helper for tokprof.
//
// Subcommands:
//
//	inspect   list which running processes expose a TLS library the eBPF tap could attach to
//	          (no privileges needed beyond what the current user may read in /proc)
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"strings"
	"text/tabwriter"

	"github.com/dotnetpower/agent-tokenomics/profiler/ebpf/internal/inspect"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(2)
	}
	switch os.Args[1] {
	case "inspect":
		runInspect(os.Args[2:])
	default:
		usage()
		os.Exit(2)
	}
}

func usage() {
	fmt.Fprintln(os.Stderr, "usage: tokprof-ebpf inspect [--all] [--json]")
}

func runInspect(args []string) {
	fs := flag.NewFlagSet("inspect", flag.ExitOnError)
	all := fs.Bool("all", false, "show every process, not only those with TLS libraries or Go TLS")
	asJSON := fs.Bool("json", false, "machine-readable output")
	_ = fs.Parse(args)

	procs, err := inspect.Scan("/proc")
	if err != nil {
		fmt.Fprintln(os.Stderr, "scan failed:", err)
		os.Exit(1)
	}
	var shown []inspect.Process
	counts := map[string]int{}
	for _, p := range procs {
		counts[p.Attach]++
		if *all || p.Attach == "yes" || p.Attach == "go-only" {
			shown = append(shown, p)
		}
	}
	if *asJSON {
		_ = json.NewEncoder(os.Stdout).Encode(map[string]any{"counts": counts, "processes": shown})
		return
	}
	tw := tabwriter.NewWriter(os.Stdout, 0, 4, 2, ' ', 0)
	fmt.Fprintln(tw, "PID\tCOMM\tATTACH\tTLS PROVIDER\tKIND")
	for _, p := range shown {
		if len(p.Providers) == 0 {
			fmt.Fprintf(tw, "%d\t%s\t%s\t%s\t\n", p.PID, p.Comm, p.Attach, p.Note)
			continue
		}
		for i, pr := range p.Providers {
			pid, comm, att := "", "", ""
			if i == 0 {
				pid, comm, att = fmt.Sprint(p.PID), p.Comm, p.Attach
			}
			fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\n", pid, comm, att, pr.File, pr.Kind)
		}
	}
	_ = tw.Flush()
	fmt.Printf("\nprocesses: %d total, %d attachable, %d Go-only, %d without a visible TLS library, %d unreadable\n",
		len(procs), counts["yes"], counts["go-only"], counts["no"], counts["unreadable"])
	if counts["yes"] > 0 {
		var kinds []string
		for _, p := range shown {
			for _, pr := range p.Providers {
				kinds = append(kinds, pr.Kind)
			}
		}
		fmt.Printf("provider kinds seen: %s\n", strings.Join(uniq(kinds), ", "))
	}
}

func uniq(in []string) []string {
	seen := map[string]bool{}
	var out []string
	for _, s := range in {
		if !seen[s] {
			seen[s] = true
			out = append(out, s)
		}
	}
	return out
}
