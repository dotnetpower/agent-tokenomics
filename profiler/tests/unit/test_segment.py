import json

from tokprof.counting import HeuristicCounter
from tokprof.model import Usage
from tokprof.parse import OPENAI_CHAT, ParsedResponse, parse_request
from tokprof.segment import build_record, build_segments, hash_text, split_sections

SALT = b"test-salt"
C = HeuristicCounter()

LONG_SYSTEM = (
    "# Role\n" + "You are a careful assistant. " * 20 + "\n"
    "# Rules\n" + "Always cite sources. " * 25 + "\n"
    "# Style\n" + "Be brief and direct. " * 25 + "\n"
)


def test_split_sections_by_heading():
    parts = split_sections(LONG_SYSTEM)
    assert [n for n, _ in parts] == ["Role", "Rules", "Style"]
    assert "".join(b for _, b in parts) == LONG_SYSTEM


def test_split_sections_small_or_unstructured_stays_whole():
    assert split_sections("just one short instruction") == [(None, "just one short instruction")]
    assert split_sections("# A\nshort\n# B\nshort") == [(None, "# A\nshort\n# B\nshort")]


def test_split_sections_xml_tags():
    text = "<rules>\n" + "x " * 150 + "\n</rules>\n<tone>\n" + "y " * 150 + "\n</tone>\n"
    names = [n for n, _ in split_sections(text)]
    assert "rules" in names and "tone" in names


def test_hash_is_salted_and_stable():
    assert hash_text("a", b"s1") == hash_text("a", b"s1")
    assert hash_text("a", b"s1") != hash_text("a", b"s2")
    assert "a" not in hash_text("a", b"s1")


def test_components_are_attributed_by_name():
    doc = "REFERENCE DOCUMENT " * 40
    body = {"model": "m", "messages": [{"role": "system", "content": "intro\n" + doc + "\noutro"}, {"role": "user", "content": "q"}]}
    parsed = parse_request(OPENAI_CHAT, body)
    segs = build_segments(parsed.segments, C, SALT, components={"ref_doc": doc})
    named = [s for s in segs if s.name == "ref_doc"]
    assert len(named) == 1 and named[0].tokens >= C.count(doc) - 1


def test_record_residual_and_quality():
    body = {"model": "gpt-4o", "messages": [{"role": "system", "content": "s" * 400}, {"role": "user", "content": "u" * 400}]}
    parsed = parse_request(OPENAI_CHAT, body)
    est = sum(C.count(m["content"]) for m in body["messages"])
    resp = ParsedResponse(usage=Usage(input_total=est + 8, output=3), has_usage=True, tool_calls=["f"], text="hello")
    rec = build_record(parsed=parsed, response=resp, counter=C, salt=SALT, run="r1")
    assert rec.est_input == est and rec.residual == 8 and rec.quality == "good"
    d = rec.to_dict()
    assert json.dumps(d)  # serializable
    assert "s" * 50 in json.dumps(d) and rec.response_text == "hello"  # prompt and response text stored by default
    no_txt = build_record(parsed=parsed, response=resp, counter=C, salt=SALT, run="r1", include_text=False).to_dict()
    assert "s" * 50 not in json.dumps(no_txt) and no_txt["response_text"] is None
    assert rec.tool_calls == ["f"] and rec.conv and rec.sysfp


def test_structured_json_user_message_decomposition():
    payload = {
        "untrusted_input": {
            "utterance": "현재 프로덕션 DB 백업 상태 확인해줘",
            "context": ["user: 안녕하세요", "assistant: 반갑습니다! 무엇을 도와드릴까요?"],
            "capabilities": [{"kind": "function_type", "name": "query.resource_current_state"}],
            "locale": "ko",
        }
    }
    body = {
        "model": "narrator-gpt-5-4-mini",
        "messages": [
            {"role": "system", "content": "You are an operational assistant."},
            {"role": "user", "content": json.dumps(payload, ensure_ascii=False)},
        ],
        "response_format": {"type": "json_schema", "json_schema": {"name": "semantic-judgment", "schema": {"type": "object"}}},
    }
    parsed = parse_request(OPENAI_CHAT, body)
    kinds = [s.kind for s in parsed.segments]
    assert "system" in kinds
    assert "history_user" in kinds and "history_assistant" in kinds
    assert "user_prompt" in kinds and "tool_def" in kinds and "response_schema" in kinds
    rec = build_record(parsed=parsed, response=ParsedResponse(text='{"intent":"db"}'), counter=C, salt=SALT)
    assert rec.sys_label == "semantic-judgment"
    by_kind = {s["kind"]: s for s in rec.segments}
    assert by_kind["user_prompt"]["text"] == "현재 프로덕션 DB 백업 상태 확인해줘"
    assert "안녕하세요" in by_kind["history_user"]["text"]
    assert "반갑습니다" in by_kind["history_assistant"]["text"]


def test_record_without_reported_usage_has_no_residual():
    parsed = parse_request(OPENAI_CHAT, {"model": "m", "messages": [{"role": "user", "content": "hi"}]})
    rec = build_record(parsed=parsed, response=None, counter=C, salt=SALT)
    assert rec.residual is None and rec.quality is None and rec.usage == {}


def test_poor_quality_when_residual_is_large():
    parsed = parse_request(OPENAI_CHAT, {"model": "m", "messages": [{"role": "user", "content": "x" * 40}]})
    resp = ParsedResponse(usage=Usage(input_total=1000), has_usage=True)
    assert build_record(parsed=parsed, response=resp, counter=C, salt=SALT).quality == "poor"


def test_tiktoken_decomposition_matches_openai_chat_token_formula():
    import pytest

    tiktoken = pytest.importorskip("tiktoken")
    from tokprof.counting import get_counter

    enc = tiktoken.get_encoding("o200k_base")
    sys_text = (
        "# Role\nYou are an autonomous software engineering agent that diagnoses token usage.\n" * 8
        + "# Rules\nAlways verify before claiming success. Keep diffs minimal and focused.\n" * 10
    )
    tools = [
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file range from disk and return its numbered lines.",
                "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]},
            },
        }
    ]
    messages = [
        {"role": "system", "content": sys_text},
        {"role": "user", "content": "Check why tests/unit/test_parse.py is failing."},
        {
            "role": "assistant",
            "content": None,
            "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "read_file", "arguments": '{"path":"a.py"}'}}],
        },
        {"role": "tool", "tool_call_id": "c1", "name": "read_file", "content": "def test_parse():\n    assert 1 == 1\n" * 20},
        {"role": "user", "content": "Now run the test suite and summarize the result."},
    ]
    body = {"model": "gpt-4o", "messages": messages, "tools": tools}
    exact_tokens = 3
    for m in messages:
        exact_tokens += 3
        if m.get("content"):
            exact_tokens += len(enc.encode(m["content"]))
        for tc in m.get("tool_calls") or []:
            fn = tc["function"]
            exact_tokens += len(enc.encode(fn["name"])) + len(enc.encode(fn["arguments"])) + 3
    for t in tools:
        exact_tokens += len(enc.encode(json.dumps(t, separators=(",", ":"), sort_keys=True)))

    counter = get_counter("gpt-4o", prefer_tiktoken=True)
    parsed = parse_request(OPENAI_CHAT, body)
    rec = build_record(
        parsed=parsed,
        response=ParsedResponse(usage=Usage(input_total=exact_tokens, output=15), has_usage=True),
        counter=counter,
        salt=SALT,
    )
    assert counter.method == "tiktoken:o200k_base"
    assert rec.quality == "good"
    assert abs(rec.residual) / exact_tokens <= 0.05


def test_stability_and_perf_hardening_across_5_rounds():
    from tokprof.counting import get_counter
    from tokprof.gateway import BodyFramer
    from tokprof.report import render_csv
    from tokprof.segment import quality_label, redact_secrets

    # Round 1: Expanded secret redaction (AWS, Google, Slack, JWT) and lone surrogate handling
    aws_key = "AKIA" + "0123456789ABCDEF"
    gcp_key = "AIza" + ("A" * 35)
    jwt_tok = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"
    red = redact_secrets(f"aws={aws_key} gcp={gcp_key} jwt={jwt_tok}")
    assert aws_key not in red and gcp_key not in red and jwt_tok not in red
    assert red.count("***REDACTED***") == 3

    tk = get_counter("openai/gpt-5-mini", prefer_tiktoken=True)
    assert tk.count("surrogate \ud800 test") > 0
    assert quality_label(10, 0) is None
    assert quality_label(10, -5) is None

    # Round 3: BodyFramer comma-separated Content-Length & multi-chunk single-buffer feed
    bf_cl = BodyFramer({"content-length": "5, 5"}, False)
    assert bf_cl.feed(b"hello world") == b"hello" and bf_cl.done
    bf_ch = BodyFramer({"transfer-encoding": "chunked"}, False)
    assert bf_ch.feed(b"3\r\nabc\r\n2;ext=1\r\nde\r\n0\r\n\r\n") == b"abcde" and bf_ch.done

    # Round 4: CSV NUL byte and CR sanitization
    rec = {
        "id": "c1",
        "ts": 100.0,
        "model": "gpt-4o",
        "provider": "openai_chat",
        "sysfp": "abcdef",
        "usage": {"input_total": 10, "output": 5},
        "segments": [{"kind": "user_prompt", "tokens": 10, "text": "hello\x00world\r\nline2"}],
        "response_text": "resp\x00ok",
    }
    csv_text = render_csv([rec])
    assert "\x00" not in csv_text and "helloworld" in csv_text and "respok" in csv_text

