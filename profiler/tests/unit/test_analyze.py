from tokprof.analyze import findings, group_requests, kind_breakdown, summarize
from tokprof.report import render_json, render_markdown, render_text


def seg(kind, tokens, name=None, h=None):
    return {"kind": kind, "tokens": tokens, "chars": tokens * 4, "hash": h if h is not None else f"{kind}-{name}-{tokens}", "name": name}


def call(ts, input_total, segments, **kw):
    r = {"ts": ts, "latency_ms": 500, "usage": {"input_total": input_total, "output": 10}, "segments": segments,
         "residual": input_total - sum(s["tokens"] for s in segments), "tool_calls": [], "sysfp": "sysA", "model": "m"}
    r.update(kw)
    return r


def pipeline():
    big = seg("system", 5000, "reference", h="ref-hash")
    tools = [seg("tool_def", 300, "search", h="t1"), seg("tool_def", 400, "unused_tool", h="t2")]
    recs = []
    for i in range(6):
        segs = [big, *tools, seg("user_prompt", 50), seg("history_user", 100 * i), seg("tool_result", 200)]
        recs.append(call(1000 + i * 0.1, sum(s["tokens"] for s in segs) + 20, segs, tool_calls=["search"] if i % 2 == 0 else []))
    return recs


def test_kind_breakdown_includes_residual():
    bd = kind_breakdown(pipeline())
    assert bd["totals"]["system"] == 5000 * 6 and bd["totals"]["residual"] == 20 * 6


def test_findings_unused_tool_and_repeated_block():
    fs = findings(pipeline())
    texts = " | ".join(f["text"] for f in fs)
    assert "unused_tool" in texts and "never called" in texts
    assert "5,000 tokens was sent in 6 calls" in texts


def test_group_requests_by_gap():
    recs = [call(0, 10, []), call(1, 10, []), call(100, 10, []), call(101, 10, []), call(102, 10, [])]
    assert [len(g) for g in group_requests(recs, 5.0)] == [2, 3]


def test_fan_out_finding():
    recs = [call(i * 0.5, 100, []) for i in range(8)]
    assert any("median" in f["text"] and "LLM calls" in f["text"] for f in findings(recs))


def test_tool_results_dominating():
    segs = [seg("system", 100), seg("tool_result", 900)]
    recs = [call(i * 20, 1000, segs) for i in range(3)]
    assert any("Tool results" in f["text"] for f in findings(recs))


def test_reports_render_and_never_crash_on_empty():
    recs = pipeline()
    t = render_text(recs)
    assert "Input token breakdown" in t and "System prompt" in t and "Findings" in t
    assert "| Component |" in render_markdown(recs)
    assert '"calls": 6' in render_json(recs)
    assert render_text([]) == "No LLM calls recorded."
    assert summarize(recs)["calls"] == 6


def test_report_tolerates_records_without_usage():
    r = {"ts": 1, "segments": [seg("user_prompt", 10)], "est_input": 10, "usage": {}}
    assert "Calls: 1" in render_text([r])
