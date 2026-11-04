// Package httpasm rebuilds HTTP/1.1 request/response exchanges from the
// plaintext chunks captured on TLS read/write calls and keeps only the
// exchanges that look like LLM API calls. Headers other than the few needed
// for framing (Host, Content-Length, Transfer-Encoding, Content-Encoding,
// Content-Type) are never retained, so credentials cannot leave this package.
package httpasm

import (
	"bytes"
	"compress/flate"
	"compress/gzip"
	"io"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/andybalholm/brotli"
	"github.com/klauspost/compress/zstd"
)

const (
	DirWrite = 0
	DirRead  = 1

	maxBuf      = 8 << 20
	maxDecoded  = 16 << 20
	connStaleAt = 5 * time.Minute
)

// Exchange is one completed call that passed the LLM filter.
type Exchange struct {
	Time      time.Time
	PID       uint32
	Comm      string
	Host      string
	Path      string
	Request   string // decoded JSON request body
	Response  string // decoded response body (or SSE transcript)
	SSE       bool
	Status    int
	TTFBMs    float64
	LatencyMs float64 // request start to last response byte
}

type key struct {
	pid uint32
	ssl uint64
}

type conn struct {
	comm      string
	wbuf      []byte
	rbuf      []byte
	reading   bool
	bad       bool
	h2        *h2conn
	wDamaged  bool // request bytes were truncated by the capture limit
	rDamaged  bool
	reqAt     time.Time
	firstResp time.Time
	lastRead  time.Time
	lastAt    time.Time
}

// Stats counts what the assembler did with the traffic it saw.
type Stats struct {
	Exchanges  int // emitted
	Ignored    int // complete HTTP exchanges that were not LLM calls
	NonHTTP    int // connections that were neither HTTP/1.x nor HTTP/2 seen from the start (for example TLS in mid-connection)
	Incomplete int // exchanges dropped because bytes were missing
	Unreadable int // LLM calls whose response could not be decoded
}

// Assembler is not safe for concurrent use.
type Assembler struct {
	conns map[key]*conn
	emit  func(Exchange)
	idle  time.Duration
	Stats Stats
	Debug func(format string, args ...any) // optional diagnostics
	// OnDone is called when an exchange on a connection has been flushed (the connection can be reused).
	OnDone func(pid uint32, ssl uint64)
}

func New(emit func(Exchange), idle time.Duration) *Assembler {
	if idle <= 0 {
		idle = 2 * time.Second
	}
	return &Assembler{conns: map[key]*conn{}, emit: emit, idle: idle}
}

// Feed adds one captured chunk.
func (a *Assembler) Feed(pid uint32, comm string, ssl uint64, dir int, data []byte, now time.Time) {
	k := key{pid, ssl}
	c := a.conns[k]
	if c == nil {
		c = &conn{comm: comm}
		a.conns[k] = c
	}
	c.lastAt = now
	if dir == DirRead {
		c.lastRead = now
	}
	if c.bad {
		return
	}
	if c.h2 != nil {
		for _, st := range c.h2.feed(dir, data, now) {
			a.h2exchange(k, c, st)
		}
		if c.h2.bad {
			c.bad, c.h2 = true, nil
			a.Stats.Incomplete++
		}
		return
	}
	if dir == DirWrite {
		if c.reading {
			a.flush(k, c)
		}
		if len(c.wbuf) == 0 {
			c.reqAt = now
		}
		if len(c.wbuf)+len(data) > maxBuf {
			c.bad, c.wbuf = true, nil
			return
		}
		c.wbuf = append(c.wbuf, data...)
		if bytes.HasPrefix(h2Preface, c.wbuf) && len(c.wbuf) < len(h2Preface) {
			return // part of the HTTP/2 preface; wait for the rest
		}
		if bytes.HasPrefix(c.wbuf, h2Preface) {
			c.h2 = newH2()
			rest := c.wbuf[len(h2Preface):]
			c.wbuf = nil
			for _, st := range c.h2.feed(DirWrite, rest, now) {
				a.h2exchange(k, c, st)
			}
			return
		}
		if !looksLikeHTTPRequest(c.wbuf) {
			if a.Debug != nil {
				a.Debug("pid %d (%s) conn %#x: not HTTP, first bytes %q", pid, comm, ssl, firstN(c.wbuf, 24))
			}
			c.bad, c.wbuf = true, nil
			a.Stats.NonHTTP++
		}
		return
	}
	if len(c.wbuf) == 0 {
		return
	}
	if !c.reading {
		if bytes.HasPrefix(data, []byte("HTTP/1.1 100")) && bytes.HasSuffix(data, []byte("\r\n\r\n")) {
			return // interim "100 Continue"; the real response follows
		}
		c.reading = true
		c.firstResp = now
	}
	if len(c.rbuf)+len(data) > maxBuf {
		a.flush(k, c)
		return
	}
	c.rbuf = append(c.rbuf, data...)
	if responseComplete(c.rbuf) {
		a.flush(k, c)
	}
}

// MarkDamaged records that a chunk was cut short by the capture limit, so the
// affected side must not be trusted.
func (a *Assembler) MarkDamaged(pid uint32, ssl uint64, dir int) {
	if c := a.conns[key{pid, ssl}]; c != nil {
		if dir == DirWrite {
			c.wDamaged = true
		} else {
			c.rDamaged = true
		}
	}
}

// Sweep flushes exchanges whose response went quiet and forgets stale connections.
func (a *Assembler) Sweep(now time.Time) {
	for k, c := range a.conns {
		if c.reading && now.Sub(c.lastAt) >= a.idle {
			a.flush(k, c)
		}
		if now.Sub(c.lastAt) >= connStaleAt {
			delete(a.conns, k)
		}
	}
}

// Close flushes everything that is pending.
func (a *Assembler) Close() {
	for k, c := range a.conns {
		if c.reading {
			a.flush(k, c)
		}
	}
}

// ForgetPID flushes and drops the state of an exited process.
func (a *Assembler) ForgetPID(pid uint32) {
	for k, c := range a.conns {
		if k.pid == pid {
			if c.reading {
				a.flush(k, c)
			}
			delete(a.conns, k)
		}
	}
}

func (a *Assembler) flush(k key, c *conn) {
	req, resp := c.wbuf, c.rbuf
	reqAt, firstResp, lastRead := c.reqAt, c.firstResp, c.lastRead
	wDamaged, rDamaged := c.wDamaged, c.rDamaged
	c.wbuf, c.rbuf, c.reading = nil, nil, false
	c.wDamaged, c.rDamaged = false, false
	if a.OnDone != nil {
		defer a.OnDone(k.pid, k.ssl)
	}
	if wDamaged {
		a.Stats.Incomplete++
		return
	}

	rh, rbody, ok := splitMessage(req)
	if !ok {
		a.Stats.Incomplete++
		return
	}
	method, path := requestLine(rh.start)
	if method != "POST" {
		a.Stats.Ignored++
		return
	}
	body, complete := decodeBody(rh, rbody)
	if !complete {
		a.Stats.Incomplete++
		return
	}
	body = bytes.TrimSpace(body)
	if !Plausible(method, path, body) {
		a.Stats.Ignored++
		return
	}
	ex := Exchange{
		Time:    reqAt,
		PID:     k.pid,
		Comm:    c.comm,
		Host:    rh.get("host"),
		Path:    path,
		Request: string(body),
	}
	if !firstResp.IsZero() {
		ex.TTFBMs = float64(firstResp.Sub(reqAt).Microseconds()) / 1000
		ex.LatencyMs = float64(lastRead.Sub(reqAt).Microseconds()) / 1000
	}
	if ph, pbody, ok := splitMessage(resp); ok && !rDamaged {
		ex.Status = statusCode(ph.start)
		ex.SSE = strings.Contains(ph.get("content-type"), "text/event-stream")
		text, _ := decodeBody(ph, pbody)
		if text == nil && len(pbody) > 0 {
			a.Stats.Unreadable++
		}
		ex.Response = string(text)
	}
	a.Stats.Exchanges++
	a.emit(ex)
}

type header struct {
	start  string
	fields map[string]string
}

func (h header) get(name string) string { return h.fields[name] }

// splitMessage separates the head from the (possibly partial) body.
func splitMessage(b []byte) (header, []byte, bool) {
	i := bytes.Index(b, []byte("\r\n\r\n"))
	if i < 0 {
		return header{}, nil, false
	}
	lines := strings.Split(string(b[:i]), "\r\n")
	h := header{start: lines[0], fields: map[string]string{}}
	for _, ln := range lines[1:] {
		j := strings.IndexByte(ln, ':')
		if j <= 0 {
			continue
		}
		name := strings.ToLower(strings.TrimSpace(ln[:j]))
		val := strings.TrimSpace(ln[j+1:])
		switch name {
		case "host":
			h.fields[name] = val
		case "content-length", "transfer-encoding", "content-encoding", "content-type":
			h.fields[name] = strings.ToLower(val)
		}
	}
	return h, b[i+4:], true
}

func requestLine(s string) (method, path string) {
	parts := strings.SplitN(s, " ", 3)
	if len(parts) < 2 {
		return "", ""
	}
	path = parts[1]
	if q := strings.IndexByte(path, '?'); q >= 0 {
		path = path[:q] // query strings can carry secrets; the path alone is enough
	}
	return parts[0], path
}

func statusCode(s string) int {
	parts := strings.SplitN(s, " ", 3)
	if len(parts) < 2 {
		return 0
	}
	n, _ := strconv.Atoi(parts[1])
	return n
}

var methods = [][]byte{[]byte("POST "), []byte("GET "), []byte("PUT "), []byte("PATCH "), []byte("DELETE "), []byte("HEAD "), []byte("OPTIONS ")}

func looksLikeHTTPRequest(b []byte) bool {
	for _, m := range methods {
		if bytes.HasPrefix(b, m) || (len(b) < len(m) && bytes.HasPrefix(m, b)) {
			return true
		}
	}
	return false
}

// The pre-filter only decides what is worth handing to tokprof, which makes the real decision.
// Keep it identical to tokprof.detect.plausible (tests/vectors/prefilter.json).
var (
	excludedPath = regexp.MustCompile(`(?i)/(embeddings?|moderations?|rerank|tokenize|detokenize|count_tokens|` +
		`audio/(speech|transcriptions|translations)|images?/(generations|edits|variations)|` +
		`files|batches|fine_tuning|uploads|vector_stores|assistants|threads|realtime)(/|$)` +
		`|:(embedContent|batchEmbedContents|countTokens)$`)
	plausibleKeys = regexp.MustCompile(`"(?:messages|contents|chat_history)"\s*:\s*\[|"(?:prompt|input|inputs|preamble|message)"\s*:`)
)

// Plausible reports whether a request body is worth sending to the detector.
func Plausible(method, path string, body []byte) bool {
	if !strings.EqualFold(method, "POST") {
		return false
	}
	head := bytes.TrimLeft(body, " \t\r\n")
	if len(head) < 8 || head[0] != '{' {
		return false
	}
	p := path
	if i := strings.IndexAny(p, "?#"); i >= 0 {
		p = p[:i]
	}
	if p = strings.TrimRight(p, "/"); p == "" {
		p = "/"
	}
	if excludedPath.MatchString(p) {
		return false
	}
	return plausibleKeys.Match(body)
}

// responseComplete reports whether the buffered bytes hold one full message.
func responseComplete(b []byte) bool {
	h, body, ok := splitMessage(b)
	if !ok {
		return false
	}
	if code := statusCode(h.start); code == 204 || code == 304 || (code >= 100 && code < 200) {
		return true
	}
	if strings.Contains(h.get("transfer-encoding"), "chunked") {
		_, done := dechunk(body)
		return done
	}
	if cl := h.get("content-length"); cl != "" {
		n, err := strconv.Atoi(cl)
		return err == nil && len(body) >= n
	}
	return false // close-delimited: wait for idle / next request
}

// decodeBody returns the decoded body and whether the framing says it is complete.
func decodeBody(h header, body []byte) ([]byte, bool) {
	complete := true
	switch {
	case strings.Contains(h.get("transfer-encoding"), "chunked"):
		body, complete = dechunk(body)
	case h.get("content-length") != "":
		n, err := strconv.Atoi(h.get("content-length"))
		if err != nil || len(body) < n {
			complete = false
		} else {
			body = body[:n]
		}
	}
	out, ok := decodeEncoded(body, h.get("content-encoding"))
	if !ok {
		return nil, complete
	}
	return out, complete
}

// decodeEncoded applies a content-encoding; ok is false when the encoding cannot be decoded.
func decodeEncoded(body []byte, enc string) ([]byte, bool) {
	switch strings.TrimSpace(enc) {
	case "gzip", "x-gzip":
		r, err := gzip.NewReader(bytes.NewReader(body))
		if err != nil {
			return nil, false
		}
		out, _ := io.ReadAll(io.LimitReader(r, maxDecoded))
		return out, true
	case "deflate":
		out, _ := io.ReadAll(io.LimitReader(flate.NewReader(bytes.NewReader(body)), maxDecoded))
		return out, true
	case "br":
		out, err := io.ReadAll(io.LimitReader(brotli.NewReader(bytes.NewReader(body)), maxDecoded))
		if err != nil {
			return nil, false
		}
		return out, true
	case "zstd":
		zr, err := zstd.NewReader(bytes.NewReader(body))
		if err != nil {
			return nil, false
		}
		defer zr.Close()
		out, err := io.ReadAll(io.LimitReader(zr, maxDecoded))
		if err != nil {
			return nil, false
		}
		return out, true
	case "", "identity":
		return body, true
	}
	return nil, false
}

// dechunk decodes chunked transfer encoding; done is true once the last chunk was seen.
func dechunk(b []byte) ([]byte, bool) {
	var out []byte
	for {
		i := bytes.Index(b, []byte("\r\n"))
		if i < 0 {
			return out, false
		}
		sizeText := string(b[:i])
		if j := strings.IndexByte(sizeText, ';'); j >= 0 {
			sizeText = sizeText[:j]
		}
		n, err := strconv.ParseInt(strings.TrimSpace(sizeText), 16, 64)
		if err != nil || n < 0 {
			return out, false
		}
		b = b[i+2:]
		if n == 0 {
			return out, true
		}
		if int64(len(b)) < n+2 {
			out = append(out, b...)
			return out, false
		}
		out = append(out, b[:n]...)
		b = b[n+2:]
	}
}

func firstN(b []byte, n int) []byte {
	if len(b) > n {
		return b[:n]
	}
	return b
}
