//go:build !(linux && amd64)

package main

import (
	"fmt"
	"os"
)

func runTrace(_ []string) {
	fmt.Fprintln(os.Stderr, "tokprof-ebpf: trace is only available on Linux amd64")
	os.Exit(1)
}
