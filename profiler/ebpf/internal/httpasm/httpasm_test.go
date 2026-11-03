package httpasm

import (
	"bytes"
	"compress/gzip"
	"fmt"
	"strings"
	"testing"
	"time"
)

func chunked(parts ...string) string {
	var b strings.Builder
	for _, p := range parts {
		fmt.Fprintf(&b, "%x\r\n%s\r\n", len(p), p)
	}
	b.WriteString("0\r\n\r\n")
	return b.String()
}

func gz(s string) string {
	var buf bytes.Buffer
	w := gzip.NewWriter(&buf)
	w.Write([]byte(s))
	w.Close()
	return buf.String()
}

const reqBody = `{"model":"m","messages":[{"role":"user","content":"hi"}]}`

func request(path, body string) string {
	return fmt.Sprintf("POST %s?key=SECRET HTTP/1.1\r\nHost: api.example.com\r\nAuthorization: Bearer SECRET\r\nContent-Length: %d\r\n\r\n%s", path, len(body), body)
}

func run(t *testing.T, feed func(a *Assembler, now time.Time)) []Exchange {
	t.Helper()
	var got []Exchange
	a := New(func(e Exchange) { got = append(got, e) }, 50*time.Millisecond)
	now := time.Unix(1000, 0)
	feed(a, now)
	a.Close()
	return got
}

func split(s string, n int) []string {
	var out []string
	for len(s) > n {
		out = append(out, s[:n])
		s = s[n:]
	}
	return append(out, s)
}

func TestJSONResponseSplitAcrossReads(t *testing.T) {
	resp := `{"usage":{"prompt_tokens":5}}`
	raw := fmt.Sprintf("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: %d\r\n\r\n%s", len(resp), resp)
	got := run(t, func(a *Assembler, now time.Time) {
		for _, p := range split(request("/v1/chat/completions", reqBody), 17) {
			a.Feed(1, "py", 7, DirWrite, []byte(p), now)
		}
		for _, p := range split(raw, 11) {
			a.Feed(1, "py", 7, DirRead, []byte(p), now.Add(time.Second))
		}
	})
	if len(got) != 1 {
		t.Fatalf("want 1 exchange, got %d", len(got))
	}
	e := got[0]
	if e.Path != "/v1/chat/completions" || e.Host != "api.example.com" || e.Request != reqBody || e.Response != resp || e.Status != 200 {
		t.Fatalf("unexpected exchange: %+v", e)
	}
	if strings.Contains(e.Path, "SECRET") || strings.Contains(e.Request, "SECRET") || strings.Contains(e.Response, "SECRET") {
		t.Fatal("credential leaked into the exchange")
	}
}

func TestChunkedGzipSSE(t *testing.T) {
	sse := "data: {\"a\":1}\n\ndata: [DONE]\n\n"
	raw := "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Encoding: gzip\r\nTransfer-Encoding: chunked\r\n\r\n" + chunked(gz(sse))
	got := run(t, func(a *Assembler, now time.Time) {
		a.Feed(2, "node", 9, DirWrite, []byte(request("/v1/responses", `{"input":"x"}`)), now)
		a.Feed(2, "node", 9, DirRead, []byte(raw), now)
	})
	if len(got) != 1 || !got[0].SSE || got[0].Response != sse {
		t.Fatalf("unexpected: %+v", got)
	}
}

func TestKeepAliveTwoCallsOneConnection(t *testing.T) {
	resp := func(s string) string {
		return fmt.Sprintf("HTTP/1.1 200 OK\r\nContent-Length: %d\r\n\r\n%s", len(s), s)
	}
	got := run(t, func(a *Assembler, now time.Time) {
		a.Feed(3, "c", 1, DirWrite, []byte(request("/v1/messages", reqBody)), now)
		a.Feed(3, "c", 1, DirRead, []byte(resp(`{"n":1}`)), now)
		a.Feed(3, "c", 1, DirWrite, []byte(request("/v1/messages", reqBody)), now)
		a.Feed(3, "c", 1, DirRead, []byte(resp(`{"n":2}`)), now)
	})
	if len(got) != 2 || got[0].Response != `{"n":1}` || got[1].Response != `{"n":2}` {
		t.Fatalf("unexpected: %+v", got)
	}
}

func TestIgnoresNonLLMAndHTTP2(t *testing.T) {
	var a *Assembler
	got := run(t, func(x *Assembler, now time.Time) {
		a = x
		x.Feed(4, "c", 1, DirWrite, []byte(request("/upload", `{"file":"x"}`)), now)
		x.Feed(4, "c", 1, DirRead, []byte("HTTP/1.1 204 No Content\r\n\r\n"), now)
		x.Feed(4, "c", 2, DirWrite, []byte("PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n"), now)
	})
	if len(got) != 0 || a.Stats.Ignored != 1 || a.Stats.NonHTTP != 1 {
		t.Fatalf("got=%v stats=%+v", got, a.Stats)
	}
}

func TestIdleSweepFlushesCloseDelimitedResponse(t *testing.T) {
	var got []Exchange
	a := New(func(e Exchange) { got = append(got, e) }, 10*time.Millisecond)
	now := time.Unix(1000, 0)
	a.Feed(5, "c", 1, DirWrite, []byte(request("/v1/chat/completions", reqBody)), now)
	a.Feed(5, "c", 1, DirRead, []byte("HTTP/1.1 200 OK\r\n\r\n{\"ok\":true}"), now)
	a.Sweep(now.Add(time.Second))
	if len(got) != 1 || got[0].Response != `{"ok":true}` {
		t.Fatalf("unexpected: %+v", got)
	}
}

func TestInterimContinueIsSkipped(t *testing.T) {
	got := run(t, func(a *Assembler, now time.Time) {
		a.Feed(6, "c", 1, DirWrite, []byte(request("/v1/chat/completions", reqBody)), now)
		a.Feed(6, "c", 1, DirRead, []byte("HTTP/1.1 100 Continue\r\n\r\n"), now)
		a.Feed(6, "c", 1, DirRead, []byte("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"), now)
	})
	if len(got) != 1 || got[0].Status != 200 {
		t.Fatalf("unexpected: %+v", got)
	}
}
