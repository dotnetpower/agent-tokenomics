//go:build linux

package tap

//go:generate go run github.com/cilium/ebpf/cmd/bpf2go -cc clang -cflags "-O2 -g -Wall" -target amd64 tls ../../bpf/tls.c -- -I/usr/include/x86_64-linux-gnu
