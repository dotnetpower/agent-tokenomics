// SPDX-License-Identifier: Dual MIT/GPL
//
// Captures the plaintext handed to / returned from OpenSSL-compatible TLS
// libraries (SSL_write, SSL_read and their _ex variants) so tokprof can
// profile LLM traffic without changing the agent or its configuration.
//
// Entry probes remember the call arguments; one shared return probe reads the
// real byte count and copies that many bytes (and only those) to a ring buffer.
// Capturing on return avoids duplicates when a non-blocking write is retried.

#include <linux/bpf.h>
#include <linux/ptrace.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>

#define CHUNK 16384
#define MAX_CHUNKS 16

#define DIR_WRITE 0
#define DIR_READ 1

struct call_args {
	__u64 ssl;
	__u64 buf;
	__u64 num;
	__u64 out; // size_t* for the _ex variants
	__u32 dir;
	__u32 ex;
};

struct event_hdr {
	__u64 ssl;
	__u32 pid;
	__u32 tid;
	__u32 dir;
	__u32 total; // bytes the call moved
	__u32 off;   // offset of this chunk inside the call
	__u32 len;   // bytes carried by this event
	char comm[16];
	__u64 ts; // CLOCK_MONOTONIC ns at the end of the call
};

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 16384);
	__type(key, __u64);
	__type(value, struct call_args);
} active SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_RINGBUF);
	__uint(max_entries, 1 << 24);
} events SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_ARRAY);
	__uint(max_entries, 1);
	__type(key, __u32);
	__type(value, __u64);
} dropped SEC(".maps");

// PID namespace of the collector, so event PIDs match /proc as the collector sees it.
const volatile __u64 ns_dev;
const volatile __u64 ns_ino;

static __always_inline int enter(struct pt_regs *ctx, __u32 dir, __u32 ex)
{
	struct call_args a = {};
	__u64 id = bpf_get_current_pid_tgid();

	a.ssl = PT_REGS_PARM1(ctx);
	a.buf = PT_REGS_PARM2(ctx);
	a.num = PT_REGS_PARM3(ctx);
	a.out = PT_REGS_PARM4(ctx);
	a.dir = dir;
	a.ex = ex;
	bpf_map_update_elem(&active, &id, &a, BPF_ANY);
	return 0;
}

SEC("uprobe/ssl_write")
int ssl_write_enter(struct pt_regs *ctx) { return enter(ctx, DIR_WRITE, 0); }

SEC("uprobe/ssl_write_ex")
int ssl_write_ex_enter(struct pt_regs *ctx) { return enter(ctx, DIR_WRITE, 1); }

SEC("uprobe/ssl_read")
int ssl_read_enter(struct pt_regs *ctx) { return enter(ctx, DIR_READ, 0); }

SEC("uprobe/ssl_read_ex")
int ssl_read_ex_enter(struct pt_regs *ctx) { return enter(ctx, DIR_READ, 1); }

static __always_inline void count_drop(void)
{
	__u32 zero = 0;
	__u64 *v = bpf_map_lookup_elem(&dropped, &zero);

	if (v)
		__sync_fetch_and_add(v, 1);
}

// Each record is reserved in the ring buffer (not staged in a shared per-CPU
// buffer) so that a task preempted mid-call can never corrupt another's data.
// The reserve size must be a constant, hence the three size classes.
static __always_inline int emit_chunk(const struct call_args *a, __u64 id, __u32 total, __u32 off, __u64 len, const __u32 cap)
{
	struct event_hdr *rec = bpf_ringbuf_reserve(&events, sizeof(struct event_hdr) + cap, 0);

	if (!rec) {
		count_drop();
		return -1;
	}
	rec->ssl = a->ssl;
	rec->pid = id >> 32;
	rec->tid = (__u32)id;
	if (ns_ino) {
		struct bpf_pidns_info ns = {};

		if (!bpf_get_ns_current_pid_tgid(ns_dev, ns_ino, &ns, sizeof(ns))) {
			rec->pid = ns.tgid;
			rec->tid = ns.pid;
		}
	}
	rec->dir = a->dir;
	rec->total = total;
	rec->off = off;
	rec->len = len;
	rec->ts = bpf_ktime_get_ns();
	bpf_get_current_comm(&rec->comm, sizeof(rec->comm));
	if (len > cap)
		len = cap;
	// The compiler drops a plain mask it can prove redundant; the verifier cannot,
	// so force it to see 0 <= len <= cap.
	asm volatile("%0 &= 0x7fff" : "+r"(len));
	if (bpf_probe_read_user((char *)rec + sizeof(struct event_hdr), len, (void *)(a->buf + off))) {
		bpf_ringbuf_discard(rec, 0);
		return -1;
	}
	bpf_ringbuf_submit(rec, 0);
	return 0;
}

SEC("uretprobe/ssl_leave")
int ssl_leave(struct pt_regs *ctx)
{
	__u64 id = bpf_get_current_pid_tgid();
	struct call_args *ap = bpf_map_lookup_elem(&active, &id);
	struct call_args a;
	long rc = (int)PT_REGS_RC(ctx); // the C functions return int; upper register bits are undefined
	__u32 n = 0;

	if (!ap)
		return 0;
	a = *ap;
	bpf_map_delete_elem(&active, &id);

	if (a.ex) {
		__u64 v = 0;

		if (rc != 1)
			return 0;
		if (bpf_probe_read_user(&v, sizeof(v), (void *)a.out))
			return 0;
		if (v > 0x7fffffff)
			return 0;
		n = (__u32)v;
	} else {
		if (rc <= 0)
			return 0;
		n = (__u32)rc;
	}
	if (n == 0 || n > a.num)
		return 0;

#pragma unroll
	for (int i = 0; i < MAX_CHUNKS; i++) {
		__u32 off = (__u32)i * CHUNK;
		__u64 len;

		if (off >= n)
			break;
		len = n - off;
		if (len > CHUNK)
			len = CHUNK;
		if (len <= 512) {
			if (emit_chunk(&a, id, n, off, len, 512))
				break;
		} else if (len <= 4096) {
			if (emit_chunk(&a, id, n, off, len, 4096))
				break;
		} else {
			if (emit_chunk(&a, id, n, off, len, CHUNK))
				break;
		}
	}
	return 0;
}

char LICENSE[] SEC("license") = "Dual MIT/GPL";
