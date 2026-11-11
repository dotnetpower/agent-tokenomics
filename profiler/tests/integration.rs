use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use tokprof::analyze::summarize;
use tokprof::appinsights::{
    build_app_insights_kql, classify_azure_error, inspect_records_content,
    parse_app_insights_payload, repair_truncated_json, report_candidate_ports,
    resolve_azure_cloud_endpoints, IngestOptions,
};
use tokprof::cli::run_cli;
use tokprof::detect::{Detector, plausible};
use tokprof::gateway::{Gateway, GatewayServer, Route};
use tokprof::help_text::render_detailed_help;
use tokprof::parse_request::{
    ANTHROPIC, COMPLETIONS, GEMINI, OPENAI_CHAT, OPENAI_RESPONSES, parse_request,
};
use tokprof::parse_response::{parse_response_body, parse_response_json};
use tokprof::report::{render_csv, render_html, render_json, render_markdown};
use tokprof::runner::run_profiled;
use tokprof::store::{JsonlWriter, read_jsonl};

#[test]
fn test_prefilter_shared_vectors() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vec_path = manifest_dir.join("tests/vectors/prefilter.json");
    let raw = fs::read_to_string(&vec_path).expect("read prefilter.json");
    let doc: Value = serde_json::from_str(&raw).expect("parse prefilter.json");
    let cases = doc
        .get("cases")
        .and_then(|v| v.as_array())
        .expect("cases array");
    assert!(!cases.is_empty());

    for case in cases {
        let name = case.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let method = case.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let path = case.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let body = case.get("body").and_then(|v| v.as_str()).unwrap_or("");
        let expect = case
            .get("expect")
            .and_then(|v| v.as_bool())
            .expect("expect bool");
        let got = plausible(method, path, body);
        assert_eq!(
            got, expect,
            "prefilter mismatch on case {name:?}: method={method} path={path}"
        );
    }
}

#[test]
fn test_selfhosted_and_cloud_parsers() {
    let det = Detector::new(None);

    // 1. Ollama /api/chat + NDJSON response
    let ollama_req: Value = serde_json::from_str(
        r#"{"model":"llama3.1:8b","messages":[{"role":"system","content":"Be concise."},{"role":"user","content":"Hello Ollama"}]}"#,
    )
    .unwrap();
    let v_ollama = det.inspect("POST", "gpu-box.lan", 11434, "/api/chat", Some(&ollama_req));
    assert!(v_ollama.llm);
    assert_eq!(v_ollama.api.as_deref(), Some(OPENAI_CHAT));

    let parsed_ollama = parse_request(OPENAI_CHAT, &ollama_req).expect("parse ollama request");
    assert_eq!(parsed_ollama.model.as_deref(), Some("llama3.1:8b"));
    assert_eq!(parsed_ollama.segments.len(), 2);

    let r_ollama = parse_response_body(
        OPENAI_CHAT,
        r#"{"model":"llama3.1:8b","message":{"role":"assistant","content":"Hi "},"done":false}
{"model":"llama3.1:8b","message":{"role":"assistant","content":"there!"},"done":true,"prompt_eval_count":14,"eval_count":6}
"#,
        false,
    );
    assert_eq!(r_ollama.usage.input_total, Some(14));
    assert_eq!(r_ollama.usage.output, Some(6));
    assert_eq!(r_ollama.text, "Hi there!");

    // 2. HuggingFace TGI /generate
    let tgi_req: Value = serde_json::from_str(
        r#"{"inputs":"What is deep learning?","parameters":{"max_new_tokens":32}}"#,
    )
    .unwrap();
    let v_tgi = det.inspect("POST", "10.0.0.12", 8080, "/generate", Some(&tgi_req));
    assert!(v_tgi.llm);
    assert_eq!(v_tgi.api.as_deref(), Some(COMPLETIONS));
    let parsed_tgi = parse_request(COMPLETIONS, &tgi_req).expect("parse tgi");
    assert_eq!(parsed_tgi.segments.len(), 1);

    // 3. llama.cpp /completion
    let llamacpp_req: Value =
        serde_json::from_str(r#"{"prompt":"Once upon a time","n_predict":16}"#).unwrap();
    let v_llama = det.inspect("POST", "127.0.0.1", 8080, "/completion", Some(&llamacpp_req));
    assert!(v_llama.llm);
    let llama_resp: Value = serde_json::from_str(
        r#"{"content":" there was a rustacean.","tokens_evaluated":5,"tokens_predicted":6}"#,
    )
    .unwrap();
    let r_llama = parse_response_json(COMPLETIONS, &llama_resp);
    assert_eq!(r_llama.usage.input_total, Some(5));
    assert_eq!(r_llama.usage.output, Some(6));
    assert_eq!(r_llama.text, " there was a rustacean.");

    // 4. AWS Bedrock Converse
    let bedrock_req: Value = serde_json::from_str(
        r#"{"system":[{"text":"You are helpful."}],"messages":[{"role":"user","content":[{"text":"Explain eBPF"}]}]}"#,
    )
    .unwrap();
    let v_bedrock = det.inspect(
        "POST",
        "bedrock-runtime.us-east-1.amazonaws.com",
        443,
        "/model/anthropic.claude-3-5-sonnet-20241022-v2:0/converse",
        Some(&bedrock_req),
    );
    assert!(v_bedrock.llm);
    assert_eq!(
        v_bedrock.model_hint.as_deref(),
        Some("anthropic.claude-3-5-sonnet-20241022-v2:0")
    );
    let parsed_bedrock = parse_request(OPENAI_CHAT, &bedrock_req).expect("parse bedrock");
    assert_eq!(parsed_bedrock.segments.len(), 2);
    let bedrock_resp: Value = serde_json::from_str(
        r#"{"output":{"message":{"role":"assistant","content":[{"text":"eBPF runs in the kernel."}]}},"usage":{"inputTokens":22,"outputTokens":8}}"#,
    )
    .unwrap();
    let r_bedrock = parse_response_json(OPENAI_CHAT, &bedrock_resp);
    assert_eq!(r_bedrock.usage.input_total, Some(22));
    assert_eq!(r_bedrock.usage.output, Some(8));
    assert_eq!(r_bedrock.text, "eBPF runs in the kernel.");

    // 5. Google Gemini generateContent
    let gemini_req: Value = serde_json::from_str(
        r#"{"systemInstruction":{"parts":[{"text":"Be brief"}]},"contents":[{"role":"user","parts":[{"text":"Hi Gemini"}]}]}"#,
    )
    .unwrap();
    let v_gemini = det.inspect(
        "POST",
        "generativelanguage.googleapis.com",
        443,
        "/v1beta/models/gemini-2.5-flash:generateContent",
        Some(&gemini_req),
    );
    assert!(v_gemini.llm);
    assert_eq!(v_gemini.api.as_deref(), Some(GEMINI));
    let gemini_resp: Value = serde_json::from_str(
        r#"{"candidates":[{"content":{"parts":[{"text":"Hello from Gemini!"}]}}],"usageMetadata":{"promptTokenCount":12,"candidatesTokenCount":5,"cachedContentTokenCount":4,"thoughtsTokenCount":2}}"#,
    )
    .unwrap();
    let r_gemini = parse_response_json(GEMINI, &gemini_resp);
    assert_eq!(r_gemini.usage.input_total, Some(12));
    assert_eq!(r_gemini.usage.cached, Some(4));
    assert_eq!(r_gemini.usage.output, Some(5));
    assert_eq!(r_gemini.usage.reasoning, Some(2));
    assert_eq!(r_gemini.text, "Hello from Gemini!");

    // 6. OpenAI Responses API & Anthropic Messages
    let resp_req: Value = serde_json::from_str(
        r#"{"model":"gpt-4.1","instructions":"Act as an architect.","input":[{"role":"user","content":"Design a cache"}]}"#,
    )
    .unwrap();
    let parsed_resp = parse_request(OPENAI_RESPONSES, &resp_req).expect("parse responses");
    assert_eq!(parsed_resp.segments.len(), 2);

    let anth_req: Value = serde_json::from_str(
        r#"{"model":"claude-sonnet-4-20250514","max_tokens":256,"system":[{"type":"text","text":"Follow rules","cache_control":{"type":"ephemeral"}}],"messages":[{"role":"user","content":"Hi Claude"}]}"#,
    )
    .unwrap();
    let parsed_anth = parse_request(ANTHROPIC, &anth_req).expect("parse anthropic");
    assert_eq!(parsed_anth.segments.len(), 2);
}

fn drain_http_request(stream: &mut TcpStream) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let mut header_end = None;
    let mut content_len = 0usize;
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if header_end.is_none() {
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(pos + 4);
                        let head = String::from_utf8_lossy(&buf[..pos]);
                        for line in head.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.trim().eq_ignore_ascii_case("content-length") {
                                    content_len = v.trim().parse::<usize>().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
                if let Some(he) = header_end {
                    if buf.len() >= he + content_len {
                        break;
                    }
                }
            }
        }
    }
}

#[test]
fn test_gateway_auth_ingest_and_proxy() {
    // Start a mock upstream LLM server
    let mock_listener = TcpListener::bind("127.0.0.1:0").expect("bind mock upstream");
    let mock_port = mock_listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in mock_listener.incoming().flatten() {
            let mut s = stream;
            drain_http_request(&mut s);
            let body = r#"{"id":"chatcmpl-1","model":"gpt-4o-mini","choices":[{"message":{"role":"assistant","content":"proxied ok"}}],"usage":{"prompt_tokens":18,"completion_tokens":4}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
            let _ = s.shutdown(std::net::Shutdown::Write);
        }
    });

    let tmp = std::env::temp_dir().join(format!("tokprof-gw-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let jsonl_path = tmp.join("calls.jsonl");
    let jsonl_str = jsonl_path.to_string_lossy().into_owned();
    let writer = Arc::new(JsonlWriter::new(&jsonl_str).unwrap());
    let w_clone = writer.clone();

    let routes = vec![Route::from_urls(
        "/o/v1",
        &format!("http://127.0.0.1:{mock_port}/v1"),
    )];

    let gw = Gateway::new(
        routes,
        Arc::new(move |rec| w_clone.write_record(&rec)),
        vec![0u8; 16],
        HashMap::new(),
        Some("testrun".to_string()),
        HashSet::new(),
        false,
        Some("secret-token-123".to_string()),
        None,
        false,
    );
    let mut srv = GatewayServer::start(gw).expect("start gateway");
    let gw_port = srv.port;

    // 1. Unauthorized ingest should return 403
    {
        let mut s = TcpStream::connect(("127.0.0.1", gw_port)).unwrap();
        let req = "POST /__tokprof/ingest HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
        s.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(resp.starts_with("HTTP/1.1 403"), "expected 403, got: {resp}");
    }

    // 2. Authorized ingest should return 204
    {
        let payload = json!({
            "method": "POST",
            "host": "api.openai.com",
            "port": 443,
            "path": "/v1/chat/completions",
            "status": 200,
            "latency_ms": 42,
            "request": r#"{"model":"gpt-4o","messages":[{"role":"user","content":"ingested hi"}]}"#,
            "response": r#"{"id":"1","model":"gpt-4o","choices":[{"message":{"role":"assistant","content":"ingested reply"}}],"usage":{"prompt_tokens":11,"completion_tokens":3}}"#,
            "sse": false,
        })
        .to_string();
        let mut s = TcpStream::connect(("127.0.0.1", gw_port)).unwrap();
        let req = format!(
            "POST /__tokprof/ingest HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Tokprof-Token: secret-token-123\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            payload.len(),
            payload
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(resp.starts_with("HTTP/1.1 204"), "expected 204, got: {resp}");
    }

    // 3. Proxy route /o/v1/chat/completions
    {
        let body = r#"{"model":"gpt-4o-mini","messages":[{"role":"user","content":"proxy hi"}]}"#;
        let mut s = TcpStream::connect(("127.0.0.1", gw_port)).unwrap();
        let req = format!(
            "POST /o/v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(resp.starts_with("HTTP/1.1 200"), "expected 200, got: {resp}");
        assert!(resp.contains("proxied ok"));
    }

    // 4. Health endpoint
    {
        let mut s = TcpStream::connect(("127.0.0.1", gw_port)).unwrap();
        let req = "GET /__tokprof/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
        s.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        assert!(
            resp.contains("\"recorded\":2") || resp.contains("\"recorded\": 2"),
            "expected 2 recorded calls, got: {resp}"
        );
    }

    srv.stop();
    let records = read_jsonl(&jsonl_str, None);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].response_text.as_deref(), Some("ingested reply"));
    assert_eq!(records[1].response_text.as_deref(), Some("proxied ok"));

    // 5. Verify report generation (Markdown, CSV, HTML, JSON) and bilingual help
    let summary = summarize(&records, 5.0);
    let md = render_markdown(&records, 5.0, Some(&summary));
    let csv = render_csv(&records, 5.0, Some(&summary));
    let html = render_html(&records, 5.0, None, "Integration Report", Some(&summary));
    let js = render_json(&records, 5.0, Some(&summary));
    assert!(md.contains("# Token profile"));
    assert!(csv.contains("session,turn,call_index"));
    assert!(html.to_ascii_lowercase().contains("<!doctype html>"));
    assert!(js.contains("\"calls\": 2"));

    let help_en = render_detailed_help(Some("en"), None);
    let help_ko = render_detailed_help(Some("ko"), None);
    assert!(help_en.contains("QUICK START"));
    assert!(help_ko.contains("빠른 시작"));

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_python_inprocess_hook_via_runner() {
    // Start a local cleartext mock LLM server (simulating self-hosted vLLM on a random port)
    let mock_listener = TcpListener::bind("127.0.0.1:0").expect("bind mock vllm");
    let mock_port = mock_listener.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = mock_listener.accept() {
            drain_http_request(&mut s);
            let body = r#"{"id":"cmpl-vllm","model":"Qwen/Qwen2.5-7B-Instruct","choices":[{"message":{"role":"assistant","content":"vllm hooked ok"}}],"usage":{"prompt_tokens":15,"completion_tokens":5}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
            let _ = s.shutdown(std::net::Shutdown::Write);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });

    let tmp = std::env::temp_dir().join(format!("tokprof-hook-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let jsonl_path = tmp.join("hook.jsonl");
    let jsonl_str = jsonl_path.to_string_lossy().into_owned();

    // Python child calls the mock URL directly via urllib.request without using OPENAI_BASE_URL,
    // exercising the embedded sitecustomize.py + _tokprof_hook.py in-process hook!
    let py_code = format!(
        "import urllib.request, json; req = urllib.request.Request('http://127.0.0.1:{mock_port}/v1/chat/completions', data=json.dumps({{'model':'Qwen/Qwen2.5-7B-Instruct','messages':[{{'role':'user','content':'hi vllm'}}]}}).encode(), headers={{'Content-Type':'application/json'}}, method='POST'); urllib.request.urlopen(req).read()"
    );
    let py_bin = if cfg!(windows) { "python" } else { "python3" };
    let cmd = vec![py_bin.to_string(), "-c".to_string(), py_code];
    let rc = run_profiled(
        &cmd,
        &jsonl_str,
        HashMap::new(),
        &[],
        false,
        true,
        false,
        true,
    );
    assert_eq!(rc, 0);

    let records = read_jsonl(&jsonl_str, None);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].model.as_deref(), Some("Qwen/Qwen2.5-7B-Instruct"));
    assert_eq!(
        records[0].usage.get("input_total").and_then(|v| v.as_i64()),
        Some(15)
    );
    assert_eq!(
        records[0].usage.get("output").and_then(|v| v.as_i64()),
        Some(5)
    );

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_cloud_shell_detection_and_ports() {
    let normal_ports = report_candidate_ports(None, false);
    assert_eq!(normal_ports[0], 18765);

    let cs_ports = report_candidate_ports(None, true);
    assert_eq!(cs_ports[0], 8080);
    assert!(cs_ports.contains(&8000));
    assert!(cs_ports.contains(&8888));

    let custom_ports = report_candidate_ports(Some(8085), true);
    assert_eq!(custom_ports[0], 8085);
    assert_eq!(custom_ports[1], 8080);

    let kql = build_app_insights_kql("6h", Some("conv-99"), Some("my-agent"));
    assert!(kql.contains("AppDependencies"));
    assert!(kql.contains("AppTraces"));
    assert!(kql.contains("ago(6h)"));
    assert!(kql.contains("conv-99"));
    assert!(kql.contains("my-agent"));
}

#[test]
fn test_app_insights_tier1_tier2_truncation_and_sampling() {
    // 1. Direct unit test of repair_truncated_json
    let broken = r#"{"model":"gpt-4o","messages":[{"role":"system","content":"You are an Azure agent."},{"role":"user","content":"Analyze logs that got cut off right here"#;
    let (repaired, flag) = repair_truncated_json(broken);
    assert!(flag);
    let parsed_repaired: Value = serde_json::from_str(&repaired).expect("repaired JSON must parse");
    assert_eq!(parsed_repaired["model"], "gpt-4o");
    assert!(
        parsed_repaired["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("[truncated by App Insights]")
    );

    // 2. Build a Log Analytics `tables` response with:
    //    - Call 1 (op-1, span-1): Full OTel GenAI span + AppTraces message events (system, history, tool_def, tool_call, tool_result, user_prompt, choice)
    //    - Call 2 (op-1, span-2): APIM / custom `request_body` truncated mid-string at property limit
    //    - Call 3 (op-2, span-3): Content Recording disabled (Tier 2 usage_only) + adaptive sampling (ItemCount = 4)
    let tools_json = json!([
        {
            "type": "function",
            "function": {
                "name": "query_kusto",
                "description": "Execute a KQL query against Azure Monitor",
                "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}
            }
        },
        {
            "type": "function",
            "function": {
                "name": "unused_ restart_vm",
                "description": "Restart an Azure VM instance",
                "parameters": {"type": "object", "properties": {"vm_id": {"type": "string"}}}
            }
        }
    ]);
    let tool_calls_json = json!([
        {
            "id": "call_kql_1",
            "type": "function",
            "function": {"name": "query_kusto", "arguments": "{\"query\":\"AppTraces | take 5\"}"}
        }
    ]);

    let truncated_apim_req = r##"{"model":"gpt-4o","messages":[{"role":"system","content":"# Section A\nSystem rules for APIM routed agent.\n# Section B\nAlways verify metrics."},{"role":"user","content":"Summarize the 32KB truncated payload before it cuts off mid-senten"##;

    let tables_payload = json!({
        "tables": [
            {
                "name": "PrimaryResult",
                "columns": [
                    {"name": "TimeGenerated", "type": "datetime"},
                    {"name": "RecordKind", "type": "string"},
                    {"name": "OperationId", "type": "string"},
                    {"name": "SpanId", "type": "string"},
                    {"name": "ParentId", "type": "string"},
                    {"name": "Name", "type": "string"},
                    {"name": "Target", "type": "string"},
                    {"name": "ResultCode", "type": "string"},
                    {"name": "DurationMs", "type": "real"},
                    {"name": "Message", "type": "string"},
                    {"name": "Properties", "type": "dynamic"},
                    {"name": "ItemCount", "type": "int"}
                ],
                "rows": [
                    [
                        "2026-11-06T01:00:00.000Z", "dependency", "op-1", "span-1", "",
                        "chat gpt-4o", "https://my-aoai.openai.azure.com/openai/deployments/gpt-4o/chat/completions",
                        "200", 420.0, "",
                        {
                            "gen_ai.system": "az.ai.openai",
                            "gen_ai.request.model": "gpt-4o",
                            "gen_ai.usage.input_tokens": 310,
                            "gen_ai.usage.output_tokens": 45,
                            "gen_ai.usage.cached_tokens": 128,
                            "gen_ai.request.tools": tools_json
                        },
                        1
                    ],
                    [
                        "2026-11-06T01:00:00.010Z", "trace", "op-1", "", "span-1",
                        "", "", "", 0.0, "gen_ai.system.message",
                        {"event.name": "gen_ai.system.message", "content": "# Role\nYou are an Azure diagnostics agent.\n# Rules\nBe concise and accurate."},
                        1
                    ],
                    [
                        "2026-11-06T01:00:00.020Z", "trace", "op-1", "", "span-1",
                        "", "", "", 0.0, "gen_ai.user.message",
                        {"event.name": "gen_ai.user.message", "content": "Check recent errors in AppTraces"},
                        1
                    ],
                    [
                        "2026-11-06T01:00:00.030Z", "trace", "op-1", "", "span-1",
                        "", "", "", 0.0, "gen_ai.assistant.message",
                        {"event.name": "gen_ai.assistant.message", "content": "", "tool_calls": tool_calls_json},
                        1
                    ],
                    [
                        "2026-11-06T01:00:00.040Z", "trace", "op-1", "", "span-1",
                        "", "", "", 0.0, "gen_ai.tool.message",
                        {"event.name": "gen_ai.tool.message", "name": "query_kusto", "id": "call_kql_1", "content": "{\"rows\":[{\"err\":\"timeout\"}]}"},
                        1
                    ],
                    [
                        "2026-11-06T01:00:00.050Z", "trace", "op-1", "", "span-1",
                        "", "", "", 0.0, "gen_ai.user.message",
                        {"event.name": "gen_ai.user.message", "content": "What caused the timeout?"},
                        1
                    ],
                    [
                        "2026-11-06T01:00:00.060Z", "trace", "op-1", "", "span-1",
                        "", "", "", 0.0, "gen_ai.choice",
                        {"event.name": "gen_ai.choice", "message": {"role": "assistant", "content": "The timeout occurred in downstream DB."}},
                        1
                    ],
                    [
                        "2026-11-06T01:00:02.000Z", "dependency", "op-1", "span-2", "",
                        "POST /chat/completions", "https://apim.azure-api.net/openai/deployments/gpt-4o/chat/completions",
                        "200", 380.0, "",
                        {
                            "gen_ai.system": "openai",
                            "gen_ai.request.model": "gpt-4o",
                            "gen_ai.usage.input_tokens": 120,
                            "gen_ai.usage.output_tokens": 20,
                            "request_body": truncated_apim_req
                        },
                        1
                    ],
                    [
                        "2026-11-06T01:00:10.000Z", "dependency", "op-2", "span-3", "",
                        "chat gpt-4o-mini", "https://my-aoai.openai.azure.com/openai/deployments/gpt-4o-mini/chat/completions",
                        "200", 190.0, "",
                        {
                            "gen_ai.system": "az.ai.openai",
                            "gen_ai.request.model": "gpt-4o-mini",
                            "gen_ai.usage.input_tokens": 540,
                            "gen_ai.usage.output_tokens": 60
                        },
                        4
                    ]
                ]
            }
        ]
    });

    let opts = IngestOptions::default();
    let (records, st) =
        parse_app_insights_payload(&tables_payload.to_string(), &opts).expect("parse app insights");

    assert_eq!(records.len(), 3);
    assert_eq!(st.tier1_full_calls, 2);
    assert_eq!(st.tier2_usage_only_calls, 1);
    assert_eq!(st.truncated_repaired_calls, 1);
    assert_eq!(st.sampled_calls, 1);
    assert_eq!(st.max_item_count, 4);

    // Verify Call 1 has all expected segment kinds decomposed
    let c1_kinds: HashSet<&str> = records[0]
        .segments
        .iter()
        .map(|s| s.kind.as_str())
        .collect();
    assert!(c1_kinds.contains("system"));
    assert!(c1_kinds.contains("history_user"));
    assert!(c1_kinds.contains("tool_def"));
    assert!(c1_kinds.contains("tool_call"));
    assert!(c1_kinds.contains("tool_result"));
    assert!(c1_kinds.contains("user_prompt"));
    assert_eq!(
        records[0].response_text.as_deref(),
        Some("The timeout occurred in downstream DB.")
    );

    // Verify Call 3 is usage_only fallback
    assert_eq!(records[2].quality.as_deref(), Some("usage_only"));
    assert_eq!(
        records[2].usage.get("input_total").and_then(|v| v.as_i64()),
        Some(540)
    );

    // Verify summarize() findings include content-recording, sampling, and truncation notices
    let summary = summarize(&records, 5.0);
    let finding_texts: Vec<&str> = summary.findings.iter().map(|f| f.text.as_str()).collect();
    assert!(
        finding_texts
            .iter()
            .any(|t| t.contains("AZURE_TRACING_GEN_AI_CONTENT_RECORDING_ENABLED=true")),
        "expected content recording finding, got: {finding_texts:?}"
    );
    assert!(
        finding_texts
            .iter()
            .any(|t| t.contains("adaptive sampling was detected")),
        "expected sampling finding, got: {finding_texts:?}"
    );
    assert!(
        finding_texts
            .iter()
            .any(|t| t.contains("32 KB log property limit")),
        "expected truncation repair finding, got: {finding_texts:?}"
    );
}

#[test]
fn test_lossless_chunk_bridge_and_live_azure_pull() {
    // 1. Start a mock LLM server and run Python with TOKPROF_STDOUT_LOG=1 + TOKPROF_STDOUT_FILE
    //    sending a 50KB prompt that exceeds App Insights' 32KB single-property limit.
    let mock_llm = TcpListener::bind("127.0.0.1:0").expect("bind mock llm");
    let llm_port = mock_llm.local_addr().unwrap().port();
    thread::spawn(move || {
        if let Ok((mut s, _)) = mock_llm.accept() {
            drain_http_request(&mut s);
            let body = r#"{"id":"cmpl-big","model":"gpt-4o","choices":[{"message":{"role":"assistant","content":"lossless chunk response ok"}}],"usage":{"prompt_tokens":6400,"completion_tokens":12}}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });

    let tmp = std::env::temp_dir().join(format!("tokprof-az-pull-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let boot_dir = tokprof::runner::materialize_boot_dir();
    let chunk_file = tmp.join("container_stdout.log");
    let chunk_file_str = chunk_file.to_string_lossy().into_owned();

    // Create a >45KB request payload (with incompressible random-looking hex blocks so gzip base64 > 20KB -> multiple chunks)
    let py_code = format!(
        "import urllib.request, json, hashlib; \
         big_sys = '# System Header\\n' + '\\n'.join(hashlib.sha256(str(i).encode()).hexdigest() for i in range(900)); \
         req_data = json.dumps({{'model':'gpt-4o','messages':[{{'role':'system','content':big_sys}},{{'role':'user','content':'verify lossless 50KB chunk bridge'}}]}}).encode(); \
         req = urllib.request.Request('http://127.0.0.1:{llm_port}/v1/chat/completions', data=req_data, headers={{'Content-Type':'application/json'}}, method='POST'); \
         urllib.request.urlopen(req).read()"
    );
    let py_bin = if cfg!(windows) { "python" } else { "python3" };
    let status = std::process::Command::new(py_bin)
        .args(["-c", &py_code])
        .env("PYTHONPATH", &boot_dir)
        .env("TOKPROF_STDOUT_LOG", "1")
        .env("TOKPROF_STDOUT_FILE", &chunk_file_str)
        .status()
        .expect("run python with stdout chunk bridge");
    assert!(status.success());

    let raw_chunks = fs::read_to_string(&chunk_file).expect("read chunk file");
    let chunk_lines: Vec<&str> = raw_chunks
        .lines()
        .filter(|l| l.starts_with("[TOKPROF_CHUNK]"))
        .collect();
    assert!(
        chunk_lines.len() >= 2,
        "expected multi-chunk emission for >45KB payload, got {} chunk(s)",
        chunk_lines.len()
    );

    // 2. Serve those [TOKPROF_CHUNK] lines inside a mock Azure Log Analytics /v1/workspaces/ws-123/query HTTP server
    let mock_la = TcpListener::bind("127.0.0.1:0").expect("bind mock log analytics");
    let la_port = mock_la.local_addr().unwrap().port();
    let rows_vec: Vec<Value> = chunk_lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            json!([
                format!("2026-11-06T02:00:0{idx}.000Z"),
                "trace",
                "op-chunk-1",
                "",
                "span-chunk-1",
                "",
                "",
                "",
                0.0,
                *line,
                {},
                1
            ])
        })
        .collect();
    let la_response = json!({
        "tables": [
            {
                "name": "PrimaryResult",
                "columns": [
                    {"name": "TimeGenerated", "type": "datetime"},
                    {"name": "RecordKind", "type": "string"},
                    {"name": "OperationId", "type": "string"},
                    {"name": "SpanId", "type": "string"},
                    {"name": "ParentId", "type": "string"},
                    {"name": "Name", "type": "string"},
                    {"name": "Target", "type": "string"},
                    {"name": "ResultCode", "type": "string"},
                    {"name": "DurationMs", "type": "real"},
                    {"name": "Message", "type": "string"},
                    {"name": "Properties", "type": "dynamic"},
                    {"name": "ItemCount", "type": "int"}
                ],
                "rows": rows_vec
            }
        ]
    })
    .to_string();

    thread::spawn(move || {
        if let Ok((mut s, _)) = mock_la.accept() {
            drain_http_request(&mut s);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                la_response.len(),
                la_response
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });

    // 3. Invoke `tokprof pull` end-to-end against the mock Log Analytics server
    let pulled_jsonl = tmp.join("pulled.jsonl");
    let pulled_str = pulled_jsonl.to_string_lossy().into_owned();
    std::env::set_var("TOKPROF_NO_SERVER", "1");
    std::env::set_var("TOKPROF_NO_BROWSER", "1");
    let pull_args = vec![
        "pull".to_string(),
        "--workspace".to_string(),
        "ws-123".to_string(),
        "--endpoint".to_string(),
        format!("http://127.0.0.1:{la_port}"),
        "--token".to_string(),
        "mock-azure-bearer-token".to_string(),
        "--out".to_string(),
        pulled_str.clone(),
        "--no-report".to_string(),
    ];
    let rc = run_cli(&pull_args);
    assert_eq!(rc, 0);

    let pulled_recs = read_jsonl(&pulled_str, None);
    assert_eq!(pulled_recs.len(), 1);
    assert_eq!(pulled_recs[0].model.as_deref(), Some("gpt-4o"));
    assert_eq!(
        pulled_recs[0].response_text.as_deref(),
        Some("lossless chunk response ok")
    );
    let user_seg = pulled_recs[0]
        .segments
        .iter()
        .find(|s| s.kind == "user_prompt")
        .expect("user_prompt segment");
    assert_eq!(
        user_seg.text.as_deref(),
        Some("verify lossless 50KB chunk bridge")
    );
    let sys_chars: usize = pulled_recs[0]
        .segments
        .iter()
        .filter(|s| s.kind == "system")
        .map(|s| s.chars)
        .sum();
    assert!(
        sys_chars > 50_000,
        "expected lossless >50KB system prompt chars, got {sys_chars}"
    );

    let _ = fs::remove_dir_all(&tmp);
}

fn read_http_request_text(stream: &mut TcpStream) -> String {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let mut header_end = None;
    let mut content_len = 0usize;
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if header_end.is_none() {
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = Some(pos + 4);
                        let head = String::from_utf8_lossy(&buf[..pos]);
                        for line in head.lines() {
                            if let Some((k, v)) = line.split_once(':') {
                                if k.trim().eq_ignore_ascii_case("content-length") {
                                    content_len = v.trim().parse::<usize>().unwrap_or(0);
                                }
                            }
                        }
                    }
                }
                if let Some(he) = header_end {
                    if buf.len() >= he + content_len {
                        break;
                    }
                }
            }
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

#[test]
fn test_rbac_fallback_law_latency_wait_resume_and_dedup() {
    // 1. Verify RBAC / Tenant / NotFound error diagnostics and Sovereign Cloud endpoints
    let err_401 = classify_azure_error(401, "InvalidAuthenticationTokenTenant: wrong tenant");
    assert!(err_401.contains("az account set --subscription"));
    let err_403 = classify_azure_error(403, "InsufficientAccessError: access denied");
    assert!(err_403.contains("Log Analytics Reader"));
    assert!(err_403.contains("--resource-id /subscriptions/"));
    let err_404 = classify_azure_error(404, "WorkspaceNotFound");
    assert!(err_404.contains("--cloud usgov|china"));

    let (gov_la, gov_ai) = resolve_azure_cloud_endpoints(Some("usgov"));
    assert_eq!(gov_la, "https://api.loganalytics.us");
    assert_eq!(gov_ai, "https://api.applicationinsights.us");

    // 2. Test Table-level RBAC automatic fallback (403 on AppTraces -> 200 on AppDependencies-only)
    //    AND Resource-centric ARM Resource ID routing (/v1/subscriptions/.../query)
    let mock_rbac = TcpListener::bind("127.0.0.1:0").expect("bind mock rbac server");
    let rbac_port = mock_rbac.local_addr().unwrap().port();
    thread::spawn(move || {
        for _ in 0..2 {
            if let Ok((mut s, _)) = mock_rbac.accept() {
                let req_txt = read_http_request_text(&mut s);
                assert!(
                    req_txt.starts_with(
                        "POST /v1/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Insights/components/app-1/query"
                    ),
                    "expected ARM resource-id query path, got: {req_txt}"
                );
                if req_txt.contains("AppTraces") {
                    let body = r#"{"error":{"code":"InsufficientAccessError","message":"The provided credentials have insufficient access to perform the requested operation on table AppTraces"}}"#;
                    let resp = format!(
                        "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = s.write_all(resp.as_bytes());
                } else {
                    let body = json!({
                        "tables": [{
                            "name": "PrimaryResult",
                            "columns": [
                                {"name": "TimeGenerated", "type": "datetime"},
                                {"name": "IngestionTime", "type": "string"},
                                {"name": "RecordKind", "type": "string"},
                                {"name": "OperationId", "type": "string"},
                                {"name": "SpanId", "type": "string"},
                                {"name": "ParentId", "type": "string"},
                                {"name": "Name", "type": "string"},
                                {"name": "Target", "type": "string"},
                                {"name": "ResultCode", "type": "string"},
                                {"name": "DurationMs", "type": "real"},
                                {"name": "Message", "type": "string"},
                                {"name": "Properties", "type": "dynamic"},
                                {"name": "ItemCount", "type": "int"}
                            ],
                            "rows": [[
                                "2026-11-06T03:00:00.000Z", "2026-11-06T03:02:30.000Z",
                                "dependency", "op-rbac-1", "span-rbac-1", "",
                                "chat gpt-4o", "https://aoai.openai.azure.com/openai/deployments/gpt-4o/chat/completions",
                                "200", 250.0, "",
                                {"gen_ai.system": "az.ai.openai", "gen_ai.request.model": "gpt-4o", "gen_ai.usage.input_tokens": 300, "gen_ai.usage.output_tokens": 40},
                                1
                            ]]
                        }]
                    })
                    .to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = s.write_all(resp.as_bytes());
                }
                let _ = s.flush();
            }
        }
    });

    let tmp = std::env::temp_dir().join(format!("tokprof-resilience-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let clouddrive_dir = tmp.join("clouddrive");
    fs::create_dir_all(&clouddrive_dir).unwrap();
    std::env::set_var(
        "TOKPROF_CLOUDDRIVE_DIR",
        clouddrive_dir.to_string_lossy().as_ref(),
    );
    std::env::set_var("TOKPROF_NO_SERVER", "1");
    std::env::set_var("TOKPROF_NO_BROWSER", "1");

    let rbac_out = tmp.join("rbac.jsonl").to_string_lossy().into_owned();
    let rc_rbac = run_cli(&[
        "pull".to_string(),
        "--resource-id".to_string(),
        "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Insights/components/app-1"
            .to_string(),
        "--endpoint".to_string(),
        format!("http://127.0.0.1:{rbac_port}"),
        "--token".to_string(),
        "mock-token".to_string(),
        "--out".to_string(),
        rbac_out.clone(),
        "--no-report".to_string(),
    ]);
    assert_eq!(rc_rbac, 0);
    let rbac_recs = read_jsonl(&rbac_out, None);
    assert_eq!(rbac_recs.len(), 1);
    assert_eq!(rbac_recs[0].quality.as_deref(), Some("usage_only"));

    // 3. Test LAW Ingestion Latency (--wait-ingest), HTTP 429 retry, Multi-replica deduplication,
    //    Split-horizon span upgrade (usage_only -> tier1_full), Azure SAS/AccountKey redaction, and --resume!
    let mock_lag = TcpListener::bind("127.0.0.1:0").expect("bind mock lag server");
    let lag_port = mock_lag.local_addr().unwrap().port();
    thread::spawn(move || {
        let cols = json!([
            {"name": "TimeGenerated", "type": "datetime"},
            {"name": "IngestionTime", "type": "string"},
            {"name": "RecordKind", "type": "string"},
            {"name": "OperationId", "type": "string"},
            {"name": "SpanId", "type": "string"},
            {"name": "ParentId", "type": "string"},
            {"name": "Name", "type": "string"},
            {"name": "Target", "type": "string"},
            {"name": "ResultCode", "type": "string"},
            {"name": "DurationMs", "type": "real"},
            {"name": "Message", "type": "string"},
            {"name": "Properties", "type": "dynamic"},
            {"name": "ItemCount", "type": "int"}
        ]);
        // Request 1: HTTP 429 Too Many Requests with Retry-After: 0.02
        if let Ok((mut s, _)) = mock_lag.accept() {
            let _ = read_http_request_text(&mut s);
            let body = r#"{"error":{"code":"ThrottledError","message":"Rate limit exceeded"}}"#;
            let resp = format!(
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0.02\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
        // Request 2 (Poll #1 after 429 retry): Only AppDependencies has ingested (3 duplicate rows from multi-replica), AppTraces not yet ingested!
        let dep_row = json!([
            "2026-11-06T04:00:00.000Z", "2026-11-06T04:03:00.000Z",
            "dependency", "op-lag-1", "span-lag-1", "",
            "chat gpt-4o", "https://aoai.openai.azure.com/openai/deployments/gpt-4o/chat/completions",
            "200", 300.0, "",
            {"gen_ai.system": "az.ai.openai", "gen_ai.request.model": "gpt-4o", "gen_ai.usage.input_tokens": 180, "gen_ai.usage.output_tokens": 25},
            1
        ]);
        if let Ok((mut s, _)) = mock_lag.accept() {
            let _ = read_http_request_text(&mut s);
            let body = json!({
                "tables": [{
                    "name": "PrimaryResult",
                    "columns": cols,
                    "rows": [dep_row.clone(), dep_row.clone(), dep_row.clone()]
                }]
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
        // Request 3 (Poll #2): Delayed AppTraces rows have now finished ingesting!
        if let Ok((mut s, _)) = mock_lag.accept() {
            let _ = read_http_request_text(&mut s);
            let body = json!({
                "tables": [{
                    "name": "PrimaryResult",
                    "columns": cols,
                    "rows": [
                        dep_row,
                        [
                            "2026-11-06T04:00:00.010Z", "2026-11-06T04:04:10.000Z",
                            "trace", "op-lag-1", "", "span-lag-1",
                            "", "", "", 0.0, "gen_ai.system.message",
                            {"event.name": "gen_ai.system.message", "content": "You are an Azure storage assistant."},
                            1
                        ],
                        [
                            "2026-11-06T04:00:00.020Z", "2026-11-06T04:04:10.000Z",
                            "trace", "op-lag-1", "", "span-lag-1",
                            "", "", "", 0.0, "gen_ai.user.message",
                            {"event.name": "gen_ai.user.message", "content": "Conn: AccountKey=ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/== and https://blob.core.windows.net/c?sv=2022&sig=abcdefghijklmnopqrstuvwxyz0123456789%2B%2F%3D"},
                            1
                        ],
                        [
                            "2026-11-06T04:00:00.030Z", "2026-11-06T04:04:10.000Z",
                            "trace", "op-lag-1", "", "span-lag-1",
                            "", "", "", 0.0, "gen_ai.choice",
                            {"event.name": "gen_ai.choice", "message": {"role": "assistant", "content": "Late trace arrived and upgraded!"}},
                            1
                        ]
                    ]
                }]
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });

    let lag_out = tmp.join("lag.jsonl").to_string_lossy().into_owned();
    let rc_lag = run_cli(&[
        "pull".to_string(),
        "--workspace".to_string(),
        "ws-lag-1".to_string(),
        "--endpoint".to_string(),
        format!("http://127.0.0.1:{lag_port}"),
        "--token".to_string(),
        "mock-token".to_string(),
        "--wait-ingest".to_string(),
        "5s".to_string(),
        "--poll-interval".to_string(),
        "30ms".to_string(),
        "--out".to_string(),
        lag_out.clone(),
        "--no-report".to_string(),
    ]);
    assert_eq!(rc_lag, 0);

    let lag_recs = read_jsonl(&lag_out, None);
    // Must be deduplicated to 1 call and upgraded in-place from usage_only to full segments!
    assert_eq!(lag_recs.len(), 1);
    assert_ne!(lag_recs[0].quality.as_deref(), Some("usage_only"));
    assert_eq!(lag_recs[0].segments.len(), 2);
    assert_eq!(
        lag_recs[0].response_text.as_deref(),
        Some("Late trace arrived and upgraded!")
    );
    // Verify Azure AccountKey and SAS sig were redacted
    let user_txt = lag_recs[0]
        .segments
        .iter()
        .find(|s| s.kind == "user_prompt")
        .and_then(|s| s.text.as_deref())
        .unwrap_or("");
    assert!(
        user_txt.contains("***REDACTED***") && !user_txt.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
        "expected Azure secrets to be redacted, got: {user_txt}"
    );
    // Verify ingestion lag (250s = 250,000ms) was recorded and triggers a finding
    let lag_summary = summarize(&lag_recs, 5.0);
    assert!(
        lag_summary
            .findings
            .iter()
            .any(|f| f.text.contains("Log Analytics ingestion lag reached")),
        "expected ingestion lag finding, got: {:?}",
        lag_summary.findings
    );

    // Verify Cloud Shell checkpoint file was saved in clouddrive/.tokprof/pull-checkpoint.json
    let cp_path = clouddrive_dir.join(".tokprof").join("pull-checkpoint.json");
    assert!(cp_path.is_file(), "expected checkpoint at {cp_path:?}");

    // 4. Test Azure Portal CSV export ingestion with Foundry `gen_ai.request.instructions` and `gen_ai.tool.call.result`
    let csv_export = "\u{feff}TimeGenerated [UTC],RecordKind,OperationId,SpanId,ParentId,Name,Target,ResultCode,DurationMs,Message,Properties,ItemCount\r\n\
2026-11-06T05:00:00.000Z,dependency,op-csv-1,sp-csv-1,,responses gpt-4.1,https://foundry.services.ai.azure.com/models/chat/completions,200,310,,\"{\"\"gen_ai.system\"\":\"\"az.ai.inference\"\",\"\"gen_ai.request.model\"\":\"\"gpt-4.1\"\",\"\"gen_ai.request.instructions\"\":\"\"You are a Foundry agent.\"\",\"\"gen_ai.usage.input_tokens\"\":220,\"\"gen_ai.usage.output_tokens\"\":35,\"\"gen_ai.usage.cache_creation_input_tokens\"\":128}\",1\r\n\
2026-11-06T05:00:00.010Z,trace,op-csv-1,,sp-csv-1,,,,0,gen_ai.tool.message,\"{\"\"event.name\"\":\"\"gen_ai.tool.message\"\",\"\"gen_ai.tool.call.name\"\":\"\"fetch_metrics\"\",\"\"gen_ai.tool.call.id\"\":\"\"tc_9\"\",\"\"gen_ai.tool.call.result\"\":\"\"{\\\"\"cpu\\\"\":42}\"\"}\",1\r\n\
2026-11-06T05:00:00.020Z,trace,op-csv-1,,sp-csv-1,,,,0,gen_ai.user.message,\"{\"\"event.name\"\":\"\"gen_ai.user.message\"\",\"\"content\"\":\"\"Summarize CPU usage\"\"}\",1\r\n\
2026-11-06T05:00:00.030Z,trace,op-csv-1,,sp-csv-1,,,,0,gen_ai.choice,\"{\"\"event.name\"\":\"\"gen_ai.choice\"\",\"\"content\"\":\"\"CPU is at 42%.\"\"}\",1\r\n";

    let (csv_recs, csv_st) =
        parse_app_insights_payload(csv_export, &IngestOptions::default()).expect("parse csv export");
    assert_eq!(csv_recs.len(), 1);
    assert_eq!(csv_st.tier1_full_calls, 1);
    assert_eq!(csv_recs[0].model.as_deref(), Some("gpt-4.1"));
    assert_eq!(
        csv_recs[0].usage.get("cache_write").and_then(|v| v.as_i64()),
        Some(128)
    );
    let csv_kinds: HashSet<&str> = csv_recs[0]
        .segments
        .iter()
        .map(|s| s.kind.as_str())
        .collect();
    assert!(csv_kinds.contains("system"));
    assert!(csv_kinds.contains("tool_result"));
    assert!(csv_kinds.contains("user_prompt"));

    // 5. Verify prompt inclusion vs `--no-content` vs `--usage-only` on the same dataset
    let ci_full = inspect_records_content(&csv_recs);
    assert_eq!(ci_full.mode, "full-prompts");
    assert!(ci_full.segments_with_text > 0);
    assert!(ci_full.stored_prompt_chars > 0);
    assert!(ci_full.stored_response_chars > 0);

    let (no_content_recs, _) = parse_app_insights_payload(
        csv_export,
        &IngestOptions {
            include_text: Some(false),
            ..IngestOptions::default()
        },
    )
    .expect("parse no-content");
    let ci_nc = inspect_records_content(&no_content_recs);
    assert_eq!(ci_nc.mode, "no-content");
    assert_eq!(ci_nc.total_segments, ci_full.total_segments);
    assert_eq!(ci_nc.segments_with_text, 0);
    assert_eq!(ci_nc.stored_prompt_chars, 0);
    assert_eq!(ci_nc.stored_response_chars, 0);
    assert_eq!(no_content_recs[0].est_input, csv_recs[0].est_input);
    assert_eq!(no_content_recs[0].sysfp, csv_recs[0].sysfp);

    let (usage_only_recs, _) = parse_app_insights_payload(
        csv_export,
        &IngestOptions {
            usage_only: true,
            ..IngestOptions::default()
        },
    )
    .expect("parse usage-only");
    let ci_uo = inspect_records_content(&usage_only_recs);
    assert_eq!(ci_uo.mode, "usage-only");
    assert_eq!(ci_uo.total_segments, 0);
    assert_eq!(
        usage_only_recs[0]
            .usage
            .get("input_total")
            .and_then(|v| v.as_i64()),
        Some(220)
    );

    std::env::remove_var("TOKPROF_CLOUDDRIVE_DIR");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn test_20_rounds_enhancements_end_to_end() {
    use tokprof::analyze::{builtin_prices, compare, doc_link, estimate_cost, summarize, to_legacy};
    use tokprof::counting::{fast_hash_pair, get_counter};
    use tokprof::detect::parse_url_parts;
    use tokprof::parse_request::{
        ANTHROPIC, GEMINI, OPENAI_CHAT, OPENAI_RESPONSES, parse_request,
    };
    use tokprof::parse_response::{SSEParser, parse_response_body};
    use tokprof::report::csv_quote;
    use tokprof::segment::{BuildRecordArgs, build_record, hash_text, redact_secrets, split_sections};

    // 1. Full-byte hashing on 24 KB system prompt with interior UUID change + mutating section finding
    let prefix = format!("# Role\n{}", "Follow safety rules carefully. ".repeat(350));
    let rt_pad = "Environment: linux x86_64 production cluster. ".repeat(12);
    let suffix = format!("# Output\n{}", "Respond concisely in JSON. ".repeat(350));
    let sys1 = format!("{prefix}\n# Runtime\n{rt_pad}req_id=11111111-1111-1111-1111-111111111111\n{suffix}");
    let sys2 = format!("{prefix}\n# Runtime\n{rt_pad}req_id=22222222-2222-2222-2222-222222222222\n{suffix}");
    let sys3 = format!("{prefix}\n# Runtime\n{rt_pad}req_id=33333333-3333-3333-3333-333333333333\n{suffix}");
    assert_eq!(sys1.len(), sys2.len());
    assert!(sys1.len() > 20_000);
    assert_ne!(fast_hash_pair(sys1.as_bytes()), fast_hash_pair(sys2.as_bytes()));
    assert_ne!(hash_text(&sys1, b"salt"), hash_text(&sys2, b"salt"));

    let tk = get_counter(Some("gpt-4o"), true);
    let mut cache_bust_recs = Vec::new();
    for (idx, sp) in [&sys1, &sys2, &sys3].into_iter().enumerate() {
        let pr = parse_request(
            OPENAI_CHAT,
            &json!({"model": "gpt-4o-2024-11-20", "messages": [{"role": "system", "content": sp}, {"role": "user", "content": "hi"}]}),
        )
        .unwrap();
        let resp = parse_response_body(
            OPENAI_CHAT,
            r#"{"choices":[{"message":{"role":"assistant","content":"ok"}}],"usage":{"prompt_tokens":4600,"completion_tokens":800,"completion_tokens_details":{"reasoning_tokens":700}}}"#,
            false,
        );
        cache_bust_recs.push(build_record(BuildRecordArgs {
            parsed: &pr,
            response: Some(&resp),
            counter: &tk,
            salt: b"s1",
            components: None,
            endpoint: "/v1/chat/completions",
            status: Some(200),
            latency_ms: Some(120),
            ttfb_ms: Some(40),
            run: Some("r1".to_string()),
            error: None,
            ts: Some(1700001000.0 + (idx as f64) * 10.0),
            include_text: Some(true),
        }));
    }
    let sum = summarize(&cache_bust_recs, 5.0);
    assert_eq!(sum.reasoning_total, 2100);
    assert!(
        sum.findings
            .iter()
            .any(|f| f.text.contains("mutating section: Runtime")),
        "expected mutating section Runtime in findings: {:?}",
        sum.findings
    );
    assert!(
        sum.findings
            .iter()
            .any(|f| f.text.contains("Reasoning tokens consumed")),
        "expected high reasoning token share finding: {:?}",
        sum.findings
    );

    // 2. Built-in pricing with version suffix matching (`gpt-4o-2024-11-20`) & `cache_write`
    let bprices = builtin_prices();
    let cost = estimate_cost(&cache_bust_recs, &bprices);
    assert!(cost.total > 0.0 && cost.models_without_price.is_empty());

    // 3. Fenced code block inside system prompt must NOT split on `# comment` inside ```bash
    let sys_with_fence = format!(
        "# Overview\n{}\n```bash\n# Should Not Be A Heading\necho hi\n```\n# Rules\n{}",
        "Intro text here. ".repeat(20),
        "Rule text here. ".repeat(20)
    );
    let secs = split_sections(&sys_with_fence, 200);
    let sec_names: Vec<Option<&str>> = secs.iter().map(|(n, _)| n.as_deref()).collect();
    assert_eq!(sec_names, vec![Some("Overview"), Some("Rules")]);

    // 4. `tool_call_id` / `call_id` / `tool_use_id` -> real tool name resolution + `<system-reminder>` + PNG vision tokens
    // 1x1 PNG base64
    let png_1x1 = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    let chat_pr = parse_request(
        OPENAI_CHAT,
        &json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "user", "content": "<system-reminder>Secret framework reminder</system-reminder>\nRun search"},
                {"role": "assistant", "content": null, "tool_calls": [{"id": "call_xyz", "type": "function", "function": {"name": "ripgrep_search", "arguments": "{\"q\":\"foo\"}"}}]},
                {"role": "tool", "tool_call_id": "call_xyz", "content": "match1\nmatch2"},
                {"role": "user", "content": [{"type": "text", "text": "check image"}, {"type": "image_url", "image_url": {"url": png_1x1}}]}
            ]
        }),
    )
    .unwrap();
    assert!(chat_pr.segments.iter().any(|s| s.kind == "system" && s.name.as_deref() == Some("system-reminder")));
    assert!(chat_pr.segments.iter().any(|s| s.kind == "tool_result" && s.name.as_deref() == Some("ripgrep_search")));
    assert_eq!(
        chat_pr
            .segments
            .iter()
            .find(|s| s.kind == "attachment")
            .and_then(|s| s.est_tokens),
        Some(255) // 85 + 170 * 1 * 1 tile
    );

    // Anthropic server_tool_use + tool_use_id resolution + Bedrock Converse cached usage
    let anth_pr = parse_request(
        ANTHROPIC,
        &json!({
            "model": "claude-sonnet-4-20250514",
            "messages": [
                {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_77", "name": "bash_exec", "input": {"cmd": "ls"}}]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_77", "content": "Cargo.toml"}]}
            ]
        }),
    )
    .unwrap();
    assert!(anth_pr.segments.iter().any(|s| s.kind == "tool_result" && s.name.as_deref() == Some("bash_exec")));

    let bedrock_resp = parse_response_body(
        OPENAI_CHAT,
        r#"{"output":{"message":{"role":"assistant","content":[{"text":"done"}]}},"usage":{"inputTokens":100,"outputTokens":25,"cacheReadInputTokens":1500,"cacheWriteInputTokens":200,"totalTokens":1825}}"#,
        false,
    );
    assert_eq!(bedrock_resp.usage.input_total, Some(1800));
    assert_eq!(bedrock_resp.usage.cached, Some(1500));
    assert_eq!(bedrock_resp.usage.cache_write, Some(200));

    // OpenAI Responses `previous_response_id` chained context + MCP/web_search + Gemini 2.5 toolUsePromptTokenCount
    let resp_api_pr = parse_request(
        OPENAI_RESPONSES,
        &json!({"model": "gpt-5", "previous_response_id": "resp_prev_1", "input": [{"type": "web_search_call", "id": "ws_1"}, {"role": "user", "content": "next"}]}),
    )
    .unwrap();
    assert!(resp_api_pr.chained_context);
    assert!(resp_api_pr.segments.iter().any(|s| s.kind == "tool_call" && s.name.as_deref() == Some("web_search_call")));

    let gem_resp = parse_response_body(
        GEMINI,
        r#"{"candidates":[{"content":{"parts":[{"text":"hi"}]}}],"usageMetadata":{"promptTokenCount":100,"toolUsePromptTokenCount":45,"cachedContentTokenCount":40,"candidatesTokenCount":10,"thoughtsTokenCount":30}}"#,
        false,
    );
    assert_eq!(gem_resp.usage.input_total, Some(145));
    assert_eq!(gem_resp.usage.reasoning, Some(30));

    // 5. SSE multi-line data + mid-stream error + empty tool_call delta name filter
    let mut sp = SSEParser::new(OPENAI_CHAT);
    sp.feed_text(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"my_tool\"}}]}}]}\n\n\
         data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"\",\"arguments\":\"{}\"}}]}}]}\n\n\
         data: {\"error\":\n\
         data: {\"message\":\"rate_limited\"}}\n\n",
    );
    let sp_res = sp.finish();
    assert_eq!(sp_res.tool_calls, vec!["my_tool".to_string()]);
    assert_eq!(sp_res.error.as_deref(), Some("rate_limited"));

    // 6. CSV formula injection guard, URL query delimiter, secret/PEM/DB URI redaction, `.jsonl.gz`, Bedrock & Langfuse import, paired compare, HTML DOM XSS guard
    assert_eq!(csv_quote("=CMD|' /C calc'!A0"), "\"'=CMD|' /C calc'!A0\"");
    assert_eq!(csv_quote("-42"), "-42");
    assert_eq!(parse_url_parts("https://imds.local:8080?api-version=1").1, "imds.local");

    let pem_and_db = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE KEY-----\n url=postgres://admin:supersecret@db.internal:5432/prod hf=hf_abcdefghijklmnopqrstuvwxyz1234";
    let red = redact_secrets(pem_and_db);
    assert!(!red.contains("supersecret") && !red.contains("MIIEpAIBAAKCAQEA") && !red.contains("hf_abcdef"));

    let tmp = std::env::temp_dir().join(format!("tokprof-r20-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();

    // `.jsonl.gz` write + read + Bedrock & Langfuse import + `tokprof clean`
    let bedrock_and_langfuse = concat!(
        "{\"schemaType\":\"ModelInvocationLog\",\"modelId\":\"anthropic.claude-3-5-sonnet\",\"input\":{\"inputBodyJson\":{\"messages\":[{\"role\":\"user\",\"content\":\"hello bedrock\"}]},\"inputTokenCount\":40,\"cacheReadInputTokenCount\":10},\"output\":{\"outputBodyJson\":{\"output\":{\"message\":{\"role\":\"assistant\",\"content\":[{\"text\":\"hi bedrock\"}]}}},\"outputTokenCount\":6}}\n",
        "{\"type\":\"GENERATION\",\"model\":\"gpt-4o-mini\",\"input\":[{\"role\":\"system\",\"content\":\"be terse\"},{\"role\":\"user\",\"content\":\"hello langfuse\"}],\"output\":\"hi langfuse\",\"usage\":{\"input\":35,\"output\":5}}\n"
    );
    let raw_log_gz = tmp.join("ext.jsonl.gz");
    {
        use flate2::Compression;
        use flate2::write::GzEncoder;
        let f = fs::File::create(&raw_log_gz).unwrap();
        let mut enc = GzEncoder::new(f, Compression::fast());
        enc.write_all(bedrock_and_langfuse.as_bytes()).unwrap();
        enc.finish().unwrap();
    }
    let gz_out = tmp.join("imported.jsonl.gz").to_string_lossy().into_owned();
    let rc_imp = run_cli(&[
        "import".to_string(),
        raw_log_gz.to_string_lossy().into_owned(),
        "--out".to_string(),
        gz_out.clone(),
        "--no-report".to_string(),
    ]);
    assert_eq!(rc_imp, 0);
    let imp_recs = read_jsonl(&gz_out, None);
    assert_eq!(imp_recs.len(), 2);
    assert_eq!(imp_recs[0].usage.get("input_total").and_then(|v| v.as_i64()), Some(50));

    // Paired compare + legacy export + Korean doc_link + HTML XSS escaping
    let cmp_val = compare(&cache_bust_recs, &imp_recs, 5.0);
    assert!(cmp_val.get("quality_guardrail").is_some());
    assert!(cmp_val.get("cost_usd_est").is_some());
    let legacy_rows = to_legacy(&cache_bust_recs, 5.0);
    assert_eq!(legacy_rows[0].get("reasoning_tokens").and_then(|v| v.as_i64()), Some(700));

    std::env::set_var("TOKPROF_LANG", "ko");
    assert!(doc_link("improve").contains("/ko/agent-tokenomics/04-improve/"));
    std::env::remove_var("TOKPROF_LANG");

    let mut xss_rec = cache_bust_recs[0].clone();
    xss_rec.model = Some("<img src=x onerror=alert(1)>".to_string());
    let html = render_html(&[xss_rec], 5.0, Some(&bprices), "XSS Check", None);
    assert!(html.contains("function escHtml("));
    assert!(!html.contains("<img src=x onerror=alert(1)>"));

    // Node.js hook test with `fetch(new Request(...))` + immediate `process.exit(0)` if `node` is installed
    if tokprof::ebpf::which_exe("node").is_some() {
        let mock_node_llm = TcpListener::bind("127.0.0.1:0").unwrap();
        let node_llm_port = mock_node_llm.local_addr().unwrap().port();
        thread::spawn(move || {
            if let Ok((mut s, _)) = mock_node_llm.accept() {
                drain_http_request(&mut s);
                let b = r#"{"id":"1","model":"gpt-4o-mini","choices":[{"message":{"role":"assistant","content":"node ok"}}],"usage":{"prompt_tokens":14,"completion_tokens":2}}"#;
                let r = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    b.len(),
                    b
                );
                let _ = s.write_all(r.as_bytes());
                let _ = s.flush();
            }
        });
        let node_out = tmp.join("node_calls.jsonl").to_string_lossy().into_owned();
        let js_code = format!(
            "const req = new Request('http://127.0.0.1:{node_llm_port}/v1/chat/completions', {{method:'POST', headers:{{'content-type':'application/json'}}, body: JSON.stringify({{model:'gpt-4o-mini', messages:[{{role:'user', content:'from Request obj'}}]}})}}); fetch(req).then(r=>r.text()).then(()=>setTimeout(()=>process.exit(0), 60));"
        );
        let rc_node = run_profiled(
            &["node".to_string(), "-e".to_string(), js_code],
            &node_out,
            HashMap::new(),
            &[],
            false,
            true,
            false,
            true,
        );
        assert_eq!(rc_node, 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut node_recs = Vec::new();
        while std::time::Instant::now() < deadline {
            node_recs = read_jsonl(&node_out, None);
            if !node_recs.is_empty() {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(30));
        }
        assert_eq!(node_recs.len(), 1, "expected Node Request+process.exit call to be captured");
    }

    // Test `tokprof clean --keep 1`
    for idx in 0..3 {
        fs::write(tmp.join(format!("old-{idx}.jsonl")), "{}\n").unwrap();
    }
    let rc_clean = run_cli(&[
        "clean".to_string(),
        tmp.to_string_lossy().into_owned(),
        "--keep".to_string(),
        "1".to_string(),
    ]);
    assert_eq!(rc_clean, 0);

    let _ = fs::remove_dir_all(&tmp);
}

