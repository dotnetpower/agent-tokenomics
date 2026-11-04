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

// Copies n bytes of a user buffer to the ring buffer in up to MAX_CHUNKS records.
// A real function (not inlined) so the unrolled loop exists once for all programs.
static __noinline int emit_all(const struct call_args *a, __u64 id, __u32 n)
{
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
			if (emit_chunk(a, id, n, off, len, 512))
				break;
		} else if (len <= 4096) {
			if (emit_chunk(a, id, n, off, len, 4096))
				break;
		} else {
			if (emit_chunk(a, id, n, off, len, CHUNK))
				break;
		}
	}
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

	emit_all(&a, id, n);
	return 0;
}


// ---- plain HTTP (no TLS): a few syscalls on sockets that carry an HTTP POST ----------------------------------------
//
// TLS traffic is read through the uprobes above. Cleartext HTTP (for example an on-prem vLLM on http://host:8000)
// never passes through a TLS library, so the write/read syscalls are observed instead. To keep the cost near zero,
// only processes the collector asked for are looked at, and a socket is copied only after a write that starts with
// "POST " has marked it; the collector unmarks it when the exchange is complete, and close() unmarks it too.

const volatile __u32 self_tgid;
const volatile __u32 watch_all;

struct sys_enter_ctx {
	__u64 pad;
	__s64 id;
	__u64 args[6];
};

struct sys_exit_ctx {
	__u64 pad;
	__s64 id;
	__s64 ret;
};

struct plain_key {
	__u32 pid;
	__u32 fd;
};

struct sys_args {
	__u64 buf; // buffer, or the iovec array for writev
	__u64 len; // byte count, or the number of iovecs for writev
	__u32 fd;
	__u32 dir;
	__u32 vec;
};

struct iov {
	__u64 base;
	__u64 len;
};

#define MAX_IOV 4

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 8192);
	__type(key, __u32);
	__type(value, __u8);
} watched SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 16384);
	__type(key, struct plain_key);
	__type(value, __u8);
} plain_conns SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 16384);
	__type(key, __u64);
	__type(value, struct sys_args);
} sys_active SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_ARRAY);
	__uint(max_entries, 8);
	__type(key, __u32);
	__type(value, __u64);
} counters SEC(".maps");

static __always_inline void bump(__u32 idx)
{
	__u64 *v = bpf_map_lookup_elem(&counters, &idx);

	if (v)
		__sync_fetch_and_add(v, 1);
}

// Looks the current process up in the collector's PID namespace: that is where /proc, and therefore the
// collector's idea of a PID, lives, and it can differ from the kernel's global numbering (WSL, containers).
static __always_inline int watching(__u32 *tgid)
{
	struct bpf_pidns_info ns = {};

	*tgid = bpf_get_current_pid_tgid() >> 32;
	if (ns_ino) {
		if (bpf_get_ns_current_pid_tgid(ns_dev, ns_ino, &ns, sizeof(ns)))
			return 0; // not visible from the collector's namespace
		*tgid = ns.tgid;
	}
	if (*tgid == self_tgid)
		return 0;
	return watch_all || bpf_map_lookup_elem(&watched, tgid);
}

static __always_inline int sys_enter(struct sys_enter_ctx *c, __u32 dir, __u32 vec)
{
	__u64 id = bpf_get_current_pid_tgid();
	struct sys_args a = {};
	__u32 tgid;

	if (!watching(&tgid))
		return 0;
	bump(0);
	a.fd = (__u32)c->args[0];
	a.buf = c->args[1];
	a.len = c->args[2];
	a.dir = dir;
	a.vec = vec;
	bpf_map_update_elem(&sys_active, &id, &a, BPF_ANY);
	return 0;
}

static __always_inline int sys_exit(struct sys_exit_ctx *c)
{
	__u64 id = bpf_get_current_pid_tgid();
	struct sys_args *ap = bpf_map_lookup_elem(&sys_active, &id);
	struct sys_args a;
	struct plain_key k = {};
	struct call_args ca = {};
	__u8 *marked;
	__u32 tgid;
	long ret = c->ret;

	if (!ap)
		return 0; // the entry was filtered out (not a watched process): by far the common case
	bump(1);
	a = *ap;
	bpf_map_delete_elem(&sys_active, &id);
	if (ret <= 0 || ret > 0x7fffffff || (!a.vec && (__u64)ret > a.len))
		return 0;
	if (!watching(&tgid)) {
		bump(6);
		return 0;
	}
	k.pid = tgid;
	k.fd = a.fd;
	marked = bpf_map_lookup_elem(&plain_conns, &k);
	if (a.dir == DIR_WRITE && !marked) {
		char head[5];
		__u8 one = 1;
		__u64 first = a.buf;

		if (a.vec) {
			struct iov v = {};

			if (bpf_probe_read_user(&v, sizeof(v), (void *)a.buf) || v.len < sizeof(head))
				return 0;
			first = v.base;
		}
		if (bpf_probe_read_user(head, sizeof(head), (void *)first))
			return 0;
		if (head[0] != 'P' || head[1] != 'O' || head[2] != 'S' || head[3] != 'T' || head[4] != ' ') {
			bump(4); // a write on a watched process that does not start an HTTP POST (usual: logs, files, other protocols)
			return 0;
		}
		bpf_map_update_elem(&plain_conns, &k, &one, BPF_ANY);
		bump(2);
	} else if (!marked) {
		return 0;
	}
	ca.ssl = (1ULL << 63) | a.fd; // plaintext connections are told apart from TLS ones by the top bit
	ca.dir = a.dir;
	bump(3);
	if (!a.vec) {
		ca.buf = a.buf;
		ca.num = a.len;
		emit_all(&ca, id, (__u32)ret);
		return 0;
	}
#pragma unroll
	for (int i = 0; i < MAX_IOV; i++) {
		struct iov v = {};
		__u64 n;

		if (i >= a.len || ret <= 0)
			break;
		if (bpf_probe_read_user(&v, sizeof(v), (void *)(a.buf + (__u64)i * sizeof(v))))
			break;
		n = v.len < (__u64)ret ? v.len : (__u64)ret;
		if (n == 0)
			continue;
		ca.buf = v.base;
		ca.num = n;
		emit_all(&ca, id, (__u32)n);
		ret -= n;
	}
	return 0;
}

SEC("tracepoint/syscalls/sys_enter_write")
int sc_enter_write(struct sys_enter_ctx *c) { return sys_enter(c, DIR_WRITE, 0); }

SEC("tracepoint/syscalls/sys_enter_sendto")
int sc_enter_sendto(struct sys_enter_ctx *c) { return sys_enter(c, DIR_WRITE, 0); }

SEC("tracepoint/syscalls/sys_enter_writev")
int sc_enter_writev(struct sys_enter_ctx *c) { return sys_enter(c, DIR_WRITE, 1); }

SEC("tracepoint/syscalls/sys_exit_writev")
int sc_exit_writev(struct sys_exit_ctx *c) { return sys_exit(c); }

SEC("tracepoint/syscalls/sys_enter_read")
int sc_enter_read(struct sys_enter_ctx *c) { return sys_enter(c, DIR_READ, 0); }

SEC("tracepoint/syscalls/sys_enter_recvfrom")
int sc_enter_recvfrom(struct sys_enter_ctx *c) { return sys_enter(c, DIR_READ, 0); }

SEC("tracepoint/syscalls/sys_exit_write")
int sc_exit_write(struct sys_exit_ctx *c) { return sys_exit(c); }

SEC("tracepoint/syscalls/sys_exit_sendto")
int sc_exit_sendto(struct sys_exit_ctx *c) { return sys_exit(c); }

SEC("tracepoint/syscalls/sys_exit_read")
int sc_exit_read(struct sys_exit_ctx *c) { return sys_exit(c); }

SEC("tracepoint/syscalls/sys_exit_recvfrom")
int sc_exit_recvfrom(struct sys_exit_ctx *c) { return sys_exit(c); }

SEC("tracepoint/syscalls/sys_enter_close")
int sc_enter_close(struct sys_enter_ctx *c)
{
	struct plain_key k = {};

	if (!watching(&k.pid))
		return 0;
	k.fd = (__u32)c->args[0];
	bpf_map_delete_elem(&plain_conns, &k);
	return 0;
}

char LICENSE[] SEC("license") = "Dual MIT/GPL";
