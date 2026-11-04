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


def test_estimate_cost_with_cached_and_default_price():
    from tokprof.analyze import estimate_cost

    recs = [{"model": "m1", "usage": {"input_total": 1_000_000, "cached": 400_000, "output": 100_000}},
            {"model": "other", "usage": {"input_total": 1_000_000, "output": 0}}]
    c = estimate_cost(recs, {"m1": {"input": 2.0, "cached": 0.5, "output": 8.0}})
    assert abs(c["total"] - (0.6 * 2.0 + 0.4 * 0.5 + 0.1 * 8.0)) < 1e-9 and c["models_without_price"] == ["other"]
    assert estimate_cost(recs, {"default": {"input": 1.0, "output": 1.0}})["models_without_price"] == []


def _requests(n, tokens):
    out = []
    for i in range(n):
        out.append({"ts": i * 100, "latency_ms": 100, "usage": {"input_total": tokens}, "segments": [], "est_input": tokens})
    return out


def test_compare_detects_real_reduction_and_flags_small_samples():
    from tokprof.analyze import compare

    before = _requests(12, 1000) + _requests(1, 1040)
    after = _requests(12, 700)
    for i, r in enumerate(after):
        r["ts"] += 5000
    res = compare(before, after)
    m = res["metrics"]["input_per_request"]
    assert m["significant"] and m["diff"] < 0 and res["reliable"]
    small = compare(_requests(2, 1000), _requests(2, 900))
    assert not small["reliable"]


def test_compare_noise_is_not_significant():
    from tokprof.analyze import compare

    a = [dict(r, usage={"input_total": 1000 + (i % 3) * 50}) for i, r in enumerate(_requests(20, 0))]
    b = [dict(r, usage={"input_total": 1000 + ((i + 1) % 3) * 50}) for i, r in enumerate(_requests(20, 0))]
    assert not compare(a, b)["metrics"]["input_per_request"]["significant"]


def test_to_legacy_is_compatible_with_token_report_schema():
    from tokprof.analyze import to_legacy

    r = {"ts": 1, "conv": "c1", "model": "m", "usage": {"input_total": 50, "cached": 10, "output": 3},
         "segments": [{"kind": "tool_def", "tokens": 7, "name": "search"}], "tool_calls": ["search"]}
    row = to_legacy([r])[0]
    assert row["input_tokens"] == 50 and row["cached_input_tokens"] == 10 and row["tool_defs"] == [{"name": "search", "tokens": 7}]
    assert row["tool_calls"][0]["name"] == "search" and row["session"] == "c1"


def test_negative_residual_finding_mentions_ollama_kv_cache():
    segs = [seg("system", 800), seg("user_prompt", 50)]
    r = call(1000, 50, segs, quality="poor")  # reported=50, est=850 -> residual=-800
    fs = findings([r])
    assert any("prompt_eval_count" in f["text"] and "KV-cached" in f["text"] for f in fs)


def test_failure_loop_cache_busting_and_context_growth_findings():
    # 1. Repeated identical tool calls + HTTP errors + changing system prompt hashes + 4x history growth
    recs = []
    for i in range(4):
        segs = [
            seg("system", 600, h=f"dynamic-sys-{i}"),
            seg("history_user", 500 * (i + 1)),
            seg("tool_result", 300 * (i + 1)),
            seg("user_prompt", 40),
        ]
        recs.append(
            call(
                1000 + i * 1.0,
                sum(s["tokens"] for s in segs),
                segs,
                conv="conv1",
                tool_calls=["run_bash"],
                status=500 if i < 2 else 200,
            )
        )
    fs = findings(recs)
    joined = " | ".join(f["text"] for f in fs)
    assert "failed with an error or HTTP >= 400" in joined
    assert "Tool call 'run_bash' was repeated 4 times in a row" in joined
    assert "busting the prefix cache" in joined
    assert "Conversation history grew" in joined


def test_pretty_and_html_reports_and_profile_cli(tmp_path, monkeypatch):
    from tokprof.cli import main
    from tokprof.report import render_html, render_pretty

    recs = pipeline()
    pretty = render_pretty(recs, color=False, prices={"m": {"input": 2.0, "output": 8.0}})
    assert "LLM Token Profile" in pretty and "Estimated cost:" in pretty and "Call timeline" in pretty
    html_out = render_html(recs, prices={"m": {"input": 2.0, "output": 8.0}})
    assert "<!DOCTYPE html>" in html_out and "Input Token Breakdown" in html_out and "Per-Call Timeline" in html_out

    monkeypatch.chdir(tmp_path)
    tok_dir = tmp_path / ".tokprof"
    tok_dir.mkdir()
    jpath = tok_dir / "run-test.jsonl"
    import json

    jpath.write_text("\n".join(json.dumps(r) for r in recs) + "\n", encoding="utf-8")
    # Zero-arg `tokprof report --html` finds the latest file in .tokprof/ automatically
    assert main(["report", "--format", "pretty", "--html"]) == 0
    assert (tok_dir / "run-test.html").is_file()
    assert (tok_dir / "latest.html").is_file()
