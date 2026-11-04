import json

from tokprof.parse import parse_request, parse_response_body
from tokprof.parse.request import COMPLETIONS, GEMINI, OPENAI_CHAT, split_chat_template


def kinds(parsed):
    return [(s.kind, s.text) for s in parsed.segments]


def test_raw_prompt_is_one_segment():
    p = parse_request(COMPLETIONS, {"model": "m", "prompt": "Write a poem about the sea."})
    assert [s.kind for s in p.segments] == ["raw_prompt"] and p.model == "m" and not p.stream


def test_chatml_prompt_is_split_into_roles():
    prompt = "<|im_start|>system\nBe brief.<|im_end|>\n<|im_start|>user\nHi<|im_end|>\n<|im_start|>assistant\nHello!<|im_end|>\n<|im_start|>user\nWhat is 2+2?<|im_end|>\n<|im_start|>assistant\n"
    turns = split_chat_template(prompt)
    assert turns == [("system", "Be brief."), ("user", "Hi"), ("assistant", "Hello!"), ("user", "What is 2+2?")]
    p = parse_request(COMPLETIONS, {"prompt": prompt})
    assert [s.kind for s in p.segments] == ["system", "history_user", "history_assistant", "user_prompt"]


def test_llama3_gemma_and_llama2_templates():
    l3 = "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nS<|eot_id|><|start_header_id|>user<|end_header_id|>\n\nQ<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n"
    assert split_chat_template(l3) == [("system", "S"), ("user", "Q")]
    gm = "<start_of_turn>user\nHello<end_of_turn>\n<start_of_turn>model\nHi<end_of_turn>\n<start_of_turn>user\nBye<end_of_turn>\n<start_of_turn>model\n"
    assert split_chat_template(gm) == [("user", "Hello"), ("assistant", "Hi"), ("user", "Bye")]
    l2 = "<s>[INST] <<SYS>>\nRules\n<</SYS>>\n\nFirst [/INST] Answer one </s><s>[INST] Second [/INST]"
    assert split_chat_template(l2) == [("system", "Rules"), ("user", "First"), ("assistant", "Answer one"), ("user", "Second")]


def test_deepseek_phi_and_command_r_templates():
    ds = "<｜begin▁of▁sentence｜><｜System｜>Be brief.<｜User｜>Hi<｜Assistant｜>Hello<｜end▁of▁sentence｜><｜User｜>2+2?<｜Assistant｜>"
    assert split_chat_template(ds) == [("system", "Be brief."), ("user", "Hi"), ("assistant", "Hello"), ("user", "2+2?")]
    phi = "<|system|>\nSystem rule<|end|>\n<|user|>\nQuestion 1<|end|>\n<|assistant|>\nAnswer 1<|end|>\n<|user|>\nQuestion 2<|end|>\n<|assistant|>\n"
    assert split_chat_template(phi) == [("system", "System rule"), ("user", "Question 1"), ("assistant", "Answer 1"), ("user", "Question 2")]
    cr = "<|START_OF_TURN_TOKEN|><|SYSTEM_TOKEN|>Preamble<|END_OF_TURN_TOKEN|><|START_OF_TURN_TOKEN|><|USER_TOKEN|>Hello<|END_OF_TURN_TOKEN|><|START_OF_TURN_TOKEN|><|CHATBOT_TOKEN|>Hi!<|END_OF_TURN_TOKEN|><|START_OF_TURN_TOKEN|><|USER_TOKEN|>Next<|END_OF_TURN_TOKEN|>"
    assert split_chat_template(cr) == [("system", "Preamble"), ("user", "Hello"), ("assistant", "Hi!"), ("user", "Next")]


def test_unknown_template_is_not_guessed():
    assert split_chat_template("just some text with no markers") is None


def test_prompt_batches_and_token_ids():
    assert parse_request(COMPLETIONS, {"prompt": ["a b", "c d"]}).segments[0].text == "a b\nc d"
    ids = parse_request(COMPLETIONS, {"prompt": [1, 2, 3]})
    assert ids.segments[0].opaque and ids.segments[0].text == ""


def test_tgi_inputs_and_stream_parameter():
    p = parse_request(COMPLETIONS, {"inputs": "hello there", "parameters": {"stream": True, "max_new_tokens": 5}})
    assert p.segments[0].text == "hello there" and p.stream


def test_ollama_generate_system_field():
    p = parse_request(COMPLETIONS, {"model": "llama3", "system": "Be terse.", "prompt": "Hi"})
    assert [s.kind for s in p.segments] == ["system", "raw_prompt"]


def test_gemini_request_segments():
    body = {
        "systemInstruction": {"parts": [{"text": "You are helpful."}]},
        "contents": [
            {"role": "user", "parts": [{"text": "Weather?"}]},
            {"role": "model", "parts": [{"functionCall": {"name": "get_weather", "args": {"city": "Seoul"}}}]},
            {"role": "user", "parts": [{"functionResponse": {"name": "get_weather", "response": {"temp": 3}}}]},
            {"role": "user", "parts": [{"text": "Thanks, and tomorrow?"}, {"inlineData": {"mimeType": "image/png", "data": "xx"}}]},
        ],
        "tools": [{"functionDeclarations": [{"name": "get_weather", "description": "d"}]}],
        "generationConfig": {"responseSchema": {"type": "object"}},
    }
    p = parse_request(GEMINI, body)
    got = [s.kind for s in p.segments]
    assert got == ["system", "history_user", "tool_call", "tool_result", "user_prompt", "attachment", "tool_def", "response_schema"]
    assert p.tools == ["get_weather"]


def test_top_level_system_for_generic_chat_bodies():
    p = parse_request(OPENAI_CHAT, {"system": [{"text": "Rules"}], "messages": [{"role": "user", "content": [{"text": "hi"}]}]})
    assert [s.kind for s in p.segments] == ["system", "user_prompt"]


def test_ollama_ndjson_stream_and_whole_response():
    nd = json.dumps({"message": {"role": "assistant", "content": "Hel"}, "done": False}) + "\n" + json.dumps({"message": {"role": "assistant", "content": "lo"}, "done": False}) + "\n" + json.dumps({"done": True, "prompt_eval_count": 12, "eval_count": 3}) + "\n"
    r = parse_response_body(OPENAI_CHAT, nd)
    assert r.has_usage and r.usage.input_total == 12 and r.usage.output == 3 and r.text == "Hello"
    r2 = parse_response_body(OPENAI_CHAT, json.dumps({"message": {"role": "assistant", "content": "x"}, "done": True, "prompt_eval_count": 4, "eval_count": 1}))
    assert r2.usage.input_total == 4 and r2.text == "x"
    gen = parse_response_body(COMPLETIONS, '{"response":"a","done":false}\n{"response":"b","done":true,"prompt_eval_count":7,"eval_count":2}\n')
    assert gen.text == "ab" and gen.usage.input_total == 7


def test_llama_cpp_and_tgi_and_vllm_completions():
    cpp = parse_response_body(COMPLETIONS, json.dumps({"content": "hi", "tokens_evaluated": 30, "tokens_cached": 20, "tokens_predicted": 4}))
    assert cpp.usage.input_total == 30 and cpp.usage.cached == 20 and cpp.usage.output == 4 and cpp.text == "hi"
    tgi = parse_response_body(COMPLETIONS, json.dumps({"generated_text": "x", "details": {"prefill": [0, 1, 2, 3], "generated_tokens": 5}}))
    assert tgi.usage.input_total == 4 and tgi.usage.output == 5
    vllm = parse_response_body(COMPLETIONS, json.dumps({"choices": [{"text": "y"}], "usage": {"prompt_tokens": 9, "completion_tokens": 2}}))
    assert vllm.usage.input_total == 9 and vllm.text == "y"
    stream = parse_response_body(COMPLETIONS, 'data: {"choices":[{"text":"a"}]}\n\ndata: {"choices":[{"text":"b"}],"usage":{"prompt_tokens":6,"completion_tokens":2}}\n\ndata: [DONE]\n\n', sse=True)
    assert stream.text == "ab" and stream.usage.input_total == 6


def test_gemini_json_array_and_sse_responses():
    chunk1 = {"candidates": [{"content": {"parts": [{"text": "a"}]}}]}
    chunk2 = {"candidates": [{"content": {"parts": [{"text": "b"}, {"functionCall": {"name": "f"}}]}}], "usageMetadata": {"promptTokenCount": 11, "cachedContentTokenCount": 5, "candidatesTokenCount": 3, "thoughtsTokenCount": 2}}
    arr = parse_response_body(GEMINI, json.dumps([chunk1, chunk2]))
    assert arr.text == "ab" and arr.tool_calls == ["f"] and arr.usage.input_total == 11 and arr.usage.cached == 5 and arr.usage.reasoning == 2
    sse = parse_response_body(GEMINI, f"data: {json.dumps(chunk1)}\n\ndata: {json.dumps(chunk2)}\n\n", sse=True)
    assert sse.usage.input_total == 11 and sse.text == "ab"


def test_unparseable_response_is_empty_not_an_error():
    assert not parse_response_body(OPENAI_CHAT, "<html>nope</html>").has_usage
    assert not parse_response_body(OPENAI_CHAT, "").has_usage


def test_cohere_v1_and_v2_request_and_response():
    req = {
        "model": "command-r-plus",
        "preamble": "Always answer concisely.",
        "chat_history": [
            {"role": "USER", "message": "Hi"},
            {"role": "CHATBOT", "message": "Hello!"},
        ],
        "message": "What is tokenomics?",
        "tools": [{"name": "lookup", "description": "search"}],
    }
    p = parse_request(OPENAI_CHAT, req)
    assert [s.kind for s in p.segments] == ["system", "history_user", "history_assistant", "user_prompt", "tool_def"]
    assert p.tools == ["lookup"]

    r_v1 = parse_response_body(OPENAI_CHAT, json.dumps({"text": "Done", "tool_calls": [{"name": "lookup"}], "meta": {"billed_units": {"input_tokens": 42, "output_tokens": 7}}}))
    assert r_v1.has_usage and r_v1.usage.input_total == 42 and r_v1.usage.output == 7 and r_v1.text == "Done" and r_v1.tool_calls == ["lookup"]

    r_v2 = parse_response_body(OPENAI_CHAT, json.dumps({"message": {"role": "assistant", "content": [{"type": "text", "text": "v2 reply"}]}, "usage": {"tokens": {"input_tokens": 55, "output_tokens": 9}}}))
    assert r_v2.has_usage and r_v2.usage.input_total == 55 and r_v2.usage.output == 9 and r_v2.text == "v2 reply"


def test_bedrock_converse_and_binary_eventstream():
    import base64
    import struct
    from tokprof.parse.request import ANTHROPIC

    req = {
        "system": [{"text": "You are an AWS assistant."}],
        "messages": [
            {"role": "user", "content": [{"text": "Check status"}]},
            {"role": "assistant", "content": [{"toolUse": {"toolUseId": "t1", "name": "get_status", "input": {"id": 1}}}]},
            {"role": "user", "content": [{"toolResult": {"toolUseId": "t1", "content": [{"text": "healthy"}]}}]},
            {"role": "user", "content": [{"text": "Summarize"}]},
        ],
        "toolConfig": {"tools": [{"toolSpec": {"name": "get_status", "description": "d"}}]},
    }
    p = parse_request(OPENAI_CHAT, req)
    assert [s.kind for s in p.segments] == ["system", "history_user", "tool_call", "tool_result", "user_prompt", "tool_def"]
    assert p.tools == ["get_status"]

    # Non-streaming Converse response
    resp = {
        "output": {"message": {"role": "assistant", "content": [{"text": "All good"}, {"toolUse": {"name": "get_status"}}]}},
        "usage": {"inputTokens": 120, "outputTokens": 15, "cacheReadInputTokens": 80},
    }
    r = parse_response_body(OPENAI_CHAT, json.dumps(resp))
    assert r.has_usage and r.usage.input_total == 120 and r.usage.cached == 80 and r.usage.output == 15
    assert r.text == "All good" and r.tool_calls == ["get_status"]

    # Binary AWS EventStream framing (12-byte prelude + headers + JSON payload + 4-byte CRC)
    def make_frame(payload_dict):
        payload = json.dumps(payload_dict).encode("utf-8")
        headers = b"\x0b:event-type\x07\x00\x05chunk"
        total_len = 12 + len(headers) + len(payload) + 4
        return struct.pack(">III", total_len, len(headers), 0) + headers + payload + b"\x00\x00\x00\x00"

    converse_stream = (
        make_frame({"contentBlockDelta": {"delta": {"text": "Hello "}, "contentBlockIndex": 0}})
        + make_frame({"contentBlockDelta": {"delta": {"text": "world"}, "contentBlockIndex": 0}})
        + make_frame({"metadata": {"usage": {"inputTokens": 95, "outputTokens": 12, "cacheReadInputTokens": 40}}})
    )
    rs = parse_response_body(OPENAI_CHAT, converse_stream)
    assert rs.has_usage and rs.usage.input_total == 95 and rs.usage.cached == 40 and rs.usage.output == 12 and rs.text == "Hello world"

    # Base64-wrapped Anthropic chunk inside invoke-with-response-stream decoded as UTF-8 text
    b64_chunk = base64.b64encode(json.dumps({"type": "message_delta", "usage": {"output_tokens": 11}, "amazon-bedrock-invocationMetrics": {"inputTokenCount": 77, "outputTokenCount": 11}}).encode()).decode()
    text_stream = make_frame({"bytes": b64_chunk}).decode("utf-8", "replace")
    ra = parse_response_body(ANTHROPIC, text_stream)
    assert ra.has_usage and ra.usage.input_total == 77 and ra.usage.output == 11
