use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use tokprof::analyze::summarize;
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
