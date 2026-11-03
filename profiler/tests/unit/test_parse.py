import json

from tokprof.parse import (
    ANTHROPIC, OPENAI_CHAT, OPENAI_RESPONSES, SSEParser, detect_provider, parse_request, parse_response_json,
)


def kinds(parsed):
    return [s.kind for s in parsed.segments]


def test_detect_provider_by_path():
    assert detect_provider("/v1/chat/completions", {}) == OPENAI_CHAT
    assert detect_provider("/openai/deployments/d/chat/completions?api-version=1", {}) == OPENAI_CHAT
    assert detect_provider("/v1/responses", {}) == OPENAI_RESPONSES
    assert detect_provider("/v1/messages", {}) == ANTHROPIC
    assert detect_provider("/v1/messages/count_tokens", {}) is None
    assert detect_provider("/v1/embeddings", {"model": "x", "input": "y"}) is None


def test_openai_chat_decomposition():
    body = {
        "model": "gpt-4o",
        "messages": [
            {"role": "system", "content": "sys"},
            {"role": "user", "content": "first question"},
            {"role": "assistant", "content": "", "tool_calls": [{"function": {"name": "search", "arguments": "{\"q\":1}"}}]},
            {"role": "tool", "tool_call_id": "c1", "content": "result text"},
            {"role": "assistant", "content": "answer"},
            {"role": "user", "content": [{"type": "text", "text": "follow up"}, {"type": "image_url", "image_url": {"url": "x"}}]},
        ],
        "tools": [{"type": "function", "function": {"name": "search", "parameters": {}}}],
    }
    p = parse_request(OPENAI_CHAT, body)
    ks = kinds(p)
    assert ks.count("user_prompt") == 1 and ks.count("history_user") == 1
    assert "tool_call" in ks and "tool_result" in ks and "tool_def" in ks and "attachment" in ks
    cur = next(s for s in p.segments if s.kind == "user_prompt")
    assert cur.text == "follow up"
    assert p.tools == ["search"]


def test_openai_responses_decomposition():
    body = {
        "model": "gpt-4o",
        "instructions": "be brief",
        "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "q1"}]},
            {"type": "function_call", "name": "lookup", "arguments": "{}", "call_id": "1"},
            {"type": "function_call_output", "call_id": "1", "output": "data"},
            {"role": "user", "content": "q2"},
        ],
        "tools": [{"type": "function", "name": "lookup"}],
    }
    p = parse_request(OPENAI_RESPONSES, body)
    assert kinds(p).count("system") == 1
    assert "tool_call" in kinds(p) and "tool_result" in kinds(p)
    assert next(s for s in p.segments if s.kind == "user_prompt").text == "q2"


def test_anthropic_decomposition():
    body = {
        "model": "claude-x", "system": [{"type": "text", "text": "rules"}],
        "messages": [
            {"role": "user", "content": "ask"},
            {"role": "assistant", "content": [{"type": "text", "text": "ok"}, {"type": "tool_use", "name": "calc", "input": {"a": 1}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t", "content": "42"}]},
            {"role": "user", "content": [{"type": "text", "text": "thanks"}]},
        ],
        "tools": [{"name": "calc", "input_schema": {}}],
    }
    p = parse_request(ANTHROPIC, body)
    ks = kinds(p)
    assert "tool_call" in ks and "tool_result" in ks and "tool_def" in ks
    assert next(s for s in p.segments if s.kind == "user_prompt").text == "thanks"


def test_usage_json_normalization():
    chat = parse_response_json(OPENAI_CHAT, {"usage": {"prompt_tokens": 100, "completion_tokens": 5, "prompt_tokens_details": {"cached_tokens": 60}, "completion_tokens_details": {"reasoning_tokens": 2}}})
    assert (chat.usage.input_total, chat.usage.cached, chat.usage.output, chat.usage.reasoning) == (100, 60, 5, 2)
    resp = parse_response_json(OPENAI_RESPONSES, {"usage": {"input_tokens": 50, "output_tokens": 7, "input_tokens_details": {"cached_tokens": 10}}})
    assert (resp.usage.input_total, resp.usage.cached) == (50, 10)
    # Anthropic reports input_tokens without cached tokens: the total must include them.
    an = parse_response_json(ANTHROPIC, {"usage": {"input_tokens": 20, "cache_read_input_tokens": 80, "cache_creation_input_tokens": 5, "output_tokens": 9}})
    assert an.usage.input_total == 105 and an.usage.cached == 80 and an.usage.cache_write == 5


def test_sse_chat_with_split_chunks_and_tool_name():
    ev1 = "data: " + json.dumps({"choices": [{"delta": {"tool_calls": [{"function": {"name": "get_weather"}}]}}]}) + "\n\n"
    ev2 = "data: " + json.dumps({"choices": [{"delta": {"content": "hi"}}]}) + "\n\n"
    ev3 = "data: " + json.dumps({"choices": [], "usage": {"prompt_tokens": 9, "completion_tokens": 1}}) + "\n\ndata: [DONE]\n\n"
    raw = (ev1 + ev2 + ev3).encode()
    p = SSEParser(OPENAI_CHAT)
    for i in range(0, len(raw), 7):  # arbitrary fragmentation
        p.feed(raw[i:i + 7])
    r = p.finish()
    assert r.tool_calls == ["get_weather"] and r.text == "hi" and r.usage.input_total == 9 and r.has_usage


def test_sse_anthropic_merges_start_and_delta():
    raw = (
        "event: message_start\ndata: " + json.dumps({"type": "message_start", "message": {"usage": {"input_tokens": 12, "cache_read_input_tokens": 3}}}) + "\n\n"
        "event: message_delta\ndata: " + json.dumps({"type": "message_delta", "usage": {"output_tokens": 8}}) + "\n\n"
    ).encode().replace(b"\n", b"\r\n")
    p = SSEParser(ANTHROPIC)
    p.feed(raw)
    r = p.finish()
    assert r.usage.input_total == 15 and r.usage.output == 8


def test_sse_ignores_garbage():
    p = SSEParser(OPENAI_CHAT)
    p.feed(b"data: {not json}\n\n: comment\n\ndata: [DONE]\n\n")
    assert not p.finish().has_usage
