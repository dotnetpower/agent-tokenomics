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
#define DIR_FORK 2

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

#if defined(__TARGET_ARCH_x86)
#define GO_PARAM1(x) ((x)->rax)
#define GO_PARAM2(x) ((x)->rbx)
#define GO_PARAM3(x) ((x)->rcx)
#define GO_G(x)      ((x)->r14)
#elif defined(__TARGET_ARCH_arm64)
#define GO_PARAM1(x) (((PT_REGS_ARM64 *)(x))->regs[0])
#define GO_PARAM2(x) (((PT_REGS_ARM64 *)(x))->regs[1])
#define GO_PARAM3(x) (((PT_REGS_ARM64 *)(x))->regs[2])
#define GO_G(x)      (((PT_REGS_ARM64 *)(x))->regs[28])
#endif

struct go_key {
	__u64 g;
	__u32 tgid;
	__u32 dir;
};

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 16384);
	__type(key, struct go_key);
	__type(value, struct call_args);
} go_active SEC(".maps");

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

// Go crypto/tls: attach uprobes to function entry and to each RET instruction (never uretprobe, which
// rewrites the stack return address and crashes the Go runtime when a goroutine stack grows or is scanned).
static __always_inline int go_enter(struct pt_regs *ctx, __u32 dir)
{
	struct call_args a = {};
	struct go_key k = {};

	k.g = GO_G(ctx);
	k.tgid = bpf_get_current_pid_tgid() >> 32;
	k.dir = dir;
	a.ssl = GO_PARAM1(ctx);
	a.buf = GO_PARAM2(ctx);
	a.num = GO_PARAM3(ctx);
	a.dir = dir;
	bpf_map_update_elem(&go_active, &k, &a, BPF_ANY);
	return 0;
}

static __always_inline int go_ret(struct pt_regs *ctx, __u32 dir)
{
	__u64 id = bpf_get_current_pid_tgid();
	struct go_key k = {};
	struct call_args *ap;
	struct call_args a;
	long rc = (long)GO_PARAM1(ctx);

	k.g = GO_G(ctx);
	k.tgid = id >> 32;
	k.dir = dir;
	ap = bpf_map_lookup_elem(&go_active, &k);
	if (!ap)
		return 0;
	a = *ap;
	bpf_map_delete_elem(&go_active, &k);
	if (rc <= 0 || rc > 0x7fffffff || (__u64)rc > a.num)
		return 0;
	emit_all(&a, id, (__u32)rc);
	return 0;
}

SEC("uprobe/go_tls_write_enter")
int go_tls_write_enter(struct pt_regs *ctx) { return go_enter(ctx, DIR_WRITE); }

SEC("uprobe/go_tls_write_ret")
int go_tls_write_ret(struct pt_regs *ctx) { return go_ret(ctx, DIR_WRITE); }

SEC("uprobe/go_tls_read_enter")
int go_tls_read_enter(struct pt_regs *ctx) { return go_enter(ctx, DIR_READ); }

SEC("uprobe/go_tls_read_ret")
int go_tls_read_ret(struct pt_regs *ctx) { return go_ret(ctx, DIR_READ); }


// ---- plain HTTP (no TLS): a few syscalls on sockets that carry an HTTP POST ----------------------------------------
//
// TLS traffic is read through the uprobes above. Cleartext HTTP (for example an on-prem vLLM on http://host:8000)
// never passes through a TLS library, so the write/read syscalls are observed instead. To keep the cost near zero,
// only processes the collector asked for are looked at, and a socket is copied only after a write that starts with
// "POST " has marked it; the collector unmarks it when the exchange is complete, and close() unmarks it too.
//
// These are *raw* tracepoints. Programs attached to the per-syscall perf tracepoints are skipped, silently, whenever
// another BPF program is running on the same CPU (a per-CPU re-entrancy guard), which on a busy machine lost
// individual writes at random. Raw tracepoints only guard against recursion of the same program.

#if defined(__TARGET_ARCH_x86)
#define NR_READ 0
#define NR_WRITE 1
#define NR_CLOSE 3
#define NR_READV 19
#define NR_WRITEV 20
#define NR_SENDTO 44
#define NR_RECVFROM 45
#define NR_SENDMSG 46
#define NR_RECVMSG 47
#define NR_CLONE 56
#define NR_FORK 57
#define NR_VFORK 58
#define NR_CLONE3 435
#elif defined(__TARGET_ARCH_arm64)
#define NR_READ 63
#define NR_WRITE 64
#define NR_CLOSE 57
#define NR_READV 65
#define NR_WRITEV 66
#define NR_SENDTO 206
#define NR_RECVFROM 207
#define NR_SENDMSG 211
#define NR_RECVMSG 212
#define NR_CLONE 220
#define NR_CLONE3 435
#else
#error "unsupported architecture: add its syscall numbers"
#endif

const volatile __u32 self_tgid;
const volatile __u32 watch_all;
const volatile __u32 debug_unmatched; // testing only: report the first bytes of writes that did not start an HTTP POST

struct plain_key {
	__u32 pid;
	__u32 fd;
};

struct sys_args {
	__u64 buf; // buffer, or the iovec array for writev/readv/sendmsg/recvmsg
	__u64 len; // byte count, or the number of iovecs for vector calls
	__u32 fd;
	__u32 dir;
	__u32 vec;
};

struct iov {
	__u64 base;
	__u64 len;
};

struct msghdr_head {
	__u64 msg_name;
	__u32 msg_namelen;
	__u32 pad;
	__u64 msg_iov;
	__u64 msg_iovlen;
};

#define MAX_IOV 8

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
	__uint(max_entries, 16);
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

// The first three syscall arguments, read from the user registers saved at syscall entry.
static __always_inline int syscall_args(struct pt_regs *regs, __u64 *a0, __u64 *a1, __u64 *a2)
{
	if (bpf_probe_read_kernel(a0, sizeof(*a0), (const void *)&PT_REGS_PARM1(regs)) ||
	    bpf_probe_read_kernel(a1, sizeof(*a1), (const void *)&PT_REGS_PARM2(regs)) ||
	    bpf_probe_read_kernel(a2, sizeof(*a2), (const void *)&PT_REGS_PARM3(regs)))
		return -1;
	return 0;
}

SEC("raw_tp/sys_enter")
int raw_sys_enter(struct bpf_raw_tracepoint_args *ctx)
{
	struct pt_regs *regs = (struct pt_regs *)ctx->args[0];
	long nr = ctx->args[1];
	__u64 id, a0, a1, a2;
	struct sys_args a = {};
	__u32 tgid;

	switch (nr) {
	case NR_WRITE:
	case NR_SENDTO:
		a.dir = DIR_WRITE;
		break;
	case NR_WRITEV:
	case NR_SENDMSG:
		a.dir = DIR_WRITE;
		a.vec = 1;
		break;
	case NR_READ:
	case NR_RECVFROM:
		a.dir = DIR_READ;
		break;
	case NR_READV:
	case NR_RECVMSG:
		a.dir = DIR_READ;
		a.vec = 1;
		break;
#if defined(NR_FORK)
	case NR_FORK:
	case NR_VFORK:
#endif
	case NR_CLONE:
	case NR_CLONE3:
		if (!watching(&tgid))
			return 0;
		id = bpf_get_current_pid_tgid();
		a.dir = DIR_FORK;
		bpf_map_update_elem(&sys_active, &id, &a, BPF_ANY);
		return 0;
	case NR_CLOSE: {
		struct plain_key k = {};

		if (!watching(&k.pid) || syscall_args(regs, &a0, &a1, &a2))
			return 0;
		k.fd = (__u32)a0;
		bpf_map_delete_elem(&plain_conns, &k);
		return 0;
	}
	default:
		return 0; // every other syscall: as little work as possible
	}

	if (!watching(&tgid))
		return 0;
	bump(0);
	if (syscall_args(regs, &a0, &a1, &a2))
		return 0;
	if (nr == NR_SENDMSG || nr == NR_RECVMSG) {
		struct msghdr_head mh = {};

		if (bpf_probe_read_user(&mh, sizeof(mh), (void *)a1))
			return 0;
		a1 = mh.msg_iov;
		a2 = mh.msg_iovlen;
	}
	id = bpf_get_current_pid_tgid();
	a.fd = (__u32)a0;
	a.buf = a1;
	a.len = a2;
	bpf_map_update_elem(&sys_active, &id, &a, BPF_ANY);
	return 0;
}

SEC("raw_tp/sys_exit")
int raw_sys_exit(struct bpf_raw_tracepoint_args *ctx)
{
	__u64 id = bpf_get_current_pid_tgid();
	struct sys_args *ap = bpf_map_lookup_elem(&sys_active, &id);
	struct sys_args a;
	struct plain_key k = {};
	struct call_args ca = {};
	__u8 *marked;
	__u32 tgid;
	long ret = ctx->args[1];

	if (!ap)
		return 0; // the entry was filtered out: by far the common case
	bump(1);
	a = *ap;
	bpf_map_delete_elem(&sys_active, &id);
	if (a.dir == DIR_FORK) {
		if (ret > 0 && ret <= 0x7fffffff) {
			__u32 child = (__u32)ret;
			__u8 one = 1;

			bpf_map_update_elem(&watched, &child, &one, BPF_ANY);
		}
		return 0;
	}
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
		if (bpf_probe_read_user(head, sizeof(head), (void *)first)) {
			bump(7); // the buffer could not be read after the syscall returned
			return 0;
		}
		int is_post = (head[0] == 'P' && head[1] == 'O' && head[2] == 'S' && head[3] == 'T' && head[4] == ' ');
		int is_h2c = (head[0] == 'P' && head[1] == 'R' && head[2] == 'I' && head[3] == ' ' && head[4] == '*');
		if (!is_post && !is_h2c) {
			bump(4); // a write on a watched process that does not start an HTTP POST or h2c preface
			if (debug_unmatched) {
				ca.ssl = (1ULL << 62) | a.fd;
				ca.buf = first;
				ca.num = a.len;
				ca.dir = a.dir;
				emit_all(&ca, id, ret > 32 ? 32 : (__u32)ret);
			}
			return 0;
		}
		one = is_h2c ? 2 : 1;
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

char LICENSE[] SEC("license") = "Dual MIT/GPL";
