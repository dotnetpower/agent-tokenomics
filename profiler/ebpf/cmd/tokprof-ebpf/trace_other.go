//go:build !(linux && (amd64 || arm64))

package main

import (
	"fmt"
	"os"
)

func runTrace(_ []string) {
	fmt.Fprintln(os.Stderr, "tokprof-ebpf: trace is only available on Linux amd64 and arm64")
	os.Exit(1)
}

func runStatus() {
	fmt.Fprintln(os.Stderr, "tokprof-ebpf: status is only available on Linux amd64 and arm64")
	os.Exit(1)
}
