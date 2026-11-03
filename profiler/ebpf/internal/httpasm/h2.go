package httpasm

import (
	"bytes"
	"encoding/binary"
	"strconv"
	"strings"
	"time"

	"golang.org/x/net/http2/hpack"
)

var h2Preface = []byte("PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")

const (
	h2Data         = 0x0
	h2Headers      = 0x1
	h2RstStream    = 0x3
	h2Continuation = 0x9

	flagEndStream  = 0x1
	flagEndHeaders = 0x4
	flagPadded     = 0x8
	flagPriority   = 0x20
)

type h2stream struct {
	method, path, authority string
	reqCT, reqCE            string
	status                  int
	respCT, respCE          string
	reqBody, respBody       []byte
	reqEnd, respEnd         bool
	gotRespHeaders          bool
	t0, tFirst, tLast       time.Time
}

type h2dir struct {
	buf     []byte
	dec     *hpack.Decoder
	block   []byte // header block being assembled across CONTINUATION frames
	blockID uint32
	blockES bool // END_STREAM was set on the HEADERS frame that started the block
	inBlock bool
}

// h2conn follows one HTTP/2 connection that was observed from its first byte.
// A connection already in progress cannot be followed because the HPACK dynamic
// table (and the frame boundaries) are unknown.
type h2conn struct {
	req, resp h2dir
	streams   map[uint32]*h2stream
	bad       bool
}

func newH2() *h2conn {
	return &h2conn{
		req:     h2dir{dec: hpack.NewDecoder(4096, nil)},
		resp:    h2dir{dec: hpack.NewDecoder(4096, nil)},
		streams: map[uint32]*h2stream{},
	}
}

// feed consumes chunk bytes of one direction and returns finished streams.
func (h *h2conn) feed(dir int, data []byte, now time.Time) []*h2stream {
	if h.bad {
		return nil
	}
	d := &h.resp
	if dir == DirWrite {
		d = &h.req
	}
	if len(d.buf)+len(data) > maxBuf {
		h.bad = true
		return nil
	}
	d.buf = append(d.buf, data...)
	var done []*h2stream
	for len(d.buf) >= 9 {
		n := int(d.buf[0])<<16 | int(d.buf[1])<<8 | int(d.buf[2])
		if n > maxBuf {
			h.bad = true
			return done
		}
		if len(d.buf) < 9+n {
			break
		}
		typ, flags := d.buf[3], d.buf[4]
		id := binary.BigEndian.Uint32(d.buf[5:9]) & 0x7fffffff
		payload := d.buf[9 : 9+n]
		if s := h.frame(dir, d, typ, flags, id, payload, now); s != nil {
			done = append(done, s)
		}
		if h.bad {
			return done
		}
		d.buf = d.buf[9+n:]
	}
	if len(d.buf) == 0 {
		d.buf = nil
	}
	return done
}

func (h *h2conn) stream(id uint32, now time.Time) *h2stream {
	s := h.streams[id]
	if s == nil {
		s = &h2stream{t0: now}
		h.streams[id] = s
	}
	return s
}

func (h *h2conn) frame(dir int, d *h2dir, typ, flags byte, id uint32, p []byte, now time.Time) *h2stream {
	if d.inBlock && typ != h2Continuation {
		h.bad = true // protocol violation or lost bytes
		return nil
	}
	switch typ {
	case h2Data:
		s := h.streams[id]
		if s == nil {
			return nil
		}
		if flags&flagPadded != 0 {
			if len(p) < 1 || int(p[0]) >= len(p) {
				h.bad = true
				return nil
			}
			p = p[1 : len(p)-int(p[0])]
		}
		if dir == DirWrite {
			if len(s.reqBody)+len(p) <= maxBuf {
				s.reqBody = append(s.reqBody, p...)
			}
		} else {
			if s.tFirst.IsZero() {
				s.tFirst = now
			}
			s.tLast = now
			if len(s.respBody)+len(p) <= maxBuf {
				s.respBody = append(s.respBody, p...)
			}
		}
		if flags&flagEndStream != 0 {
			return h.end(dir, id, s, now)
		}
	case h2Headers:
		if flags&flagPadded != 0 {
			if len(p) < 1 || int(p[0]) >= len(p) {
				h.bad = true
				return nil
			}
			p = p[1 : len(p)-int(p[0])]
		}
		if flags&flagPriority != 0 {
			if len(p) < 5 {
				h.bad = true
				return nil
			}
			p = p[5:]
		}
		d.block = append(d.block[:0], p...)
		d.blockID, d.blockES = id, flags&flagEndStream != 0
		d.inBlock = true
		if flags&flagEndHeaders != 0 {
			return h.headers(dir, d, now)
		}
	case h2Continuation:
		if !d.inBlock || id != d.blockID {
			h.bad = true
			return nil
		}
		d.block = append(d.block, p...)
		if len(d.block) > 1<<20 {
			h.bad = true
			return nil
		}
		if flags&flagEndHeaders != 0 {
			return h.headers(dir, d, now)
		}
	case h2RstStream:
		delete(h.streams, id)
	}
	return nil
}

func (h *h2conn) headers(dir int, d *h2dir, now time.Time) *h2stream {
	d.inBlock = false
	fields, err := d.dec.DecodeFull(d.block)
	id, es := d.blockID, d.blockES
	d.block = d.block[:0]
	if err != nil {
		h.bad = true
		return nil
	}
	s := h.stream(id, now)
	if dir == DirWrite {
		for _, f := range fields {
			switch f.Name {
			case ":method":
				s.method = f.Value
			case ":path":
				s.path = f.Value
			case ":authority":
				s.authority = f.Value
			case "content-type":
				s.reqCT = strings.ToLower(f.Value)
			case "content-encoding":
				s.reqCE = strings.ToLower(f.Value)
			}
		}
	} else if !s.gotRespHeaders || s.status/100 == 1 {
		for _, f := range fields {
			switch f.Name {
			case ":status":
				s.status, _ = strconv.Atoi(f.Value)
			case "content-type":
				s.respCT = strings.ToLower(f.Value)
			case "content-encoding":
				s.respCE = strings.ToLower(f.Value)
			}
		}
		if s.status/100 != 1 {
			s.gotRespHeaders = true
			if s.tFirst.IsZero() {
				s.tFirst = now
			}
		}
	} // otherwise these are trailers, which only matter for END_STREAM
	if es {
		return h.end(dir, id, s, now)
	}
	return nil
}

func (h *h2conn) end(dir int, id uint32, s *h2stream, now time.Time) *h2stream {
	if dir == DirWrite {
		s.reqEnd = true
		return nil
	}
	s.respEnd, s.tLast = true, now
	delete(h.streams, id)
	if !s.reqEnd {
		return nil // the server answered before the request finished (an error); not an LLM exchange
	}
	return s
}

// h2exchange converts a finished stream into an Exchange, or reports why it is not one.
func (a *Assembler) h2exchange(k key, c *conn, s *h2stream) {
	if s.method != "POST" {
		a.Stats.Ignored++
		return
	}
	path := s.path
	if q := strings.IndexByte(path, '?'); q >= 0 {
		path = path[:q]
	}
	req, ok := decodeEncoded(s.reqBody, s.reqCE)
	if !ok {
		a.Stats.Incomplete++
		return
	}
	req = bytes.TrimSpace(req)
	if !looksLikeLLMBody(req) {
		a.Stats.Ignored++
		return
	}
	resp, ok := decodeEncoded(s.respBody, s.respCE)
	if !ok && len(s.respBody) > 0 {
		a.Stats.Unreadable++
	}
	ex := Exchange{
		Time: s.t0, PID: k.pid, Comm: c.comm, Host: s.authority, Path: path,
		Request: string(req), Response: string(resp), Status: s.status,
		SSE: strings.Contains(s.respCT, "text/event-stream"),
	}
	if !s.tFirst.IsZero() {
		ex.TTFBMs = float64(s.tFirst.Sub(s.t0).Microseconds()) / 1000
	}
	ex.LatencyMs = float64(s.tLast.Sub(s.t0).Microseconds()) / 1000
	a.Stats.Exchanges++
	a.emit(ex)
}
