pub mod analyze;
pub mod appinsights;
pub mod cli;
pub mod counting;
pub mod detect;
pub mod doctor;
pub mod ebpf;
pub mod gateway;
pub mod help_text;
pub mod mitm;
pub mod model;
pub mod parse_request;
pub mod parse_response;
pub mod report;
pub mod runner;
pub mod segment;
pub mod selftest;
pub mod session;
pub mod store;
pub mod upstream;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;
    use std::time::Instant;

    #[test]
    fn test_perf_metrics_snapshot() {
        let tk = counting::get_counter(Some("gpt-4o"), true);
        let sys_prompt = (0..40)
            .map(|i| format!("# Section {i}\n{}", "Detailed guidance for section. ".repeat(20)))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = tk.count(&sys_prompt); // warm up cache

        // 1. Token count cache hit (10,000 lookups on 26 KB system prompt)
        let t0 = Instant::now();
        for _ in 0..10_000 {
            let _ = tk.count(&sys_prompt);
        }
        let count_cache_us = t0.elapsed().as_micros();

        // 2. hash_text (10,000 calls on 26 KB system prompt)
        let salt = b"0123456789abcdef";
        let t0 = Instant::now();
        for _ in 0..10_000 {
            let _ = segment::hash_text(&sys_prompt, salt);
        }
        let hash_text_us = t0.elapsed().as_micros();

        // 3. Canonical JSON serialization + parse_request (1,000 calls)
        let payload = json!({
            "untrusted_input": {
                "utterance": "How do I optimize tokens?",
                "context": [{"role": "user", "content": "hi"}, {"role": "assistant", "content": "hello"}],
                "capabilities": (0..15).map(|i| json!({"name": format!("tool_{i}"), "description": "desc ".repeat(20)})).collect::<Vec<_>>()
            }
        });
        let req_body = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": sys_prompt},
                {"role": "user", "content": payload.to_string()}
            ],
            "tools": (0..15).map(|i| json!({"type": "function", "function": {"name": format!("fn_{i}"), "parameters": {"type": "object", "properties": {"x": {"type": "string"}}}}})).collect::<Vec<_>>()
        });
        let t0 = Instant::now();
        let mut parsed = None;
        for _ in 0..500 {
            parsed = parse_request::parse_request(parse_request::OPENAI_CHAT, &req_body).ok();
        }
        let parse_req_us = t0.elapsed().as_micros();
        let parsed = parsed.unwrap();

        // 4. SSEParser streaming 2,000 chunks x 20 runs
        let mut sse_blob = Vec::new();
        for i in 0..2000 {
            sse_blob.extend_from_slice(
                format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"token_{i} \"}}}}]}}\n\n").as_bytes(),
            );
        }
        sse_blob.extend_from_slice(
            b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1200,\"completion_tokens\":2000}}\n\ndata: [DONE]\n\n",
        );
        let t0 = Instant::now();
        for _ in 0..20 {
            let mut sp = parse_response::SSEParser::new(parse_request::OPENAI_CHAT);
            sp.feed(&sse_blob);
            let _ = sp.finish();
        }
        let sse_us = t0.elapsed().as_micros();

        // 5. Detector classify_host + path_hint + model_from_path (10,000 calls)
        let t0 = Instant::now();
        for _ in 0..10_000 {
            let _ = detect::classify_host("vllm.inference.svc.cluster.local", 8000);
            let _ = detect::path_hint("/openai/deployments/gpt-4o/chat/completions");
            let _ = detect::model_from_path("/openai/deployments/gpt-4o/chat/completions");
        }
        let detect_us = t0.elapsed().as_micros();

        // 6. Full summarize + render_pretty + render_html + render_csv (200 records x 20 runs)
        let base_rec = segment::build_record(segment::BuildRecordArgs {
            parsed: &parsed,
            response: None,
            counter: &tk,
            salt,
            components: None,
            endpoint: "/v1/chat/completions",
            status: Some(200),
            latency_ms: Some(250),
            ttfb_ms: Some(80),
            run: Some("r1".to_string()),
            error: None,
            ts: Some(1700000000.0),
            include_text: Some(true),
        });
        let mut recs = Vec::with_capacity(200);
        for i in 0..200 {
            let mut r = base_rec.clone();
            r.ts = 1700000000.0 + ((i / 4) as f64) * 10.0 + ((i % 4) as f64) * 0.2;
            r.usage.insert("input_total".to_string(), json!(4500 + (i % 10) * 100));
            r.usage.insert("cached".to_string(), json!(2000));
            r.usage.insert("output".to_string(), json!(150));
            recs.push(r);
        }
        let t0 = Instant::now();
        for _ in 0..20 {
            let s = analyze::summarize(&recs, 5.0);
            let _ = report::render_pretty(&recs, 5.0, false, None, false, Some(&s));
            let _ = report::render_html(&recs, 5.0, None, "Bench", Some(&s));
            let _ = report::render_csv(&recs, 5.0, Some(&s));
        }
        let report_suite_us = t0.elapsed().as_micros();

        eprintln!(
            "BENCH_SNAPSHOT: count_cache_10k_ms={:.2} hash_10k_ms={:.2} parse_req_500_ms={:.2} sse_20x_ms={:.2} detect_10k_ms={:.2} report_200recs_20x_ms={:.2}",
            count_cache_us as f64 / 1000.0,
            hash_text_us as f64 / 1000.0,
            parse_req_us as f64 / 1000.0,
            sse_us as f64 / 1000.0,
            detect_us as f64 / 1000.0,
            report_suite_us as f64 / 1000.0,
        );
    }

    #[test]
    fn test_counting_and_caching() {
        let h = counting::TokenCounter::heuristic();
        assert_eq!(h.count(""), 0);
        assert_eq!(h.count("abcd"), 1);
        assert!(h.count("안녕하세요 hello") > 5);

        let tk = counting::get_counter(Some("openai/gpt-5-mini"), true);
        assert_eq!(tk.method, "tiktoken:o200k_base");
        let c1 = tk.count("Hello world, this is a token counting test.");
        let c2 = tk.count("Hello world, this is a token counting test.");
        assert_eq!(c1, c2);
        assert!(c1 > 5);
    }

    #[test]
    fn test_segment_redaction_and_splitting() {
        let aws = format!("AKIA{}", "0123456789ABCDEF");
        let gcp = format!("AIza{}", "A".repeat(35));
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let red = segment::redact_secrets(&format!("aws={aws} gcp={gcp} jwt={jwt}"));
        assert!(!red.contains(&aws));
        assert!(!red.contains(&gcp));
        assert!(!red.contains(jwt));
        assert_eq!(red.matches("***REDACTED***").count(), 3);

        let long_sys = format!(
            "# Role\n{}\n# Rules\n{}\n# Style\n{}\n",
            "You are a careful assistant. ".repeat(20),
            "Always cite sources. ".repeat(25),
            "Be brief and direct. ".repeat(25)
        );
        let secs = segment::split_sections(&long_sys, segment::MIN_SECTION_CHARS);
        let names: Vec<Option<&str>> = secs.iter().map(|(n, _)| n.as_deref()).collect();
        assert_eq!(names, vec![Some("Role"), Some("Rules"), Some("Style")]);

        assert_eq!(segment::quality_label(Some(10), Some(0)), None);
        assert_eq!(segment::quality_label(Some(10), Some(-5)), None);
        assert_eq!(
            segment::quality_label(Some(10), Some(1000)).as_deref(),
            Some("good")
        );
    }

    #[test]
    fn test_structured_json_user_decomposition_and_templates() {
        let payload = json!({
            "untrusted_input": {
                "utterance": "현재 프로덕션 DB 백업 상태 확인해줘",
                "context": ["user: 안녕하세요", "assistant: 반갑습니다!"],
                "capabilities": [{"kind": "function_type", "name": "query.resource_current_state"}],
                "locale": "ko"
            }
        });
        let body = json!({
            "model": "narrator-gpt-5-4-mini",
            "messages": [
                {"role": "system", "content": "You are an operational assistant."},
                {"role": "user", "content": payload.to_string()}
            ],
            "response_format": {"type": "json_schema", "json_schema": {"name": "semantic-judgment", "schema": {"type": "object"}}}
        });
        let parsed = parse_request::parse_request(parse_request::OPENAI_CHAT, &body).unwrap();
        let kinds: Vec<&str> = parsed.segments.iter().map(|s| s.kind.as_str()).collect();
        assert!(kinds.contains(&"system"));
        assert!(kinds.contains(&"history_user"));
        assert!(kinds.contains(&"history_assistant"));
        assert!(kinds.contains(&"user_prompt"));
        assert!(kinds.contains(&"tool_def"));
        assert!(kinds.contains(&"response_schema"));
        assert_eq!(parsed.step_hint.as_deref(), Some("semantic-judgment"));

        let chatml = "<|im_start|>system\nYou are terse.<|im_end|>\n<|im_start|>user\nhi<|im_end|>";
        let turns = parse_request::split_chat_template(chatml).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].0, "system");
        assert_eq!(turns[1].0, "user");
    }

    #[test]
    fn test_sse_and_eventstream_bounds() {
        let mut sse = parse_response::SSEParser::new(parse_request::OPENAI_CHAT);
        sse.feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\n");
        sse.feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}],\"usage\":{\"prompt_tokens\":50.0,\"completion_tokens\":2}}\r\n\r\ndata: [DONE]\n\n");
        let res = sse.finish();
        assert_eq!(res.text, "hello");
        assert_eq!(res.usage.input_total, Some(50));
        assert_eq!(res.usage.output, Some(2));

        // Malformed EventStream with huge headers_len (0xFFFF_FFFF) must not overflow or panic
        let bad_frame = [
            0x00, 0x00, 0x00, 0x20, // total_len = 32
            0xFF, 0xFF, 0xFF, 0xFF, // headers_len = u32::MAX
            0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let objs = parse_response::extract_eventstream_objects_bytes(&bad_frame);
        assert!(objs.is_empty());
    }

    #[test]
    fn test_detector_and_glob_cidr() {
        assert!(detect::glob_match("*.internal", "vllm.corp.internal"));
        assert!(detect::glob_match("gpu-?-*.local", "gpu-1-node.local"));
        assert!(!detect::glob_match("*.internal", "api.openai.com"));
        assert!(detect::cidr_match("10.0.0.0/8", "10.240.1.55"));
        assert!(!detect::cidr_match("10.0.0.0/8", "192.168.1.1"));

        let det = detect::Detector::default();
        let body = json!({"model": "qwen", "messages": [{"role": "user", "content": "hi"}]});
        let v = det.inspect(
            "POST",
            "vllm.inference.svc.cluster.local",
            8000,
            "/v1/chat/completions",
            Some(&body),
        );
        assert!(v.llm);
        assert_eq!(v.api.as_deref(), Some("openai_chat"));
    }

    #[test]
    fn test_body_framer_and_csv_report() {
        let mut h_cl = HashMap::new();
        h_cl.insert("content-length".to_string(), "5, 5".to_string());
        let mut bf1 = gateway::BodyFramer::new(&h_cl, false);
        assert_eq!(bf1.feed(b"hello world"), b"hello");
        assert!(bf1.done);

        let mut h_ch = HashMap::new();
        h_ch.insert("transfer-encoding".to_string(), "chunked".to_string());
        let mut bf2 = gateway::BodyFramer::new(&h_ch, false);
        assert_eq!(bf2.feed(b"3\r\nabc\r\n2;ext=1\r\nde\r\n0\r\n\r\n"), b"abcde");
        assert!(bf2.done);

        // Verify CSV column count is strictly 34 across header, session, turn, and call rows
        let counter = counting::TokenCounter::heuristic();
        let req = parse_request::parse_request(
            parse_request::OPENAI_CHAT,
            &json!({"model": "gpt-4o", "messages": [{"role": "system", "content": "sys"}, {"role": "user", "content": "hi\0world\r\nline2"}]}),
        )
        .unwrap();
        let resp = parse_response::ParsedResponse {
            usage: model::Usage {
                input_total: Some(25),
                cached: Some(10),
                output: Some(5),
                ..Default::default()
            },
            text: "ok\0resp".to_string(),
            has_usage: true,
            ..Default::default()
        };
        let rec = segment::build_record(segment::BuildRecordArgs {
            parsed: &req,
            response: Some(&resp),
            counter: &counter,
            salt: b"salt",
            components: None,
            endpoint: "/v1/chat/completions",
            status: Some(200),
            latency_ms: Some(120),
            ttfb_ms: Some(40),
            run: Some("r1".to_string()),
            error: None,
            ts: Some(1700000000.0),
            include_text: Some(true),
        });
        let csv_out = report::render_csv(std::slice::from_ref(&rec), 5.0, None);
        assert!(!csv_out.contains('\0'));
        let header_line = csv_out.lines().next().unwrap();
        assert_eq!(header_line.split(',').count(), 34);
        let session_line = csv_out.lines().nth(1).unwrap();
        assert_eq!(session_line.split(',').count(), 34);
        let html_out = report::render_html(std::slice::from_ref(&rec), 5.0, None, "Test", None);
        assert!(html_out.contains("<!DOCTYPE html>") && html_out.contains("Per-Turn Breakdown"));
        let pretty_out = report::render_pretty(std::slice::from_ref(&rec), 5.0, false, None, false, None);
        assert!(pretty_out.contains("Per-Session breakdown") && pretty_out.contains("Per-Turn breakdown"));
    }

    #[test]
    fn test_cli_bilingual_help() {
        let en = help_text::render_detailed_help(Some("en"), None);
        assert!(en.contains("Zero-Code LLM Agent Token Profiler"));
        let ko = help_text::render_detailed_help(Some("ko"), None);
        assert!(ko.contains("코드 수정 없는 LLM 에이전트 토큰 프로파일러"));

        assert_eq!(session::parse_capped_duration_secs(None), 180);
        assert_eq!(session::parse_capped_duration_secs(Some("45s")), 45);
        assert_eq!(session::parse_capped_duration_secs(Some("2m")), 120);
        assert_eq!(session::parse_capped_duration_secs(Some("600s")), 180);
        assert_eq!(session::parse_capped_duration_secs(Some("10m")), 180);

        let pretty_json = report::format_prompt_html(
            r#"(response_schema) [NarratorSchema]
{"type":"object","required":["summary","status"],"properties":{"summary":{"type":"string"}}}"#,
        );
        assert!(pretty_json.contains(r#"class="p-badge""#));
        assert!(pretty_json.contains(r#"class="j-key""#));
        assert!(pretty_json.contains(r#"class="j-str""#));
    }
}

