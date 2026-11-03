//go:build linux

package tap

// Cross-architecture headers: apt install libc6-dev-arm64-cross (provides /usr/aarch64-linux-gnu/include).
//go:generate go run github.com/cilium/ebpf/cmd/bpf2go -cc clang -cflags "-O2 -g -Wall" -target amd64 tls ../../bpf/tls.c -- -I/usr/include/x86_64-linux-gnu
//go:generate go run github.com/cilium/ebpf/cmd/bpf2go -cc clang -cflags "-O2 -g -Wall" -target arm64 tls ../../bpf/tls.c -- -I/usr/aarch64-linux-gnu/include
