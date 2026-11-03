package httpasm

import (
	"bytes"
	"testing"
	"time"

	"golang.org/x/net/http2"
	"golang.org/x/net/http2/hpack"
)

type h2side struct {
	buf bytes.Buffer
	fr  *http2.Framer
	enc *hpack.Encoder
	hb  bytes.Buffer
}

func newSide() *h2side {
	s := &h2side{}
	s.fr = http2.NewFramer(&s.buf, nil)
	s.enc = hpack.NewEncoder(&s.hb)
	return s
}

func (s *h2side) headers(id uint32, endStream bool, fields ...hpack.HeaderField) {
	s.hb.Reset()
	for _, f := range fields {
		s.enc.WriteField(f)
	}
	s.fr.WriteHeaders(http2.HeadersFrameParam{StreamID: id, BlockFragment: s.hb.Bytes(), EndHeaders: true, EndStream: endStream})
}

func (s *h2side) take() []byte {
	b := append([]byte(nil), s.buf.Bytes()...)
	s.buf.Reset()
	return b
}

func hf(n, v string) hpack.HeaderField { return hpack.HeaderField{Name: n, Value: v} }

func splitFeed(a *Assembler, pid uint32, ssl uint64, dir int, b []byte, n int, now time.Time) {
	for len(b) > 0 {
		m := n
		if m > len(b) {
			m = len(b)
		}
		a.Feed(pid, "c", ssl, dir, b[:m], now)
		b = b[m:]
	}
}

func TestHTTP2TwoStreamsInterleaved(t *testing.T) {
	var got []Exchange
	a := New(func(e Exchange) { got = append(got, e) }, time.Second)
	now := time.Unix(1000, 0)

	cl, sv := newSide(), newSide()
	cl.fr.WriteSettings()
	cl.headers(1, false, hf(":method", "POST"), hf(":scheme", "https"), hf(":authority", "api.example.com"), hf(":path", "/v1/chat/completions?key=SECRET"), hf("authorization", "Bearer SECRET"))
	cl.fr.WriteData(1, false, []byte(reqBody[:20]))
	cl.headers(3, false, hf(":method", "POST"), hf(":authority", "api.example.com"), hf(":path", "/v1/responses"))
	cl.fr.WriteData(1, true, []byte(reqBody[20:]))
	cl.fr.WriteData(3, true, []byte(`{"input":"x"}`))
	splitFeed(a, 1, 5, DirWrite, append(append([]byte{}, h2Preface...), cl.take()...), 13, now)

	sv.fr.WriteSettings()
	sv.headers(3, false, hf(":status", "200"), hf("content-type", "text/event-stream"))
	sv.fr.WriteData(3, true, []byte("data: {\"n\":1}\n\ndata: [DONE]\n\n"))
	sv.headers(1, false, hf(":status", "200"), hf("content-type", "application/json"))
	sv.fr.WriteData(1, false, []byte(`{"usage":`))
	sv.fr.WriteData(1, true, []byte(`{"prompt_tokens":5}}`))
	splitFeed(a, 1, 5, DirRead, sv.take(), 7, now.Add(time.Second))

	if len(got) != 2 {
		t.Fatalf("want 2 exchanges, got %d (stats %+v)", len(got), a.Stats)
	}
	byPath := map[string]Exchange{}
	for _, e := range got {
		byPath[e.Path] = e
	}
	e1, e3 := byPath["/v1/chat/completions"], byPath["/v1/responses"]
	if e1.Request != reqBody || e1.Response != `{"usage":{"prompt_tokens":5}}` || e1.Status != 200 || e1.SSE || e1.Host != "api.example.com" {
		t.Fatalf("stream 1: %+v", e1)
	}
	if !e3.SSE || e3.Request != `{"input":"x"}` || e3.Response != "data: {\"n\":1}\n\ndata: [DONE]\n\n" {
		t.Fatalf("stream 3: %+v", e3)
	}
	for _, e := range got {
		if bytes.Contains([]byte(e.Path+e.Request+e.Response+e.Host), []byte("SECRET")) {
			t.Fatal("credential leaked")
		}
	}
	if e1.LatencyMs < 900 || e1.TTFBMs < 900 {
		t.Fatalf("timing not measured: %+v", e1)
	}
}

func TestHTTP2UsesHPACKStateAcrossRequests(t *testing.T) {
	var got []Exchange
	a := New(func(e Exchange) { got = append(got, e) }, time.Second)
	now := time.Unix(1000, 0)
	cl, sv := newSide(), newSide()
	for i, id := range []uint32{1, 3} {
		cl.headers(id, false, hf(":method", "POST"), hf(":authority", "h"), hf(":path", "/v1/chat/completions"), hf("x-custom", "same-value-twice"))
		cl.fr.WriteData(id, true, []byte(reqBody))
		pre := []byte{}
		if i == 0 {
			pre = h2Preface
		}
		a.Feed(2, "c", 9, DirWrite, append(append([]byte{}, pre...), cl.take()...), now)
		sv.headers(id, false, hf(":status", "200"))
		sv.fr.WriteData(id, true, []byte(`{"ok":1}`))
		a.Feed(2, "c", 9, DirRead, sv.take(), now)
	}
	if len(got) != 2 {
		t.Fatalf("want 2, got %d stats=%+v", len(got), a.Stats)
	}
}

func TestHTTP2MidConnectionIsNotFollowed(t *testing.T) {
	a := New(func(Exchange) { t.Fatal("must not emit") }, time.Second)
	cl := newSide()
	cl.headers(5, true, hf(":method", "GET"), hf(":path", "/"))
	a.Feed(3, "c", 1, DirWrite, cl.take(), time.Unix(1, 0))
	if a.Stats.NonHTTP != 1 {
		t.Fatalf("stats %+v", a.Stats)
	}
}
