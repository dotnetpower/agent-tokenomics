use crate::analyze::{estimate_cost, summarize, SummaryReport};
use crate::counting::fast_hash_pair;
use crate::model::CallRecord;
use crate::segment::{fast_char_count, format_with_commas, safe_char_prefix};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::Write as _;

pub const CSV_COLUMNS: &[&str] = &[
    "row_type",
    "session",
    "turn",
    "call_index",
    "call_id",
    "timestamp",
    "model",
    "provider",
    "step",
    "step_label",
    "turns_count",
    "calls_count",
    "input_total",
    "cached_tokens",
    "output_tokens",
    "system_tokens",
    "history_tokens",
    "history_user_tokens",
    "history_assistant_tokens",
    "user_prompt_tokens",
    "tools_schema_tokens",
    "tool_def_tokens",
    "tool_call_tokens",
    "tool_result_tokens",
    "response_schema_tokens",
    "other_tokens",
    "residual_tokens",
    "latency_ms",
    "status",
    "system_prompt",
    "conversation_history",
    "user_prompt",
    "tools_schema",
    "assistant_response",
];

pub fn kind_label(kind: &str) -> &str {
    match kind {
        "system" => "System prompt",
        "history_user" => "History: user turns",
        "history_assistant" => "History: assistant turns",
        "user_prompt" => "Current user prompt",
        "tool_def" => "Tools: definitions",
        "tool_call" => "Tools: calls",
        "tool_result" => "Tools: results",
        "attachment" => "Attachments (uncounted)",
        "response_schema" => "Response schema",
        "raw_prompt" => "Raw prompt (not split)",
        "other" => "Other",
        "residual" => "Unattributed (template/hidden)",
        other => other,
    }
}

pub fn kind_color_hex(kind: &str) -> &'static str {
    match kind {
        "system" => "#6366f1",
        "tool_def" => "#8b5cf6",
        "tool_call" => "#a855f7",
        "tool_result" => "#ec4899",
        "history_user" => "#0ea5e9",
        "history_assistant" => "#38bdf8",
        "user_prompt" => "#10b981",
        "response_schema" => "#f59e0b",
        "raw_prompt" => "#14b8a6",
        "attachment" => "#64748b",
        "other" => "#94a3b8",
        "residual" => "#cbd5e1",
        _ => "#94a3b8",
    }
}

fn ansi_code(tag: &str) -> &'static str {
    match tag {
        "system" => "\x1b[38;5;63m",
        "tool_def" => "\x1b[38;5;135m",
        "tool_call" => "\x1b[38;5;171m",
        "tool_result" => "\x1b[38;5;204m",
        "history_user" => "\x1b[38;5;39m",
        "history_assistant" => "\x1b[38;5;75m",
        "user_prompt" => "\x1b[38;5;42m",
        "response_schema" => "\x1b[38;5;214m",
        "raw_prompt" => "\x1b[38;5;37m",
        "residual" => "\x1b[38;5;245m",
        "high" => "\x1b[1;31m",
        "medium" => "\x1b[1;33m",
        "low" => "\x1b[1;36m",
        "info" => "\x1b[1;34m",
        "bold" => "\x1b[1m",
        "dim" => "\x1b[2m",
        "reset" => "\x1b[0m",
        _ => "",
    }
}

pub fn fmt_num(n: i64) -> String {
    let abs = n.unsigned_abs() as usize;
    let s = format_with_commas(abs);
    if n < 0 {
        format!("-{s}")
    } else {
        s
    }
}

pub fn fmt_usize(n: usize) -> String {
    format_with_commas(n)
}

fn ascii_bar(share: f64, width: usize) -> String {
    let clamped = share.clamp(0.0, 1.0);
    let n = ((clamped * (width as f64)).round() as usize).min(width);
    format!("{}{}", "#".repeat(n), ".".repeat(width - n))
}

pub fn utf8_ok() -> bool {
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            let vl = v.trim().to_ascii_lowercase();
            if !vl.is_empty() {
                return !(vl == "c" || vl == "posix" || vl.contains("ascii"));
            }
        }
    }
    true
}

fn ubo_bar(share: f64, width: usize, unicode_ok: bool) -> String {
    if !unicode_ok {
        return ascii_bar(share, width);
    }
    const BLOCKS: &[char] = &[' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
    let clamped = share.clamp(0.0, 1.0);
    let total_eighths = (clamped * (width as f64) * 8.0).round() as usize;
    let full = (total_eighths / 8).min(width);
    let rem = total_eighths % 8;
    let mut bar = "█".repeat(full);
    let mut used = full;
    if rem > 0 && full < width {
        bar.push(BLOCKS[rem]);
        used += 1;
    }
    if used < width {
        bar.push_str(&"░".repeat(width - used));
    }
    bar
}

pub fn preview_line(text: &str, limit: usize) -> String {
    if text.is_empty() || limit == 0 {
        return String::new();
    }
    let (sample, _) = safe_char_prefix(text, limit * 3);
    let clean: String = sample.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean_chars = fast_char_count(&clean);
    if clean_chars > limit {
        let (pref, _) = safe_char_prefix(&clean, limit.saturating_sub(1));
        format!("{pref}…")
    } else {
        clean
    }
}

pub fn indent_block(text: &str, prefix: &str, max_lines: usize, max_chars: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let orig_chars = fast_char_count(text);
    let trimmed = text.trim();
    let (clipped, clipped_chars) = safe_char_prefix(trimmed, max_chars);
    let mut truncated = clipped_chars > max_chars;
    let mut raw_lines: Vec<&str> = clipped.lines().collect();
    if raw_lines.len() > max_lines {
        raw_lines.truncate(max_lines);
        truncated = true;
    }
    let mut out: Vec<String> = raw_lines
        .into_iter()
        .map(|ln| format!("{prefix}{ln}"))
        .collect();
    if truncated {
        out.push(format!(
            "{prefix}... ({} chars total; see HTML dashboard or --full-prompts for full text)",
            fmt_usize(orig_chars)
        ));
    }
    out
}

pub fn render_cost(records: &[CallRecord], prices: &Map<String, Value>) -> String {
    let c = estimate_cost(records, prices);
    let mut line = format!(
        "Estimated cost: {:.4} (currency units of your price file; prices change, treat as an estimate)",
        c.total
    );
    if !c.models_without_price.is_empty() {
        line.push_str(&format!(
            "\n  no price for: {}",
            c.models_without_price.join(", ")
        ));
    }
    line
}

pub fn render_compare(result: &Value) -> String {
    let req_b = result
        .get("requests")
        .and_then(|r| r.get("before"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let req_a = result
        .get("requests")
        .and_then(|r| r.get("after"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let mut lines = vec![format!("Requests: before {req_b}, after {req_a}")];
    for (key, title) in [
        ("input_per_request", "Input tokens per request"),
        ("calls_per_request", "LLM calls per request"),
    ] {
        let Some(m) = result.get("metrics").and_then(|o| o.get(key)) else {
            continue;
        };
        let Some(diff) = m.get("diff").and_then(|v| v.as_f64()) else {
            lines.push(format!("{title}: not enough data"));
            continue;
        };
        let b_mean = m.get("before_mean").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let a_mean = m.get("after_mean").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let ci = m.get("ci95").and_then(|v| v.as_array());
        let (ci0, ci1) = ci
            .map(|arr| {
                (
                    arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0),
                    arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
                )
            })
            .unwrap_or((0.0, 0.0));
        let rel = m
            .get("relative_change_pct")
            .and_then(|v| v.as_f64())
            .map(|p| format!("{p:+.1}%"))
            .unwrap_or_else(|| "n/a".to_string());
        let sig = m
            .get("significant")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let verdict = if sig { "significant" } else { "within noise" };
        lines.push(format!(
            "{title}: {b_mean:.1} -> {a_mean:.1}  ({rel}; diff {diff:+.1}, 95% CI {ci0:+.1} to {ci1:+.1}; {verdict})"
        ));
    }
    if !result
        .get("reliable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        lines.push(
            "Warning: fewer than 5 requests on a side; the interval is not reliable. Repeat runs on a fixed task set."
                .to_string(),
        );
    }
    lines.push(
        "Reminder: report task success next to cost; fewer tokens only count if quality held."
            .to_string(),
    );
    lines.join("\n")
}

pub fn render_text(
    records: &[CallRecord],
    gap_s: f64,
    full_prompts: bool,
    summary_opt: Option<&SummaryReport>,
) -> String {
    if records.is_empty() {
        return "No LLM calls recorded.".to_string();
    }
    let owned_s;
    let s = match summary_opt {
        Some(su) => su,
        None => {
            owned_s = summarize(records, gap_s);
            &owned_s
        }
    };
    let total = s.input_total.max(1) as f64;
    let mut lines = vec![
        format!(
            "Sessions: {}   Turns: {}   Calls: {}   Input tokens: {}   Output tokens: {}   Cached input: {}",
            s.sessions.len(),
            s.turns.len(),
            s.calls,
            fmt_num(s.input_total),
            fmt_num(s.output_total),
            fmt_num(s.cached_total),
        ),
        format!(
            "First call input (fixed overhead): {}   Median input per call: {}",
            fmt_num(s.first_call_input),
            fmt_num(s.median_input)
        ),
    ];
    if !s.fan_out.is_empty() {
        let mut fo = s.fan_out.clone();
        fo.sort_unstable();
        let med_fo = fo[fo.len() / 2];
        let max_fo = *fo.last().unwrap_or(&0);
        lines.push(format!(
            "LLM calls per request (gap-based grouping): median {med_fo}, max {max_fo}, requests {}",
            fo.len()
        ));
    }
    lines.push(String::new());
    lines.push("Input token breakdown".to_string());
    lines.push("---------------------".to_string());
    for kind in &s.kinds_order {
        let v = s.by_kind.get(kind).copied().unwrap_or(0);
        if v != 0 {
            let share = (v as f64) / total;
            lines.push(format!(
                "{:<32}{:>12}  {:5.1}%  {}",
                kind_label(kind),
                fmt_num(v),
                100.0 * share,
                ascii_bar(share, 24)
            ));
        }
    }
    if !s.sessions.is_empty() {
        lines.extend([
            String::new(),
            "Per-session summary".to_string(),
            "-------------------".to_string(),
            format!(
                "{:<9}{:>6}{:>7}{:>11}{:>11}{:>11}{:>11}{:>12}{:>11}{:>9}",
                "session",
                "turns",
                "calls",
                "system",
                "history",
                "user",
                "tools/sch",
                "input",
                "cached",
                "output"
            ),
        ]);
        for sess in &s.sessions {
            lines.push(format!(
                "{:<9}{:>6}{:>7}{:>11}{:>11}{:>11}{:>11}{:>12}{:>11}{:>9}",
                sess.session,
                sess.turns_count,
                sess.calls_count,
                fmt_num(sess.system),
                fmt_num(sess.history),
                fmt_num(sess.user_prompt),
                fmt_num(sess.tools_schema),
                fmt_num(sess.input_total),
                fmt_num(sess.cached_total),
                fmt_num(sess.output_total)
            ));
        }
    }
    if !s.turns.is_empty() {
        lines.extend([
            String::new(),
            "Per-turn breakdown (System / History / User Prompt / Tools & Schema)".to_string(),
            "--------------------------------------------------------------------".to_string(),
            format!(
                "{:<9}{:>6}{:>11}{:>11}{:>11}{:>11}{:>12}{:>11}{:>9}",
                "turn", "calls", "system", "history", "user", "tools/sch", "input", "cached", "output"
            ),
        ]);
        for t in &s.turns {
            let tag = format!("{}·T{}", t.session, t.turn);
            lines.push(format!(
                "{:<9}{:>6}{:>11}{:>11}{:>11}{:>11}{:>12}{:>11}{:>9}",
                tag,
                t.calls_count,
                fmt_num(t.system),
                fmt_num(t.history),
                fmt_num(t.user_prompt),
                fmt_num(t.tools_schema),
                fmt_num(t.input_total),
                fmt_num(t.cached_total),
                fmt_num(t.output_total)
            ));
            if !t.user_text.is_empty() {
                lines.push(format!("    User prompt: {}", preview_line(&t.user_text, 100)));
            }
            if !t.history_text.is_empty() {
                lines.push(format!(
                    "    History:     {}",
                    preview_line(&t.history_text, 100)
                ));
            }
            if !t.response_text.is_empty() {
                lines.push(format!(
                    "    Assistant:   {}",
                    preview_line(&t.response_text, 100)
                ));
            }
        }
    }
    if !s.call_details.is_empty() {
        lines.extend([
            String::new(),
            "Per-call breakdown".to_string(),
            "------------------".to_string(),
            format!(
                "{:<4}{:<8}{:<22}{:>9}{:>9}{:>9}{:>10}{:>10}{:>9}{:>7}",
                "#", "turn", "model", "system", "history", "user", "tools/sch", "input", "cached", "out"
            ),
        ]);
        for cd in s.call_details.iter().take(40) {
            let tag = format!("{}·T{}", cd.session, cd.turn);
            let m_trunc: String = cd.model.chars().take(20).collect();
            lines.push(format!(
                "#{:<3}{:<8}{:<22}{:>9}{:>9}{:>9}{:>10}{:>10}{:>9}{:>7}",
                cd.index,
                tag,
                m_trunc,
                fmt_num(cd.system),
                fmt_num(cd.history),
                fmt_num(cd.user_prompt),
                fmt_num(cd.tools_schema),
                fmt_num(cd.input_total),
                fmt_num(cd.cached),
                fmt_num(cd.output)
            ));
        }
    }
    if s.steps.len() > 1 || (s.steps.first().map(|st| st.step.as_str()) != Some("(no sy") && !s.steps.is_empty()) {
        lines.extend([
            String::new(),
            "Per-step token map (steps identified by system prompt)".to_string(),
            "------------------------------------------------------".to_string(),
            format!(
                "{:<8}{:<28}{:>6}{:>12}{:>12}",
                "step", "label", "calls", "input", "fixed/call"
            ),
        ]);
        for st in s.steps.iter().take(12) {
            let lbl: String = st
                .label
                .as_deref()
                .or(st.model.as_deref())
                .unwrap_or("-")
                .chars()
                .take(26)
                .collect();
            lines.push(format!(
                "{:<8}{:<28}{:>6}{:>12}{:>12}",
                st.step,
                lbl,
                st.calls,
                fmt_num(st.input),
                fmt_num(st.fixed_per_call)
            ));
        }
    }
    if s.endpoints.len() > 1
        || (s.endpoints.first().map(|e| e.class.as_str()) != Some("unknown")
            && !s.endpoints.is_empty())
    {
        lines.extend([
            String::new(),
            "Endpoints (by detected API and host class)".to_string(),
            "------------------------------------------".to_string(),
            format!(
                "{:<26}{:<24}{:<16}{:>6}{:>12}",
                "host", "class", "api", "calls", "input"
            ),
        ]);
        for e in s.endpoints.iter().take(12) {
            let who_raw = e.host.clone().unwrap_or_else(|| {
                if e.id.is_empty() {
                    "-".to_string()
                } else {
                    format!("#{}", e.id)
                }
            });
            let who: String = who_raw.chars().take(24).collect();
            let cls: String = e.class.chars().take(22).collect();
            lines.push(format!(
                "{:<26}{:<24}{:<16}{:>6}{:>12}",
                who,
                cls,
                e.api,
                e.calls,
                fmt_num(e.input)
            ));
        }
    }
    let has_prompts = s.call_details.iter().any(|cd| {
        !cd.prompts.system.is_empty()
            || !cd.prompts.history.is_empty()
            || !cd.prompts.user_prompt.is_empty()
            || !cd.prompts.tools_schema.is_empty()
            || !cd.prompts.response.is_empty()
    });
    if has_prompts {
        let max_l = if full_prompts { 10_000 } else { 10 };
        let max_c = if full_prompts { 100_000 } else { 700 };
        lines.extend([
            String::new(),
            "Captured prompt & response contents".to_string(),
            "-----------------------------------".to_string(),
        ]);
        for cd in s.call_details.iter().take(20) {
            let p = &cd.prompts;
            if p.system.is_empty()
                && p.history.is_empty()
                && p.user_prompt.is_empty()
                && p.tools_schema.is_empty()
                && p.response.is_empty()
            {
                continue;
            }
            lines.push(format!(
                "  Call #{} ({}·T{} | {} | step {})",
                cd.index, cd.session, cd.turn, cd.model, cd.step
            ));
            for (val, title) in [
                (&p.system, format!("System Prompt ({} tok)", fmt_num(cd.system))),
                (
                    &p.history,
                    format!("Conversation History ({} tok)", fmt_num(cd.history)),
                ),
                (
                    &p.user_prompt,
                    format!("User Prompt ({} tok)", fmt_num(cd.user_prompt)),
                ),
                (
                    &p.tools_schema,
                    format!("Tools & Schema ({} tok)", fmt_num(cd.tools_schema)),
                ),
                (
                    &p.response,
                    format!("Assistant Output ({} tok)", fmt_num(cd.output)),
                ),
            ] {
                if !val.is_empty() {
                    lines.push(format!("    [{title}]"));
                    lines.extend(indent_block(val, "      ", max_l, max_c));
                }
            }
        }
    }
    if !s.findings.is_empty() {
        lines.extend([String::new(), "Findings".to_string(), "--------".to_string()]);
        for f in &s.findings {
            lines.push(format!("[{}] {}", f.severity, f.text));
            lines.push(format!("        see {}", f.see));
        }
    }
    lines.push(String::new());
    lines.push(
        "Note: segment counts are local estimates; 'Unattributed' is reported input minus counted segments."
            .to_string(),
    );
    lines.join("\n")
}

pub fn render_pretty(
    records: &[CallRecord],
    gap_s: f64,
    color: bool,
    prices: Option<&Map<String, Value>>,
    full_prompts: bool,
    summary_opt: Option<&SummaryReport>,
) -> String {
    if records.is_empty() {
        return "No LLM calls recorded.".to_string();
    }
    let c = |tag: &str, text: &str| -> String {
        if color {
            format!("{}{}{}", ansi_code(tag), text, ansi_code("reset"))
        } else {
            text.to_string()
        }
    };
    let owned_s;
    let s = match summary_opt {
        Some(su) => su,
        None => {
            owned_s = summarize(records, gap_s);
            &owned_s
        }
    };
    let total = s.input_total.max(1) as f64;
    let cache_pct = if s.input_total > 0 {
        100.0 * (s.cached_total as f64) / total
    } else {
        0.0
    };
    let mut fo = s.fan_out.clone();
    fo.sort_unstable();
    let med_fo = if fo.is_empty() { 0 } else { fo[fo.len() / 2] };
    let max_fo = fo.last().copied().unwrap_or(0);

    let u_ok = utf8_ok();
    let rule = if u_ok { "─" } else { "-" }.repeat(92);
    let top = if u_ok {
        format!("╭{}╮", "─".repeat(90))
    } else {
        format!("+{}+", "-".repeat(90))
    };
    let mid = if u_ok {
        "│  tokprof — LLM Token Profile (Per-Session · Per-Turn · Prompt Inspector)                 │"
    } else {
        "|  tokprof - LLM Token Profile (Per-Session / Per-Turn / Prompt Inspector)                 |"
    };
    let bot = if u_ok {
        format!("╰{}╯", "─".repeat(90))
    } else {
        format!("+{}+", "-".repeat(90))
    };

    let mut lines = vec![
        c("bold", &top),
        c("bold", mid),
        c("bold", &bot),
        format!(
            "  Sessions: {}   Turns: {}   Calls: {}   Input: {}   Output: {}   Cached: {} ({cache_pct:.1}%)",
            c("bold", &fmt_usize(s.sessions.len())),
            c("bold", &fmt_usize(s.turns.len())),
            c("bold", &fmt_usize(s.calls)),
            c("bold", &fmt_num(s.input_total)),
            c("bold", &fmt_num(s.output_total)),
            c("bold", &fmt_num(s.cached_total)),
        ),
        format!(
            "  First-call overhead: {} tok   Median input/call: {} tok   Fan-out: median {med_fo}, max {max_fo} calls/turn",
            fmt_num(s.first_call_input),
            fmt_num(s.median_input)
        ),
    ];
    if let Some(pmap) = prices {
        lines.push(format!("  {}", render_cost(records, pmap)));
    }
    lines.push(String::new());
    lines.push(c("bold", "  1. Input token breakdown (total)"));
    lines.push(format!("  {rule}"));
    for kind in &s.kinds_order {
        let v = s.by_kind.get(kind).copied().unwrap_or(0);
        if v != 0 {
            let share = (v as f64) / total;
            let bar = c(kind, &ubo_bar(share, 26, u_ok));
            lines.push(format!(
                "  {:<32}{:>12}  {:5.1}%  {bar}",
                kind_label(kind),
                fmt_num(v),
                100.0 * share
            ));
        }
    }

    if !s.sessions.is_empty() {
        lines.extend([
            String::new(),
            c(
                "bold",
                "  2. Per-Session breakdown (System Prompt / Conv History / User Prompt / Tools & Schema)",
            ),
            format!("  {rule}"),
            format!(
                "  {:<9}{:>6}{:>7}{:>11}{:>11}{:>13}{:>14}{:>12}{:>10}{:>8}",
                "session",
                "turns",
                "calls",
                "system",
                "history",
                "user_prompt",
                "tools/schema",
                "total_in",
                "cached",
                "output"
            ),
        ]);
        for sess in &s.sessions {
            lines.push(format!(
                "  {:<9}{:>6}{:>7}{:>11}{:>11}{:>13}{:>14}{:>12}{:>10}{:>8}",
                sess.session,
                sess.turns_count,
                sess.calls_count,
                fmt_num(sess.system),
                fmt_num(sess.history),
                fmt_num(sess.user_prompt),
                fmt_num(sess.tools_schema),
                fmt_num(sess.input_total),
                fmt_num(sess.cached_total),
                fmt_num(sess.output_total),
            ));
        }
    }

    if !s.turns.is_empty() {
        lines.extend([
            String::new(),
            c(
                "bold",
                "  3. Per-Turn breakdown & prompt summary (System / History / User Prompt / Tools & Schema)",
            ),
            format!("  {rule}"),
            format!(
                "  {:<10}{:>6}{:>11}{:>11}{:>13}{:>14}{:>12}{:>10}{:>8}",
                "turn",
                "calls",
                "system",
                "history",
                "user_prompt",
                "tools/schema",
                "total_in",
                "cached",
                "output"
            ),
        ]);
        for t in &s.turns {
            let tag = format!("{}·T{}", t.session, t.turn);
            lines.push(format!(
                "  {:<10}{:>6}{:>11}{:>11}{:>13}{:>14}{:>12}{:>10}{:>8}",
                tag,
                t.calls_count,
                fmt_num(t.system),
                fmt_num(t.history),
                fmt_num(t.user_prompt),
                fmt_num(t.tools_schema),
                fmt_num(t.input_total),
                fmt_num(t.cached_total),
                fmt_num(t.output_total)
            ));
            if !t.user_text.is_empty() {
                lines.push(format!(
                    "     {} {}",
                    c("user_prompt", "User prompt:"),
                    preview_line(&t.user_text, 105)
                ));
            }
            if !t.history_text.is_empty() {
                lines.push(format!(
                    "     {} {}",
                    c("history_user", "Conv history:"),
                    preview_line(&t.history_text, 105)
                ));
            }
            if !t.response_text.is_empty() {
                lines.push(format!(
                    "     {} {}",
                    c("dim", "Assistant:"),
                    preview_line(&t.response_text, 105)
                ));
            }
        }
    }

    if !s.call_details.is_empty() {
        lines.extend([
            String::new(),
            c("bold", "  4. Per-Call breakdown"),
            format!("  {rule}"),
            format!(
                "  {:<5}{:<9}{:<23}{:>10}{:>10}{:>10}{:>11}{:>11}{:>9}{:>7}",
                "#", "turn", "model", "system", "history", "user", "tools/sch", "total_in", "cached", "out"
            ),
        ]);
        for cd in s.call_details.iter().take(40) {
            let tag = format!("{}·T{}", cd.session, cd.turn);
            let m_trunc: String = cd.model.chars().take(21).collect();
            lines.push(format!(
                "  #{:<4}{:<9}{:<23}{:>10}{:>10}{:>10}{:>11}{:>11}{:>9}{:>7}",
                cd.index,
                tag,
                m_trunc,
                fmt_num(cd.system),
                fmt_num(cd.history),
                fmt_num(cd.user_prompt),
                fmt_num(cd.tools_schema),
                fmt_num(cd.input_total),
                fmt_num(cd.cached),
                fmt_num(cd.output)
            ));
        }
    }

    let has_prompts = s.call_details.iter().any(|cd| {
        !cd.prompts.system.is_empty()
            || !cd.prompts.history.is_empty()
            || !cd.prompts.user_prompt.is_empty()
            || !cd.prompts.response.is_empty()
    });
    if has_prompts {
        let max_l = if full_prompts { 10_000 } else { 8 };
        let max_c = if full_prompts { 100_000 } else { 600 };
        lines.extend([
            String::new(),
            c("bold", "  5. Captured Prompt & Response Inspector"),
            format!("  {rule}"),
        ]);
        for cd in s.call_details.iter().take(25) {
            let p = &cd.prompts;
            if p.system.is_empty()
                && p.history.is_empty()
                && p.user_prompt.is_empty()
                && p.response.is_empty()
            {
                continue;
            }
            lines.push(c(
                "bold",
                &format!(
                    "  ▸ Call #{} ({} · Turn {} · {} · step {})",
                    cd.index, cd.session, cd.turn, cd.model, cd.step
                ),
            ));
            if !p.system.is_empty() {
                lines.push(c(
                    "system",
                    &format!("    [System Prompt · {} tokens]", fmt_num(cd.system)),
                ));
                lines.extend(indent_block(&p.system, "      ", max_l, max_c));
            }
            if !p.history.is_empty() {
                lines.push(c(
                    "history_user",
                    &format!("    [Conversation History · {} tokens]", fmt_num(cd.history)),
                ));
                lines.extend(indent_block(&p.history, "      ", max_l, max_c));
            }
            if !p.user_prompt.is_empty() {
                lines.push(c(
                    "user_prompt",
                    &format!("    [User Prompt · {} tokens]", fmt_num(cd.user_prompt)),
                ));
                lines.extend(indent_block(&p.user_prompt, "      ", max_l, max_c));
            }
            if !p.tools_schema.is_empty() && full_prompts {
                lines.push(c(
                    "tool_def",
                    &format!(
                        "    [Tools & Schema · {} tokens]",
                        fmt_num(cd.tools_schema)
                    ),
                ));
                lines.extend(indent_block(&p.tools_schema, "      ", max_l, max_c));
            }
            if !p.response.is_empty() {
                lines.push(c(
                    "dim",
                    &format!("    [Assistant Output · {} tokens]", fmt_num(cd.output)),
                ));
                lines.extend(indent_block(&p.response, "      ", max_l, max_c));
            }
        }
    }

    if !s.findings.is_empty() {
        lines.extend([
            String::new(),
            c("bold", "  Findings & recommendations"),
            format!("  {rule}"),
        ]);
        for f in &s.findings {
            let badge = c(&f.severity, &format!("[{}]", f.severity.to_ascii_uppercase()));
            lines.push(format!("  {badge} {}", f.text));
            lines.push(c("dim", &format!("         -> {}", f.see)));
        }
    }
    lines.push(String::new());
    lines.push(c(
        "dim",
        "  Note: segment counts are local estimates; 'Unattributed' is reported input minus counted segments.",
    ));
    lines.join("\n")
}

pub fn render_markdown(
    records: &[CallRecord],
    gap_s: f64,
    summary_opt: Option<&SummaryReport>,
) -> String {
    if records.is_empty() {
        return "No LLM calls recorded.".to_string();
    }
    let owned_s;
    let s = match summary_opt {
        Some(su) => su,
        None => {
            owned_s = summarize(records, gap_s);
            &owned_s
        }
    };
    let total = s.input_total.max(1) as f64;
    let mut md = vec![
        "# Token profile".to_string(),
        String::new(),
        format!("- Calls: **{}**", s.calls),
        format!(
            "- Input tokens: **{}**, output tokens: **{}**, cached input: **{}**",
            fmt_num(s.input_total),
            fmt_num(s.output_total),
            fmt_num(s.cached_total)
        ),
        format!(
            "- First call input (fixed overhead): **{}**",
            fmt_num(s.first_call_input)
        ),
        String::new(),
        "## Input token breakdown".to_string(),
        String::new(),
        "| Component | Tokens | Share |".to_string(),
        "|-----------|-------:|------:|".to_string(),
    ];
    for kind in &s.kinds_order {
        let v = s.by_kind.get(kind).copied().unwrap_or(0);
        if v != 0 {
            md.push(format!(
                "| {} | {} | {:.1}% |",
                kind_label(kind),
                fmt_num(v),
                100.0 * (v as f64) / total
            ));
        }
    }
    if s.endpoints.len() > 1
        || (s.endpoints.first().map(|e| e.class.as_str()) != Some("unknown")
            && !s.endpoints.is_empty())
    {
        md.extend([
            String::new(),
            "## Endpoints".to_string(),
            String::new(),
            "| Host | Class | API | Calls | Input tokens |".to_string(),
            "|------|-------|-----|------:|-------------:|".to_string(),
        ]);
        for e in s.endpoints.iter().take(12) {
            let who = e.host.clone().unwrap_or_else(|| {
                if e.id.is_empty() {
                    "-".to_string()
                } else {
                    format!("#{}", e.id)
                }
            });
            md.push(format!(
                "| {} | {} | {} | {} | {} |",
                who,
                e.class,
                e.api,
                e.calls,
                fmt_num(e.input)
            ));
        }
    }
    if !s.findings.is_empty() {
        md.extend([String::new(), "## Findings".to_string(), String::new()]);
        for f in &s.findings {
            md.push(format!(
                "- **{}**: {} ([guide]({}))",
                f.severity, f.text, f.see
            ));
        }
    }
    md.join("\n")
}

pub fn render_json(
    records: &[CallRecord],
    gap_s: f64,
    summary_opt: Option<&SummaryReport>,
) -> String {
    let owned_s;
    let s = match summary_opt {
        Some(su) => su,
        None => {
            owned_s = summarize(records, gap_s);
            &owned_s
        }
    };
    serde_json::to_string_pretty(s).unwrap_or_else(|_| "{}".to_string())
}

pub fn csv_quote(val: &str) -> String {
    if val.is_empty() {
        return String::new();
    }
    let bytes = val.as_bytes();
    let mut needs_clean = false;
    let mut needs_quote = false;
    for &b in bytes {
        match b {
            b'\0' | b'\r' => needs_clean = true,
            b',' | b'"' | b'\n' => needs_quote = true,
            _ => {}
        }
    }
    if !needs_clean && !needs_quote {
        return val.to_string();
    }
    let clean = if needs_clean {
        val.replace('\0', "")
            .replace("\r\n", "\n")
            .replace('\r', "\n")
    } else {
        val.to_string()
    };
    if needs_quote || clean.contains('\n') {
        if clean.contains('"') {
            format!("\"{}\"", clean.replace('"', "\"\""))
        } else {
            format!("\"{clean}\"")
        }
    } else {
        clean
    }
}

fn write_csv_cached<'a>(
    val: &'a str,
    cache: &'a mut HashMap<(u64, u64, usize), String>,
    out: &mut String,
) {
    if val.is_empty() {
        return;
    }
    if val.len() <= 64 {
        let bytes = val.as_bytes();
        if !bytes
            .iter()
            .any(|&b| matches!(b, b'\0' | b'\r' | b',' | b'"' | b'\n'))
        {
            out.push_str(val);
            return;
        }
        out.push_str(&csv_quote(val));
        return;
    }
    let (h1, h2) = fast_hash_pair(val.as_bytes());
    let key = (h1, h2, val.len());
    if let Some(q) = cache.get(&key) {
        out.push_str(q);
        return;
    }
    let q = csv_quote(val);
    out.push_str(&q);
    cache.insert(key, q);
}

pub fn render_csv(
    records: &[CallRecord],
    gap_s: f64,
    summary_opt: Option<&SummaryReport>,
) -> String {
    let header = CSV_COLUMNS.join(",");
    if records.is_empty() {
        return header;
    }
    let owned_s;
    let s = match summary_opt {
        Some(su) => su,
        None => {
            owned_s = summarize(records, gap_s);
            &owned_s
        }
    };
    let est_row_bytes = s
        .call_details
        .first()
        .map(|c| {
            c.prompts.system.len()
                + c.prompts.tools_schema.len()
                + c.prompts.user_prompt.len()
                + c.prompts.history.len()
                + c.prompts.response.len()
                + 256
        })
        .unwrap_or(512);
    let mut out = String::with_capacity(header.len() + s.call_details.len() * est_row_bytes + 4096);
    out.push_str(&header);
    let mut cache: HashMap<(u64, u64, usize), String> = HashMap::with_capacity(64);

    for sess in &s.sessions {
        out.push_str("\nsession,");
        write_csv_cached(&sess.session, &mut cache, &mut out);
        out.push_str(",,,,,");
        write_csv_cached(&sess.models.join(","), &mut cache, &mut out);
        let _ = write!(
            out,
            ",,,,{},{},{},{},{},{},{},,,{},{},,,,,{},{},,,,,,,",
            sess.turns_count,
            sess.calls_count,
            sess.input_total,
            sess.cached_total,
            sess.output_total,
            sess.system,
            sess.history,
            sess.user_prompt,
            sess.tools_schema,
            sess.other,
            sess.residual
        );
    }
    for t in &s.turns {
        out.push_str("\nturn,");
        write_csv_cached(&t.session, &mut cache, &mut out);
        let _ = write!(out, ",{},,,,", t.turn);
        write_csv_cached(&t.models.join(","), &mut cache, &mut out);
        let _ = write!(
            out,
            ",,,,1,{},{},{},{},{},{},{},{},{},{},,,,,{},{},{},,,",
            t.calls_count,
            t.input_total,
            t.cached_total,
            t.output_total,
            t.system,
            t.history,
            t.history_user,
            t.history_assistant,
            t.user_prompt,
            t.tools_schema,
            t.other,
            t.residual,
            t.latency_ms,
        );
        write_csv_cached(&t.history_text, &mut cache, &mut out);
        out.push(',');
        write_csv_cached(&t.user_text, &mut cache, &mut out);
        out.push_str(",,");
        write_csv_cached(&t.response_text, &mut cache, &mut out);
    }
    for cd in &s.call_details {
        let p = &cd.prompts;
        out.push_str("\ncall,");
        write_csv_cached(&cd.session, &mut cache, &mut out);
        let _ = write!(out, ",{},{},", cd.turn, cd.index);
        write_csv_cached(&cd.id, &mut cache, &mut out);
        let _ = write!(out, ",{},", cd.ts);
        write_csv_cached(&cd.model, &mut cache, &mut out);
        out.push(',');
        write_csv_cached(&cd.provider, &mut cache, &mut out);
        out.push(',');
        write_csv_cached(&cd.step, &mut cache, &mut out);
        out.push(',');
        write_csv_cached(cd.step_label.as_deref().unwrap_or(""), &mut cache, &mut out);
        let _ = write!(
            out,
            ",,1,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},",
            cd.input_total,
            cd.cached,
            cd.output,
            cd.system,
            cd.history,
            cd.history_user,
            cd.history_assistant,
            cd.user_prompt,
            cd.tools_schema,
            cd.tool_def,
            cd.tool_call,
            cd.tool_result,
            cd.response_schema,
            cd.other,
            cd.residual,
        );
        if let Some(lat) = cd.latency_ms {
            let _ = write!(out, "{lat}");
        }
        out.push(',');
        if let Some(st) = cd.status {
            let _ = write!(out, "{st}");
        }
        out.push(',');
        write_csv_cached(&p.system, &mut cache, &mut out);
        out.push(',');
        write_csv_cached(&p.history, &mut cache, &mut out);
        out.push(',');
        write_csv_cached(&p.user_prompt, &mut cache, &mut out);
        out.push(',');
        write_csv_cached(&p.tools_schema, &mut cache, &mut out);
        out.push(',');
        write_csv_cached(&p.response, &mut cache, &mut out);
    }
    out
}

pub fn html_escape(s: &str) -> String {
    let bytes = s.as_bytes();
    if !bytes
        .iter()
        .any(|&b| matches!(b, b'&' | b'<' | b'>' | b'"' | b'\''))
    {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    let mut last = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        let rep = match b {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'>' => "&gt;",
            b'"' => "&quot;",
            b'\'' => "&#x27;",
            _ => continue,
        };
        if i > last {
            out.push_str(&s[last..i]);
        }
        out.push_str(rep);
        last = i + 1;
    }
    if last < s.len() {
        out.push_str(&s[last..]);
    }
    out
}

pub fn render_html(
    records: &[CallRecord],
    gap_s: f64,
    prices: Option<&Map<String, Value>>,
    title: &str,
    summary_opt: Option<&SummaryReport>,
) -> String {
    let owned_s;
    let s = match summary_opt {
        Some(su) => su,
        None => {
            owned_s = summarize(records, gap_s);
            &owned_s
        }
    };
    let total = s.input_total.max(1) as f64;
    let cache_pct = if s.input_total > 0 {
        100.0 * (s.cached_total as f64) / total
    } else {
        0.0
    };
    let mut fo = s.fan_out.clone();
    fo.sort_unstable();
    let med_fo = if fo.is_empty() { 0 } else { fo[fo.len() / 2] };
    let max_fo = fo.last().copied().unwrap_or(0);
    let cost_info = prices
        .filter(|_| !records.is_empty())
        .map(|pm| estimate_cost(records, pm));

    let mut stack_divs = Vec::new();
    let mut breakdown_rows = Vec::new();
    for kind in &s.kinds_order {
        let v = s.by_kind.get(kind).copied().unwrap_or(0);
        if v == 0 {
            continue;
        }
        let pct = 100.0 * (v as f64) / total;
        let col = kind_color_hex(kind);
        let label = html_escape(kind_label(kind));
        if pct > 0.0 {
            stack_divs.push(format!(
                r#"<div class="seg" style="width:{:.2}%;background:{col}" title="{label}: {} tokens ({pct:.1}%)"></div>"#,
                pct.max(0.5),
                fmt_num(v)
            ));
        }
        breakdown_rows.push(format!(
            r#"<tr><td><span class="dot" style="background:{col}"></span>{label}</td><td class="num">{}</td><td class="num">{pct:.1}%</td><td><div class="mini-track"><div class="mini-fill" style="width:{:.1}%;background:{col}"></div></div></td></tr>"#,
            fmt_num(v),
            pct.clamp(0.0, 100.0)
        ));
    }

    let mut session_rows = Vec::new();
    for sess in &s.sessions {
        session_rows.push(format!(
            r#"<tr><td><strong>{}</strong></td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num"><strong>{}</strong></td><td class="num">{}</td><td class="num">{}</td></tr>"#,
            html_escape(&sess.session),
            fmt_usize(sess.turns_count),
            fmt_usize(sess.calls_count),
            fmt_num(sess.system),
            fmt_num(sess.history),
            fmt_num(sess.user_prompt),
            fmt_num(sess.tools_schema),
            fmt_num(sess.input_total),
            fmt_num(sess.cached_total),
            fmt_num(sess.output_total),
        ));
    }

    let max_turn_in = s
        .turns
        .iter()
        .map(|t| t.input_total.max(1))
        .max()
        .unwrap_or(1) as f64;
    let mut turn_rows = Vec::new();
    for t in &s.turns {
        let tag = format!("{} · Turn {}", t.session, t.turn);
        let mut t_bars = Vec::new();
        for (k_name, kv, col) in [
            ("System Prompt", t.system, kind_color_hex("system")),
            (
                "Conversation History",
                t.history,
                kind_color_hex("history_user"),
            ),
            ("User Prompt", t.user_prompt, kind_color_hex("user_prompt")),
            ("Tools & Schema", t.tools_schema, kind_color_hex("tool_def")),
        ] {
            if kv > 0 {
                let w = 100.0 * (kv as f64) / max_turn_in;
                t_bars.push(format!(
                    r#"<span style="width:{w:.2}%;background:{col}" title="{k_name}: {} tok"></span>"#,
                    fmt_num(kv)
                ));
            }
        }
        let mut previews = Vec::new();
        if !t.user_text.is_empty() {
            previews.push(format!(
                r#"<div><strong>User Prompt:</strong> <code>{}</code></div>"#,
                html_escape(&preview_line(&t.user_text, 140))
            ));
        }
        if !t.history_text.is_empty() {
            previews.push(format!(
                r#"<div><strong>Conv History:</strong> <code>{}</code></div>"#,
                html_escape(&preview_line(&t.history_text, 140))
            ));
        }
        if !t.response_text.is_empty() {
            previews.push(format!(
                r#"<div><strong>Assistant:</strong> <code>{}</code></div>"#,
                html_escape(&preview_line(&t.response_text, 140))
            ));
        }
        let preview_html = if previews.is_empty() {
            String::new()
        } else {
            format!(r#"<div class="turn-preview">{}</div>"#, previews.join(""))
        };
        turn_rows.push(format!(
            r#"<tr><td><strong>{}</strong></td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num"><strong>{}</strong></td><td class="num">{}</td><td class="num">{}</td><td><div class="call-bar">{}</div>{preview_html}</td></tr>"#,
            html_escape(&tag),
            fmt_usize(t.calls_count),
            fmt_num(t.system),
            fmt_num(t.history),
            fmt_num(t.user_prompt),
            fmt_num(t.tools_schema),
            fmt_num(t.input_total),
            fmt_num(t.cached_total),
            fmt_num(t.output_total),
            t_bars.join("")
        ));
    }

    let max_in = s
        .call_details
        .iter()
        .map(|cd| cd.input_total.max(1))
        .max()
        .unwrap_or(1) as f64;
    let shown_cnt = s.call_details.len().min(150);

    let mut step_rows = Vec::new();
    for st in s.steps.iter().take(20) {
        let lbl = html_escape(
            st.label
                .as_deref()
                .or(st.model.as_deref())
                .unwrap_or("-"),
        );
        step_rows.push(format!(
            r#"<tr><td><code>{}</code></td><td>{lbl}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td></tr>"#,
            html_escape(&st.step),
            fmt_usize(st.calls),
            fmt_num(st.input),
            fmt_num(st.fixed_per_call)
        ));
    }

    let mut ep_rows = Vec::new();
    for e in s.endpoints.iter().take(20) {
        let who = html_escape(&e.host.clone().unwrap_or_else(|| {
            if e.id.is_empty() {
                "-".to_string()
            } else {
                format!("#{}", e.id)
            }
        }));
        ep_rows.push(format!(
            r#"<tr><td><code>{who}</code></td><td>{}</td><td><code>{}</code></td><td class="num">{}</td><td class="num">{}</td></tr>"#,
            html_escape(&e.class),
            html_escape(&e.api),
            fmt_usize(e.calls),
            fmt_num(e.input)
        ));
    }

    let mut finding_cards = Vec::new();
    for f in &s.findings {
        let sev = html_escape(&f.severity);
        finding_cards.push(format!(
            r#"<div class="finding sev-{sev}"><span class="badge badge-{sev}">{}</span><div class="finding-body"><div>{}</div><a href="{}" target="_blank" rel="noopener">Troubleshooting guide &rarr;</a></div></div>"#,
            sev.to_ascii_uppercase(),
            html_escape(&f.text),
            html_escape(&f.see)
        ));
    }
    if finding_cards.is_empty() {
        finding_cards.push(
            r#"<div class="finding sev-info"><span class="badge badge-info">OK</span><div class="finding-body">No token-waste anomalies triggered on this run.</div></div>"#
                .to_string(),
        );
    }

    let cost_card = cost_info
        .map(|ci| {
            format!(
                r#"<div class="card"><div class="k-label">Estimated Cost</div><div class="k-val">{:.4}</div><div class="k-sub">from price table</div></div>"#,
                ci.total
            )
        })
        .unwrap_or_default();

    let title_esc = html_escape(title);
    let sess_cnt = fmt_usize(s.sessions.len());
    let turns_cnt = fmt_usize(s.turns.len());
    let calls_cnt = fmt_usize(s.calls);
    let in_tot = fmt_num(s.input_total);
    let med_in = fmt_num(s.median_input);
    let cached_tot = fmt_num(s.cached_total);
    let out_tot = fmt_num(s.output_total);
    let first_in = fmt_num(s.first_call_input);
    let stack_html = stack_divs.join("");
    let bd_html = breakdown_rows.join("");
    let sess_html = if session_rows.is_empty() {
        r#"<tr><td colspan="10" class="sub">No sessions recorded</td></tr>"#.to_string()
    } else {
        session_rows.join("")
    };
    let turn_html = if turn_rows.is_empty() {
        r#"<tr><td colspan="10" class="sub">No turns recorded</td></tr>"#.to_string()
    } else {
        turn_rows.join("")
    };
    let shown_calls = shown_cnt;
    let total_calls = s.call_details.len();
    let findings_html = finding_cards.join("");
    let steps_html = if step_rows.is_empty() {
        r#"<tr><td colspan="5" class="sub">No steps recorded</td></tr>"#.to_string()
    } else {
        step_rows.join("")
    };
    let eps_html = if ep_rows.is_empty() {
        r#"<tr><td colspan="5" class="sub">No endpoints recorded</td></tr>"#.to_string()
    } else {
        ep_rows.join("")
    };

    let est_call_bytes = s
        .call_details
        .first()
        .map(|c| {
            c.prompts.system.len()
                + c.prompts.tools_schema.len()
                + c.prompts.user_prompt.len()
                + c.prompts.history.len()
                + c.prompts.response.len()
                + 1024
        })
        .unwrap_or(1024);
    let mut doc = String::with_capacity(16_384 + shown_cnt * est_call_bytes);
    let _ = write!(
        doc,
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title_esc}</title>
<style>
:root {{
  --bg: #0f172a; --panel: #1e293b; --border: #334155; --text: #f8fafc; --muted: #94a3b8;
  --accent: #6366f1; --high: #ef4444; --medium: #f59e0b; --low: #06b6d4; --info: #3b82f6;
}}
@media (prefers-color-scheme: light) {{
  :root {{ --bg: #f8fafc; --panel: #ffffff; --border: #e2e8f0; --text: #0f172a; --muted: #64748b; }}
}}
* {{ box-sizing: border-box; }}
body {{ margin: 0; padding: 28px 20px 60px; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: var(--bg); color: var(--text); line-height: 1.5; }}
.wrap {{ max-width: 1120px; margin: 0 auto; }}
header {{ display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; gap: 12px; margin-bottom: 22px; }}
h1 {{ font-size: 1.5rem; margin: 0; letter-spacing: -0.02em; }}
h2 {{ font-size: 1.05rem; margin: 26px 0 12px; letter-spacing: -0.01em; }}
.sub {{ color: var(--muted); font-size: 0.875rem; }}
.cards {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(165px, 1fr)); gap: 12px; margin-bottom: 22px; }}
.card {{ background: var(--panel); border: 1px solid var(--border); border-radius: 10px; padding: 14px 16px; }}
.k-label {{ color: var(--muted); font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.04em; }}
.k-val {{ font-size: 1.45rem; font-weight: 700; margin-top: 4px; font-variant-numeric: tabular-nums; }}
.k-sub {{ color: var(--muted); font-size: 0.78rem; margin-top: 2px; }}
.panel {{ background: var(--panel); border: 1px solid var(--border); border-radius: 10px; padding: 18px; margin-bottom: 18px; }}
.stack-bar {{ display: flex; height: 22px; border-radius: 6px; overflow: hidden; background: var(--border); margin-bottom: 16px; }}
.stack-bar .seg {{ height: 100%; transition: opacity 0.15s; }}
.stack-bar .seg:hover {{ opacity: 0.82; }}
table {{ width: 100%; border-collapse: collapse; font-size: 0.88rem; }}
th, td {{ padding: 8px 10px; text-align: left; border-bottom: 1px solid var(--border); }}
th {{ color: var(--muted); font-weight: 600; font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.04em; }}
td.num, th.num {{ text-align: right; font-variant-numeric: tabular-nums; }}
.dot {{ display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 8px; vertical-align: middle; }}
.mini-track {{ width: 140px; height: 8px; background: var(--border); border-radius: 4px; overflow: hidden; }}
.mini-fill {{ height: 100%; border-radius: 4px; }}
.call-bar {{ display: flex; height: 12px; width: 200px; background: var(--border); border-radius: 4px; overflow: hidden; }}
.call-bar span {{ display: block; height: 100%; }}
.turn-preview {{ margin-top: 6px; font-size: 0.78rem; color: var(--muted); display: flex; flex-direction: column; gap: 2px; }}
.p-details {{ margin-top: 6px; font-size: 0.8rem; }}
.p-details summary {{ cursor: pointer; color: var(--accent); font-weight: 600; user-select: none; }}
.p-box {{ margin-top: 8px; display: flex; flex-direction: column; gap: 10px; max-width: 640px; }}
.p-sec {{ border: 1px solid var(--border); border-radius: 6px; padding: 8px 10px; background: var(--bg); }}
.p-head {{ font-size: 0.78rem; margin-bottom: 6px; }}
.p-pre {{ margin: 0; white-space: pre-wrap; word-break: break-word; max-height: 320px; overflow-y: auto; font-size: 0.78rem; line-height: 1.45; font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}
.finding {{ display: flex; align-items: flex-start; gap: 12px; padding: 12px 14px; border-radius: 8px; border: 1px solid var(--border); margin-bottom: 10px; background: var(--panel); }}
.finding-body {{ font-size: 0.9rem; }}
.finding-body a {{ color: var(--accent); text-decoration: none; font-size: 0.82rem; font-weight: 500; }}
.badge {{ font-size: 0.7rem; font-weight: 700; padding: 2px 7px; border-radius: 4px; color: #fff; flex-shrink: 0; margin-top: 2px; }}
.badge-high {{ background: var(--high); }}
.badge-medium {{ background: var(--medium); color: #111827; }}
.badge-low {{ background: var(--low); color: #111827; }}
.badge-info {{ background: var(--info); }}
code {{ font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; font-size: 0.82rem; }}
.grid-2 {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); gap: 18px; }}
footer {{ color: var(--muted); font-size: 0.8rem; margin-top: 24px; text-align: center; }}
</style>
</head>
<body>
<div class="wrap">
  <header>
    <div>
      <h1>{title_esc}</h1>
      <div class="sub">Zero-code LLM input token decomposition &mdash; system prompt, conversation history, user prompt, and tools</div>
    </div>
  </header>

  <div class="cards">
    <div class="card"><div class="k-label">Sessions &amp; Turns</div><div class="k-val">{sess_cnt} / {turns_cnt}</div><div class="k-sub">{calls_cnt} total LLM call(s)</div></div>
    <div class="card"><div class="k-label">Input Tokens</div><div class="k-val">{in_tot}</div><div class="k-sub">median {med_in} / call</div></div>
    <div class="card"><div class="k-label">Cached Input</div><div class="k-val">{cached_tot}</div><div class="k-sub">{cache_pct:.1}% cache hit rate</div></div>
    <div class="card"><div class="k-label">Output Tokens</div><div class="k-val">{out_tot}</div><div class="k-sub">generated by model</div></div>
    <div class="card"><div class="k-label">Fixed Overhead</div><div class="k-val">{first_in}</div><div class="k-sub">first call input</div></div>
    <div class="card"><div class="k-label">Fan-out</div><div class="k-val">{med_fo}</div><div class="k-sub">median calls/turn (max {max_fo})</div></div>
    {cost_card}
  </div>

  <div class="panel">
    <h2>1. Input Token Breakdown (Total)</h2>
    <div class="stack-bar">{stack_html}</div>
    <table>
      <thead><tr><th>Component</th><th class="num">Tokens</th><th class="num">Share</th><th>Distribution</th></tr></thead>
      <tbody>{bd_html}</tbody>
    </table>
  </div>

  <div class="panel">
    <h2>2. Per-Session Summary</h2>
    <table>
      <thead>
        <tr>
          <th>Session</th><th class="num">Turns</th><th class="num">Calls</th>
          <th class="num">System Prompt</th><th class="num">Conv History</th><th class="num">User Prompt</th><th class="num">Tools &amp; Schema</th>
          <th class="num">Total Input</th><th class="num">Cached</th><th class="num">Output</th>
        </tr>
      </thead>
      <tbody>{sess_html}</tbody>
    </table>
  </div>

  <div class="panel">
    <h2>3. Per-Turn Breakdown (System Prompt vs. Conversation History vs. User Prompt)</h2>
    <table>
      <thead>
        <tr>
          <th>Session &middot; Turn</th><th class="num">Calls</th>
          <th class="num">System Prompt</th><th class="num">Conv History</th><th class="num">User Prompt</th><th class="num">Tools &amp; Schema</th>
          <th class="num">Total Input</th><th class="num">Cached</th><th class="num">Output</th><th>Composition &amp; Prompt Preview</th>
        </tr>
      </thead>
      <tbody>{turn_html}</tbody>
    </table>
  </div>

  <div class="panel">
    <h2>4. Per-Call Timeline &amp; Full Prompt Inspector ({shown_calls} of {total_calls} call(s))</h2>
    <table>
      <thead>
        <tr>
          <th class="num">#</th><th>Turn</th><th>Model / Step</th>
          <th class="num">System</th><th class="num">History</th><th class="num">User</th><th class="num">Tools/Schema</th>
          <th class="num">Input</th><th class="num">Cached</th><th class="num">Output</th><th class="num">Latency</th><th>Prompts &amp; Response</th>
        </tr>
      </thead>
      <tbody>"#
    );

    if shown_cnt == 0 {
        doc.push_str(r#"<tr><td colspan="12" class="sub">No calls recorded</td></tr>"#);
    } else {
        let mut sec_cache: HashMap<(&'static str, i64, u64, u64, usize), String> =
            HashMap::with_capacity(64);
        for cd in s.call_details.iter().take(150) {
            let _ = write!(
                doc,
                r#"<tr><td class="num">#{}</td><td><code>{}·T{}</code></td><td><code>{}"#,
                cd.index,
                html_escape(&cd.session),
                cd.turn,
                html_escape(&cd.model)
            );
            if let Some(ref sl) = cd.step_label {
                if sl != &cd.model {
                    let _ = write!(doc, r#" <span class="sub">({})</span>"#, html_escape(sl));
                }
            }
            let lat = cd
                .latency_ms
                .map(|ms| format!("{} ms", fmt_num(ms)))
                .unwrap_or_else(|| "-".to_string());
            let _ = write!(
                doc,
                r#"</code></td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num">{}</td><td class="num"><strong>{}</strong></td><td class="num">{}</td><td class="num">{}</td><td class="num">{lat}</td><td><div class="call-bar">"#,
                fmt_num(cd.system),
                fmt_num(cd.history),
                fmt_num(cd.user_prompt),
                fmt_num(cd.tools_schema),
                fmt_num(cd.input_total),
                fmt_num(cd.cached),
                fmt_num(cd.output),
            );
            for (k_name, kv, col) in [
                ("System Prompt", cd.system, kind_color_hex("system")),
                (
                    "Conversation History",
                    cd.history,
                    kind_color_hex("history_user"),
                ),
                ("User Prompt", cd.user_prompt, kind_color_hex("user_prompt")),
                ("Tools & Schema", cd.tools_schema, kind_color_hex("tool_def")),
            ] {
                if kv > 0 {
                    let w = 100.0 * (kv as f64) / max_in;
                    let _ = write!(
                        doc,
                        r#"<span style="width:{w:.2}%;background:{col}" title="{k_name}: {} tok"></span>"#,
                        fmt_num(kv)
                    );
                }
            }
            doc.push_str("</div>");
            let p = &cd.prompts;
            let has_any_prompt = !p.system.is_empty()
                || !p.history.is_empty()
                || !p.user_prompt.is_empty()
                || !p.tools_schema.is_empty()
                || !p.other.is_empty()
                || !p.response.is_empty();
            if has_any_prompt {
                doc.push_str(
                    r#"<details class="p-details"><summary>View prompts &amp; response</summary><div class="p-box">"#,
                );
                for (val, label, tok_cnt, col) in [
                    (&p.system, "System Prompt", cd.system, kind_color_hex("system")),
                    (
                        &p.history,
                        "Conversation History",
                        cd.history,
                        kind_color_hex("history_user"),
                    ),
                    (
                        &p.user_prompt,
                        "Current User Prompt",
                        cd.user_prompt,
                        kind_color_hex("user_prompt"),
                    ),
                    (
                        &p.tools_schema,
                        "Tools & Response Schema",
                        cd.tools_schema,
                        kind_color_hex("tool_def"),
                    ),
                    (&p.other, "Other Context", cd.other, kind_color_hex("other")),
                    (&p.response, "Assistant Output", cd.output, "#10b981"),
                ] {
                    if !val.is_empty() {
                        let (h1, h2) = fast_hash_pair(val.as_bytes());
                        let ckey = (label, tok_cnt, h1, h2, val.len());
                        if let Some(cached_sec) = sec_cache.get(&ckey) {
                            doc.push_str(cached_sec);
                        } else {
                            let sec_html = format!(
                                r#"<div class="p-sec"><div class="p-head"><span class="dot" style="background:{col}"></span><strong>{label}</strong> <span class="sub">({} tokens)</span></div><pre class="p-pre">{}</pre></div>"#,
                                fmt_num(tok_cnt),
                                html_escape(val)
                            );
                            doc.push_str(&sec_html);
                            sec_cache.insert(ckey, sec_html);
                        }
                    }
                }
                doc.push_str("</div></details>");
            } else {
                doc.push_str(
                    r#"<span class="sub">no text stored (re-capture to inspect full prompt text)</span>"#,
                );
            }
            doc.push_str("</td></tr>");
        }
    }

    let _ = write!(
        doc,
        r#"</tbody>
    </table>
  </div>

  <div class="panel">
    <h2>Findings &amp; Recommendations</h2>
    {findings_html}
  </div>

  <div class="grid-2">
    <div class="panel">
      <h2>Per-Step Token Map</h2>
      <table>
        <thead><tr><th>Step</th><th>Label / Model</th><th class="num">Calls</th><th class="num">Input</th><th class="num">Fixed/Call</th></tr></thead>
        <tbody>{steps_html}</tbody>
      </table>
    </div>
    <div class="panel">
      <h2>Detected Endpoints</h2>
      <table>
        <thead><tr><th>Host</th><th>Class</th><th>API</th><th class="num">Calls</th><th class="num">Input</th></tr></thead>
        <tbody>{eps_html}</tbody>
      </table>
    </div>
  </div>

  <footer>Generated by <strong>tokprof</strong> &middot; Segment counts are local estimates; Unattributed is provider-reported input minus counted segments. Secrets are automatically redacted.</footer>
</div>
</body>
</html>
"#
    );
    doc
}
