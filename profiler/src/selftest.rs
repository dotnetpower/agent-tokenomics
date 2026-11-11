use crate::gateway::{Gateway, GatewayServer, Route};
use crate::parse_response::find_subslice;
use crate::store::{read_jsonl, JsonlWriter};
use rand::RngCore;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

struct MockLlmServer {
    pub url: String,
    pub port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockLlmServer {
    fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();

        let handle = thread::spawn(move || {
            while !stop_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if stop_clone.load(Ordering::SeqCst) {
                            break;
                        }
                        let _ = stream.set_nodelay(true);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                        Self::serve_conn(&mut stream);
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            url: format!("http://127.0.0.1:{port}"),
            port,
            stop,
            handle: Some(handle),
        })
    }

    fn serve_conn(stream: &mut TcpStream) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        let hend: usize;
        loop {
            let Ok(n) = stream.read(&mut chunk) else {
                return;
            };
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                hend = pos;
                break;
            }
            if buf.len() > 65_536 {
                return;
            }
        }
        let head_str = String::from_utf8_lossy(&buf[..hend]).into_owned();
        let mut lines = head_str.split("\r\n");
        let first = lines.next().unwrap_or("");
        let path = first.split_whitespace().nth(1).unwrap_or("/");
        let mut cl = 0usize;
        for ln in lines {
            if let Some((k, v)) = ln.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    cl = v.trim().parse::<usize>().unwrap_or(0);
                }
            }
        }
        while buf.len() < hend + 4 + cl {
            let Ok(n) = stream.read(&mut chunk) else {
                break;
            };
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let body_slice = &buf[(hend + 4)..buf.len().min(hend + 4 + cl)];
        let req_json: Value = serde_json::from_slice(body_slice).unwrap_or(Value::Null);
        let is_stream = req_json
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if path.ends_with("/chat/completions") {
            if is_stream {
                let sse_body = concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"hi \"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"function\":{\"name\":\"get_weather\"}}]}}]}\n\n",
                    "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":120,\"completion_tokens\":8,\"prompt_tokens_details\":{\"cached_tokens\":64}}}\n\n",
                    "data: [DONE]\n\n"
                );
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    sse_body.len(),
                    sse_body
                );
                let _ = stream.write_all(resp.as_bytes());
            } else {
                let resp_json = json!({
                    "id": "chatcmpl-mock",
                    "object": "chat.completion",
                    "choices": [{
                        "message": {
                            "role": "assistant",
                            "content": "Sunny, 22C",
                            "tool_calls": [{"type": "function", "function": {"name": "get_weather", "arguments": "{\"city\":\"Seoul\"}"}}]
                        }
                    }],
                    "usage": {
                        "prompt_tokens": 120,
                        "completion_tokens": 10,
                        "prompt_tokens_details": {"cached_tokens": 64}
                    }
                })
                .to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resp_json.len(),
                    resp_json
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        } else if path.ends_with("/responses") {
            let resp_json = json!({
                "id": "resp-mock",
                "object": "response",
                "output": [{"type": "message", "content": [{"type": "output_text", "text": "hello from responses"}]}],
                "usage": {"input_tokens": 90, "output_tokens": 6, "input_tokens_details": {"cached_tokens": 32}}
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_json.len(),
                resp_json
            );
            let _ = stream.write_all(resp.as_bytes());
        } else if path.ends_with(":generateContent") {
            let resp_json = json!({
                "candidates": [{"content": {"role": "model", "parts": [{"text": "hello from gemini"}]}}],
                "usageMetadata": {"promptTokenCount": 70, "cachedContentTokenCount": 16, "candidatesTokenCount": 4, "thoughtsTokenCount": 12}
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_json.len(),
                resp_json
            );
            let _ = stream.write_all(resp.as_bytes());
        } else if path.ends_with("/messages") {
            let resp_json = json!({
                "id": "msg-mock",
                "type": "message",
                "content": [{"type": "text", "text": "hello from anthropic"}],
                "usage": {"input_tokens": 80, "cache_read_input_tokens": 20, "output_tokens": 5}
            })
            .to_string();
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_json.len(),
                resp_json
            );
            let _ = stream.write_all(resp.as_bytes());
        }
        let _ = stream.flush();
    }

    fn stop(&mut self) {
        if !self.stop.swap(true, Ordering::SeqCst) {
            let _ = TcpStream::connect_timeout(
                &std::net::SocketAddr::from(([127, 0, 0, 1], self.port)),
                Duration::from_millis(50),
            );
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for MockLlmServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn post_http(port: u16, path: &str, body: &Value) -> bool {
    let Ok(addr) = format!("127.0.0.1:{port}").parse() else {
        return false;
    };
    let Ok(mut sock) = TcpStream::connect_timeout(&addr, Duration::from_secs(5)) else {
        return false;
    };
    let _ = sock.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = sock.set_write_timeout(Some(Duration::from_secs(5)));
    let payload = body.to_string();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        payload.len(),
        payload
    );
    if sock.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut out = Vec::new();
    let _ = sock.read_to_end(&mut out);
    !out.is_empty()
}

pub fn run_selftest() -> i32 {
    let mut mock = match MockLlmServer::start() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("tokprof selftest: failed to start mock server: {e}");
            return 1;
        }
    };
    let tmp_dir = std::env::temp_dir().join(format!("tokprof-selftest-{}", std::process::id()));
    let _ = fs::create_dir_all(&tmp_dir);
    let out_file = tmp_dir.join("selftest.jsonl").to_string_lossy().into_owned();

    let routes = vec![
        Route::from_urls("/openai/v1", &format!("{}/v1", mock.url)),
        Route::from_urls("/anthropic", &mock.url),
        Route::from_urls("/gemini", &mock.url),
    ];
    let writer = match JsonlWriter::new(&out_file) {
        Ok(w) => Arc::new(w),
        Err(e) => {
            eprintln!("tokprof selftest: failed to create jsonl writer: {e}");
            return 1;
        }
    };
    let mut salt = vec![0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let w_clone = writer.clone();
    let gw = Gateway::new(
        routes,
        Arc::new(move |rec| w_clone.write_record(&rec)),
        salt,
        HashMap::new(),
        Some("selftest".to_string()),
        HashSet::new(),
        false,
        None,
        None,
        false,
    );
    let mut srv = match GatewayServer::start(gw) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tokprof selftest: failed to start gateway: {e}");
            return 1;
        }
    };
    let port = srv.port;

    let sys_msg = format!(
        "# Role\nYou are a helpful assistant.\n{}\n# Style\n{}",
        "rule one. ".repeat(40),
        "be brief. ".repeat(40)
    );
    let tools = json!([{
        "type": "function",
        "function": {
            "name": "get_weather",
            "description": "Get weather",
            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
        }
    }]);
    let msgs = json!([
        {"role": "system", "content": sys_msg},
        {"role": "user", "content": "hello"},
        {"role": "assistant", "content": "hi"},
        {"role": "user", "content": "what is the weather? use-tool"}
    ]);

    let ok1 = post_http(
        port,
        "/openai/v1/chat/completions",
        &json!({"model": "gpt-4o", "messages": msgs, "tools": tools}),
    );
    let ok2 = post_http(
        port,
        "/openai/v1/chat/completions",
        &json!({"model": "gpt-4o", "messages": msgs, "tools": tools, "stream": true}),
    );
    let ok3 = post_http(
        port,
        "/openai/v1/responses",
        &json!({"model": "gpt-4o", "instructions": sys_msg, "input": "hello"}),
    );
    let ok4 = post_http(
        port,
        "/anthropic/v1/messages",
        &json!({"model": "claude-x", "max_tokens": 16, "system": sys_msg, "messages": [{"role": "user", "content": "hello"}]}),
    );
    let ok5 = post_http(
        port,
        "/gemini/v1beta/models/gemini-2.5-flash:generateContent",
        &json!({"systemInstruction": {"parts": [{"text": sys_msg}]}, "contents": [{"role": "user", "parts": [{"text": "hello"}]}]}),
    );
    let ok6 = post_http(
        port,
        "/v1/traces",
        &json!({
            "resourceSpans": [{
                "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "selftest-agent"}}]},
                "scopeSpans": [{
                    "spans": [{
                        "traceId": "0102030405060708090a0b0c0d0e0f10",
                        "spanId": "0102030405060708",
                        "name": "chat gpt-4o",
                        "startTimeUnixNano": "1700000000000000000",
                        "endTimeUnixNano": "1700000000250000000",
                        "attributes": [
                            {"key": "gen_ai.system", "value": {"stringValue": "openai"}},
                            {"key": "gen_ai.request.model", "value": {"stringValue": "gpt-4o"}},
                            {"key": "gen_ai.usage.input_tokens", "value": {"intValue": "55"}},
                            {"key": "gen_ai.usage.output_tokens", "value": {"intValue": "7"}}
                        ],
                        "events": [
                            {"name": "gen_ai.user.message", "attributes": [{"key": "content", "value": {"stringValue": "hello otlp"}}]},
                            {"name": "gen_ai.choice", "attributes": [{"key": "content", "value": {"stringValue": "otlp ok"}}]}
                        ]
                    }]
                }]
            }]
        }),
    );

    srv.stop();
    writer.close();
    mock.stop();

    let recs = read_jsonl(&out_file, None);
    let _ = fs::remove_dir_all(&tmp_dir);

    let all_http_ok = ok1 && ok2 && ok3 && ok4 && ok5 && ok6 && recs.len() == 6;
    let with_usage = recs
        .iter()
        .filter(|r| r.usage.get("input_total").and_then(|v| v.as_i64()).unwrap_or(0) > 0)
        .count();
    let usage_ok = !recs.is_empty() && with_usage == recs.len();

    let kinds: HashSet<String> = recs
        .iter()
        .flat_map(|r| r.segments.iter().map(|s| s.kind.clone()))
        .collect();
    let segs_ok = kinds.contains("system")
        && kinds.contains("user_prompt")
        && kinds.contains("tool_def");

    let providers: HashSet<String> = recs.iter().map(|r| r.provider.clone()).collect();
    let provs_ok = providers.contains("openai_chat")
        && providers.contains("openai_responses")
        && providers.contains("anthropic")
        && providers.contains("gemini");

    let results = [
        (
            "gateway & OTLP capture (chat + stream + responses + anthropic + gemini + otlp)",
            all_http_ok,
            format!("recorded {}/6 call(s)", recs.len()),
        ),
        (
            "usage captured (json + sse)",
            usage_ok,
            format!("{with_usage}/{} records", recs.len()),
        ),
        (
            "segments decomposed",
            segs_ok,
            {
                let mut ks: Vec<_> = kinds.iter().cloned().collect();
                ks.sort();
                ks.join(", ")
            },
        ),
        (
            "providers seen",
            provs_ok,
            {
                let mut ps: Vec<_> = providers.iter().cloned().collect();
                ps.sort();
                ps.join(", ")
            },
        ),
    ];

    let all_passed = results.iter().all(|(_, p, _)| *p);
    println!("tokprof selftest");
    for (name, passed, detail) in &results {
        println!(
            "  [{}] {name}: {detail}",
            if *passed { "ok" } else { "FAIL" }
        );
    }
    println!("{}", if all_passed { "PASS" } else { "FAIL" });
    if all_passed {
        0
    } else {
        1
    }
}
