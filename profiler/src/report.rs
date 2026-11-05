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
        "system" => "#245e63",
        "tool_def" => "#5b4b8a",
        "tool_call" => "#7c5295",
        "tool_result" => "#a84268",
        "history_user" => "#2b6cb0",
        "history_assistant" => "#4a89bd",
        "user_prompt" => "#296e48",
        "response_schema" => "#b76e1f",
        "raw_prompt" => "#287872",
        "attachment" => "#647078",
        "other" => "#848e95",
        "residual" => "#b8b4a9",
        _ => "#848e95",
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
            r#"<tr><td><span class="dot" style="background:{col}"></span><strong>{label}</strong></td><td class="num dp-mono">{}</td><td class="num dp-mono">{pct:.1}%</td><td><div class="mini-track"><div class="mini-fill" style="width:{:.1}%;background:{col}"></div></div></td></tr>"#,
            fmt_num(v),
            pct.clamp(0.0, 100.0)
        ));
    }

    let mut session_rows = Vec::new();
    let mut session_options = Vec::new();
    for sess in &s.sessions {
        let sess_esc = html_escape(&sess.session);
        session_options.push(format!(r#"<option value="{sess_esc}">{sess_esc}</option>"#));
        let sess_cache_pct = if sess.input_total > 0 {
            100.0 * (sess.cached_total as f64) / (sess.input_total as f64)
        } else {
            0.0
        };
        let status_cls = if sess_cache_pct >= 50.0 {
            "dp-status-ready"
        } else if sess_cache_pct >= 15.0 {
            "dp-status-waiting"
        } else {
            "dp-status-unknown"
        };
        session_rows.push(format!(
            r#"<tr><td><strong>{sess_esc}</strong> <span class="dp-status {status_cls}">{sess_cache_pct:.0}% cached</span></td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono"><strong>{}</strong></td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td></tr>"#,
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
        let preview_row = if previews.is_empty() {
            String::new()
        } else {
            format!(
                r#"<tr class="turn-preview-row"><td colspan="10"><div class="turn-preview">{}</div></td></tr>"#,
                previews.join("")
            )
        };
        turn_rows.push(format!(
            r#"<tr><td><strong>{}</strong></td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono"><strong>{}</strong></td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td><div class="call-bar">{}</div></td></tr>{preview_row}"#,
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

    // Build Edition Canvas nodes & SVG connector lines (Origin -> Turn 1 -> Turn 2 / Latest Call)
    let mut canvas_nodes = Vec::new();
    let mut canvas_svg = Vec::new();
    let sys_baseline = s.by_kind.get("system").copied().unwrap_or(0)
        + s.by_kind.get("tool_def").copied().unwrap_or(0);
    canvas_nodes.push(format!(
        r#"<button type="button" class="dp-edition-node" style="left:36px;top:128px" aria-pressed="true" onclick="showEditionArea('all');document.getElementById('edition-builder').open=true;document.getElementById('edition-builder').scrollIntoView({{behavior:'smooth'}})">
          <span class="dp-edition-node-meta">BASELINE<span>↗</span></span>
          <strong>{}</strong>
          <span class="dp-edition-node-title">System + Tools tokens</span>
          <span class="dp-status dp-status-ready">First call: {} tok</span>
        </button>"#,
        fmt_num(sys_baseline),
        fmt_num(s.first_call_input)
    ));

    let turn_sample: Vec<_> = s.turns.iter().take(2).collect();
    if turn_sample.is_empty() {
        canvas_nodes.push(
            r#"<button type="button" class="dp-edition-node" style="left:286px;top:128px" disabled>
              <span class="dp-edition-node-meta">TURN<span>—</span></span>
              <strong>0</strong>
              <span class="dp-edition-node-title">No turns recorded</span>
              <span class="dp-status dp-status-unknown">Idle</span>
            </button>"#
                .to_string(),
        );
    } else {
        for (idx, t) in turn_sample.iter().enumerate() {
            let top_y = if turn_sample.len() == 1 {
                128
            } else if idx == 0 {
                48
            } else {
                210
            };
            let start_x = 36 + 184;
            let start_y = 128 + 62;
            let end_x = 286;
            let end_y = top_y + 62;
            let mid_x = (start_x + end_x) / 2;
            let is_cur = if idx == 0 { " class=\"is-current\"" } else { "" };
            canvas_svg.push(format!(
                r#"<g{is_cur}><path d="M {start_x} {start_y} C {mid_x} {start_y}, {mid_x} {end_y}, {end_x} {end_y}"/><circle cx="{start_x}" cy="{start_y}" r="3.5"/><path d="M {} {} L {end_x} {end_y} L {} {}"/></g>"#,
                end_x - 7,
                end_y - 4,
                end_x - 7,
                end_y + 4
            ));
            let st_cls = if t.cached_total > 0 {
                "dp-status-ready"
            } else {
                "dp-status-waiting"
            };
            canvas_nodes.push(format!(
                r#"<button type="button" class="dp-edition-node" style="left:286px;top:{top_y}px" onclick="showEditionArea('turns');document.getElementById('sec-turns').scrollIntoView({{behavior:'smooth'}})">
                  <span class="dp-edition-node-meta">{} · T{}<span>↗</span></span>
                  <strong>{}</strong>
                  <span class="dp-edition-node-title">Hist {} · User {} tok</span>
                  <span class="dp-status {st_cls}">{} call(s) · Out {}</span>
                </button>"#,
                html_escape(&t.session),
                t.turn,
                fmt_num(t.input_total),
                fmt_num(t.history),
                fmt_num(t.user_prompt),
                fmt_usize(t.calls_count),
                fmt_num(t.output_total)
            ));
        }
    }

    let call_sample: Vec<_> = s.call_details.iter().take(2).collect();
    for (idx, cd) in call_sample.iter().enumerate() {
        let top_y = if call_sample.len() == 1 {
            128
        } else if idx == 0 {
            48
        } else {
            210
        };
        let parent_y = if turn_sample.len() <= 1 {
            128 + 62
        } else if idx == 0 {
            48 + 62
        } else {
            210 + 62
        };
        let start_x = 286 + 184;
        let end_x = 546;
        let end_y = top_y + 62;
        let mid_x = (start_x + end_x) / 2;
        let is_cur = if idx == 0 { " class=\"is-current\"" } else { "" };
        canvas_svg.push(format!(
            r#"<g{is_cur}><path d="M {start_x} {parent_y} C {mid_x} {parent_y}, {mid_x} {end_y}, {end_x} {end_y}"/><circle cx="{start_x}" cy="{parent_y}" r="3.5"/><path d="M {} {} L {end_x} {end_y} L {} {}"/></g>"#,
            end_x - 7,
            end_y - 4,
            end_x - 7,
            end_y + 4
        ));
        let st_cls = if cd.cached > 0 {
            "dp-status-ready"
        } else {
            "dp-status-waiting"
        };
        canvas_nodes.push(format!(
            r#"<button type="button" class="dp-edition-node" style="left:546px;top:{top_y}px" onclick="openCallDossier({})">
              <span class="dp-edition-node-meta">CALL #{}<span>↗</span></span>
              <strong>{}</strong>
              <span class="dp-edition-node-title">{}</span>
              <span class="dp-status {st_cls}">Cached {} · Out {}</span>
            </button>"#,
            cd.index,
            cd.index,
            fmt_num(cd.input_total),
            html_escape(&cd.model),
            fmt_num(cd.cached),
            fmt_num(cd.output)
        ));
    }

    let mut step_rows = Vec::new();
    for st in s.steps.iter().take(20) {
        let lbl = html_escape(
            st.label
                .as_deref()
                .or(st.model.as_deref())
                .unwrap_or("-"),
        );
        step_rows.push(format!(
            r#"<tr><td><code>{}</code></td><td>{lbl}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td></tr>"#,
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
            r#"<tr><td><code>{who}</code></td><td><span class="dp-status dp-status-unknown">{}</span></td><td><code>{}</code></td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td></tr>"#,
            html_escape(&e.class),
            html_escape(&e.api),
            fmt_usize(e.calls),
            fmt_num(e.input)
        ));
    }

    let mut finding_cards = Vec::new();
    for f in &s.findings {
        let sev = html_escape(&f.severity);
        let status_cls = match f.severity.as_str() {
            "high" => "dp-status-held",
            "medium" => "dp-status-waiting",
            "low" => "dp-status-ready",
            _ => "dp-status-unknown",
        };
        finding_cards.push(format!(
            r#"<div class="finding sev-{sev}"><span class="dp-status {status_cls} badge badge-{sev}">{}</span><div class="finding-body"><div>{}</div><a href="{}" target="_blank" rel="noopener">Troubleshooting guide &rarr;</a></div></div>"#,
            sev.to_ascii_uppercase(),
            html_escape(&f.text),
            html_escape(&f.see)
        ));
    }
    if finding_cards.is_empty() {
        finding_cards.push(
            r#"<div class="finding sev-info"><span class="dp-status dp-status-ready badge badge-info">OK</span><div class="finding-body">No token-waste anomalies triggered on this run.</div></div>"#
                .to_string(),
        );
    }

    let cost_card = cost_info
        .map(|ci| {
            format!(
                r#"<section class="dp-edition-paper card"><span class="dp-kicker k-label">07 / ESTIMATED COST</span><strong class="k-val">{:.4}</strong><span class="dp-edition-small k-sub">from configured price table</span></section>"#,
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
    let sess_opts_html = session_options.join("");
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
    let findings_cnt = s.findings.len();
    let canvas_svg_html = canvas_svg.join("");
    let canvas_nodes_html = canvas_nodes.join("");
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
                + 1200
        })
        .unwrap_or(1200);
    let mut doc = String::with_capacity(24_576 + shown_cnt * est_call_bytes);
    let _ = write!(
        doc,
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title_esc}</title>
<style>"##
    );
    let _ = write!(
        doc,
        r##"
:root {{
  --dp-bg: #f5f3ee;
  --dp-surface: #fffefa;
  --dp-ink: #202d35;
  --dp-muted: #56616a;
  --dp-rule: #cccac3;
  --dp-control: #7c858a;
  --dp-accent: #245e63;
  --dp-tint: #e5edeb;
  --dp-title: clamp(30px, 3.2vw, 44px);
  --dp-display: Georgia, "Noto Serif KR", "Times New Roman", serif;
  --cs-font: "Pretendard", -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Noto Sans KR", sans-serif;
  --cs-font-mono: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}}
"##
    );
    let _ = write!(
        doc,
        r##".dp-studio.dp-dark {{
  --dp-bg: #101c24;
  --dp-surface: #1b2b35;
  --dp-ink: #f4f2eb;
  --dp-muted: #bdc8ce;
  --dp-rule: #43535c;
  --dp-control: #8b9ba4;
  --dp-accent: #b9deda;
  --dp-tint: #30464e;
}}"##
    );
    let _ = write!(
        doc,
        r##"
*, *::before, *::after {{ box-sizing: border-box; }}
body {{ margin: 0; background: var(--dp-bg); color: var(--dp-ink); font: 14px/1.6 var(--cs-font); letter-spacing: 0; }}
.dp-studio {{ min-height: 100vh; background: var(--dp-bg); color: var(--dp-ink); }}
.dp-studio h1, .dp-studio h2, .dp-studio h3, .dp-studio p {{ margin: 0; }}"##
    );
    let _ = write!(
        doc,
        r##"
.dp-studio h1 {{ font: 400 var(--dp-title)/1.2 var(--dp-display); text-wrap: balance; }}
.dp-studio h2 {{ font: 400 24px/1.3 var(--dp-display); }}
.dp-studio h3 {{ font: 400 21px/1.3 var(--dp-display); }}
.dp-studio a {{ color: var(--dp-accent); text-underline-offset: 4px; }}
.dp-studio button, .dp-studio input, .dp-studio select {{ font: inherit; color: inherit; }}
.dp-studio button, .dp-studio select, .dp-studio input {{
  min-height: 38px; border: 1px solid var(--dp-control); border-radius: 6px;
  background: var(--dp-surface); padding: 6px 12px; max-width: 100%;
}}
.dp-studio button {{ cursor: pointer; transition: background 0.15s, border-color 0.15s, box-shadow 0.15s; }}
.dp-studio button:hover, .dp-studio button[aria-pressed="true"] {{ background: var(--dp-tint); }}
.dp-studio button[aria-pressed="true"] {{ border-color: var(--dp-accent); }}
.dp-studio :focus-visible {{ outline: 3px solid var(--dp-accent); outline-offset: 3px; }}
.dp-studio code, .dp-mono {{ font-family: var(--cs-font-mono); font-variant-numeric: tabular-nums; font-size: 12.5px; }}
.dp-masthead {{ padding: 18px 40px; display: flex; gap: 24px; align-items: center; justify-content: space-between; border-bottom: 1px solid var(--dp-rule); background: var(--dp-surface); flex-wrap: wrap; }}
.dp-brand {{ display: flex; gap: 14px; align-items: center; text-decoration: none; color: var(--dp-ink) !important; font-size: 18px; font-weight: 650; }}
.dp-brand-mark {{ width: 36px; height: 36px; border-radius: 7px; background: var(--dp-accent); color: #fffefa; display: inline-flex; align-items: center; justify-content: center; font: 400 20px/1 var(--dp-display); }}
.dp-brand small {{ display: block; font-size: 12px; font-weight: 400; color: var(--dp-muted); }}
.dp-switcher {{ display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }}
.dp-switcher a {{ padding: 7px 13px; border: 1px solid transparent; border-radius: 6px; text-decoration: none; color: var(--dp-muted); font-size: 13px; }}
.dp-switcher a:hover, .dp-switcher a[aria-current="page"] {{ border-color: var(--dp-control); color: var(--dp-ink); background: var(--dp-bg); }}
.dp-switcher a span {{ margin-right: 6px; font-family: var(--cs-font-mono); font-size: 11px; color: var(--dp-accent); }}
.dp-notice {{ padding: 11px 40px; display: flex; flex-wrap: wrap; gap: 8px 24px; border-bottom: 1px solid var(--dp-rule); color: var(--dp-muted); font-size: 13px; background: #faf8f3; }}
.dp-studio.dp-dark .dp-notice {{ background: #15242d; }}
.dp-notice strong {{ color: var(--dp-ink); font-weight: 600; letter-spacing: 0.04em; }}
.dp-page {{ padding: 28px 40px 56px; max-width: 1520px; margin: auto; }}
.dp-edition-lead {{ border-bottom: 1px solid var(--dp-rule); padding-bottom: 18px; display: flex; justify-content: space-between; align-items: flex-end; gap: 16px; flex-wrap: wrap; }}
.dp-heading p, .sub {{ color: var(--dp-muted); margin-top: 8px; font-size: 15px; }}
.dp-kicker, .k-label {{ display: block; font-size: 11px; color: var(--dp-muted); margin-bottom: 6px; font-weight: 600; letter-spacing: 0.12em; text-transform: uppercase; font-family: var(--cs-font-mono); }}
.dp-edition-choice {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 14px; margin: 22px 0; }}
.dp-edition-paper, .panel, .card {{ border: 1px solid var(--dp-rule); border-radius: 8px; padding: 18px 20px; background: var(--dp-surface); box-shadow: 0 6px 20px rgb(34 45 52 / 0.04); }}
.card .k-val {{ display: block; font: 400 30px/1.15 var(--dp-display); color: var(--dp-ink); margin-top: 4px; font-variant-numeric: tabular-nums; }}
.card .k-sub, .dp-edition-small {{ display: block; font-size: 12.5px; color: var(--dp-muted); margin-top: 6px; }}
.dp-edition-builder {{ border-bottom: 1px solid var(--dp-rule); margin-top: 6px; }}
.dp-edition-builder > summary {{ display: flex; align-items: center; gap: 14px; padding: 14px 0; list-style: none; cursor: pointer; font-weight: 600; font-size: 15px; }}
.dp-edition-builder > summary::-webkit-details-marker {{ display: none; }}
.dp-edition-builder > summary::before {{ content: "+"; color: var(--dp-accent); font: 400 22px/1 var(--dp-display); }}
.dp-edition-builder[open] > summary::before {{ content: "\2212"; }}
.dp-edition-builder-body {{ padding: 8px 0 24px; }}
.dp-edition-info-grid {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; margin-top: 14px; }}
.dp-edition-facts {{ margin: 14px 0 18px; }}
.dp-edition-facts > div, .dp-dossier > div {{ display: flex; justify-content: space-between; gap: 16px; padding: 8px 0; border-bottom: 1px solid var(--dp-rule); font-size: 13px; }}
.dp-edition-facts dt, .dp-dossier dt {{ color: var(--dp-muted); }}
.dp-edition-facts dd, .dp-dossier dd {{ margin: 0; text-align: right; font-variant-numeric: tabular-nums; font-weight: 500; }}
.dp-edition-nav {{ display: flex; flex-wrap: wrap; gap: 6px; border-bottom: 1px solid var(--dp-rule); margin-top: 18px; }}
.dp-studio .dp-edition-nav button {{ border: 0; border-bottom: 2px solid transparent; border-radius: 0; background: transparent; padding: 9px 13px; color: var(--dp-muted); font-size: 13px; font-weight: 500; white-space: nowrap; }}
.dp-studio .dp-edition-nav button[aria-pressed="true"] {{ border-color: var(--dp-accent); color: var(--dp-ink); font-weight: 600; background: transparent; }}
.dp-edition-layout {{ margin-top: 22px; }}
.dp-edition-canvas {{ background: #fcfbf7; border: 1px solid var(--dp-rule); border-radius: 8px; overflow: hidden; box-shadow: 0 16px 48px rgb(34 45 52 / 0.06); }}
.dp-studio.dp-dark .dp-edition-canvas {{ background: #16242d; }}
.dp-edition-canvas-head {{ display: flex; justify-content: space-between; align-items: flex-end; gap: 16px; padding: 16px 22px; flex-wrap: wrap; }}
.dp-edition-caption {{ color: var(--dp-muted); font-size: 12.5px; font-family: var(--cs-font-mono); }}
.dp-edition-scroll {{ max-width: 100%; overflow-x: auto; background: radial-gradient(circle at center, #dedbd2 0.75px, transparent 0.95px) 0 0 / 19px 19px, #f3f1ea; border-top: 1px solid #d6d3c9; }}
.dp-studio.dp-dark .dp-edition-scroll {{ background: radial-gradient(circle at center, #334650 0.75px, transparent 0.95px) 0 0 / 19px 19px, #132028; border-top-color: var(--dp-rule); }}
.dp-edition-map {{ position: relative; width: 790px; height: 368px; margin-inline: auto; }}
.dp-edition-map-labels {{ position: absolute; inset: 18px 0 auto; display: grid; grid-template-columns: 250px 260px 1fr; color: #647078; font: 600 11px/1.4 var(--cs-font-mono); letter-spacing: 0.1em; }}
.dp-edition-map-labels span {{ padding-left: 36px; }}
.dp-edition-lines {{ position: absolute; inset: 0; pointer-events: none; }}
.dp-edition-lines g {{ fill: none; stroke: #95a5a8; stroke-width: 1.7; stroke-linejoin: round; }}
.dp-edition-lines g.is-current {{ stroke: #245e63; stroke-width: 2.4; }}
.dp-edition-lines circle {{ fill: #f3f1ea; }}
.dp-studio button.dp-edition-node {{ position: absolute; display: flex; flex-direction: column; width: 184px; height: 124px; padding: 10px 13px; gap: 3px; text-align: left; background: #fffefa; border: 1px solid #c5c8c0; border-radius: 7px; color: #202d35; box-shadow: 0 5px 16px rgb(27 43 50 / 0.09); }}
.dp-studio.dp-dark button.dp-edition-node {{ background: #1b2b35; color: #f4f2eb; border-color: #43535c; }}
.dp-studio button.dp-edition-node:hover, .dp-studio button.dp-edition-node[aria-pressed="true"] {{ border-color: #245e63; box-shadow: 0 9px 22px rgb(27 70 73 / 0.18); }}
.dp-edition-node-meta {{ display: flex; justify-content: space-between; width: 100%; color: #496069; font: 600 10px/1.3 var(--cs-font-mono); letter-spacing: 0.07em; }}
.dp-edition-node strong {{ font: 400 26px/1.1 var(--dp-display); }}
.dp-edition-node-title {{ font-size: 11px; color: #52616a; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }}
.dp-edition-node .dp-status {{ margin-top: auto; padding: 1px 6px; font-size: 10px; }}
.dp-status {{ display: inline-block; padding: 2px 8px; border: 1px solid var(--dp-rule); border-radius: 4px; font-size: 11.5px; font-weight: 600; width: fit-content; white-space: nowrap; }}
.dp-status-ready {{ background: #e6eee7; color: #294c36; border-color: #c5d6c8; }}
.dp-status-waiting {{ background: #eee6d8; color: #62471e; border-color: #dfd0b6; }}
.dp-status-held {{ background: #f0e3e2; color: #793c3c; border-color: #e2c5c3; }}
.dp-status-unknown {{ background: #e7e9eb; color: #424d57; border-color: #cfd4d8; }}
.dp-edition-workspace {{ border-top: 1px solid var(--dp-rule); padding-top: 22px; margin-top: 28px; }}
.dp-edition-section-title {{ display: flex; justify-content: space-between; gap: 18px; align-items: baseline; margin: 28px 0 14px; flex-wrap: wrap; }}
.dp-edition-section-title:first-child {{ margin-top: 4px; }}
.dp-edition-section-title a {{ font-size: 12.5px; text-decoration: none; font-family: var(--cs-font-mono); }}
.stack-bar {{ display: flex; height: 22px; border-radius: 6px; overflow: hidden; background: #e7e4dc; border: 1px solid var(--dp-rule); margin-bottom: 14px; }}
.stack-bar .seg {{ height: 100%; transition: opacity 0.15s; }}
.stack-bar .seg:hover {{ opacity: 0.82; }}
.dp-table-wrap {{ overflow-x: auto; border: 1px solid var(--dp-rule); border-radius: 8px; background: var(--dp-surface); box-shadow: 0 4px 16px rgb(34 45 52 / 0.03); padding: 0; }}
.dp-table, table {{ width: 100%; border-collapse: collapse; text-align: left; font-size: 13px; }}
.dp-table th, table th {{ font-size: 11.5px; color: var(--dp-muted); font-weight: 600; background: var(--dp-bg); text-transform: uppercase; letter-spacing: 0.05em; font-family: var(--cs-font-mono); white-space: nowrap; }}
.dp-table td, .dp-table th, table td, table th {{ padding: 8px 9px; border-bottom: 1px solid var(--dp-rule); vertical-align: middle; }}
.dp-table tr:last-child td, table tr:last-child td {{ border-bottom: 0; }}
.dp-table tr.call-row:hover td {{ background: #faf8f2; }}
.dp-studio.dp-dark .dp-table tr.call-row:hover td {{ background: #213440; }}
td.num, th.num {{ text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }}
.call-row td:nth-child(2) {{ white-space: nowrap; }}
.call-row .step-sub {{ display: block; font-size: 11px; color: var(--dp-muted); font-family: var(--cs-font); margin-top: 1px; max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
.dot {{ display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 8px; vertical-align: middle; }}
.mini-track {{ width: 140px; height: 8px; background: #e7e4dc; border-radius: 4px; overflow: hidden; }}
.mini-fill {{ height: 100%; border-radius: 4px; }}
.call-actions {{ display: flex; align-items: center; gap: 6px; white-space: nowrap; }}
.call-bar {{ display: flex; height: 9px; width: 68px; flex-shrink: 0; background: #e7e4dc; border-radius: 4px; overflow: hidden; border: 1px solid #dcd9d0; }}
.call-bar span {{ display: block; height: 100%; }}
.dp-studio button.dp-inline-btn {{
  min-height: 25px; padding: 2px 7px; font-size: 11px; font-weight: 600;
  border: 1px solid var(--dp-rule); border-radius: 5px; background: var(--dp-bg); color: var(--dp-accent);
}}
.dp-studio button.dp-inline-btn:hover {{ background: var(--dp-tint); border-color: var(--dp-accent); }}
.turn-preview-row td {{ background: #faf8f3; padding: 7px 14px 9px; }}
.dp-studio.dp-dark .turn-preview-row td {{ background: #15242d; }}
.turn-preview {{ font-size: 12px; color: var(--dp-muted); display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 6px 18px; }}
.call-detail-row td.call-detail-cell {{ background: #faf8f3; padding: 12px 14px 14px; border-bottom: 2px solid var(--dp-rule); }}
.dp-studio.dp-dark .call-detail-row td.call-detail-cell {{ background: #15242d; }}
.p-box {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; width: 100%; align-items: start; }}
@media (max-width: 820px) {{ .p-box {{ grid-template-columns: 1fr; }} }}
.p-sec {{ border: 1px solid var(--dp-rule); border-radius: 6px; padding: 10px 12px; background: var(--dp-surface); min-width: 0; }}
.p-head {{ font-size: 12px; margin-bottom: 6px; display: flex; align-items: center; gap: 4px; border-bottom: 1px solid #eceae2; padding-bottom: 5px; }}
.dp-studio.dp-dark .p-head {{ border-bottom-color: var(--dp-rule); }}
.p-pre {{ margin: 0; white-space: pre-wrap; word-break: break-word; max-height: 260px; overflow-y: auto; font-size: 12px; line-height: 1.5; font-family: var(--cs-font-mono); color: var(--dp-ink); }}
.finding {{ display: flex; align-items: flex-start; gap: 12px; padding: 12px 14px; border-radius: 7px; border: 1px solid var(--dp-rule); margin-bottom: 10px; background: var(--dp-bg); }}
.finding-body {{ font-size: 13px; }}
.finding-body a {{ color: var(--dp-accent); font-size: 12px; font-weight: 600; display: inline-block; margin-top: 4px; }}
.dp-toolbar {{ display: flex; gap: 14px; align-items: flex-end; flex-wrap: wrap; margin-bottom: 14px; }}
.dp-toolbar label {{ display: grid; gap: 4px; font-size: 12.5px; color: var(--dp-muted); font-weight: 500; }}
.dp-toolbar input {{ width: 280px; }}
.dp-toolbar output {{ color: var(--dp-muted); font-size: 12.5px; padding: 8px 0; font-family: var(--cs-font-mono); }}
.dp-studio button.dp-edition-version {{
  min-height: 28px; padding: 2px 6px; border: 0; border-radius: 4px;
  background: transparent; color: var(--dp-accent); font: 600 13px/1.4 var(--cs-font-mono);
  font-variant-numeric: tabular-nums; text-decoration: underline; text-underline-offset: 3px;
}}
.dp-studio button.dp-edition-version:hover {{ background: var(--dp-tint); }}
.dp-edition-backdrop {{
  display: none; position: fixed; inset: 0; z-index: 999; background: rgb(20 37 44 / 0.34);
}}
.dp-edition-backdrop.is-open {{ display: block; }}
.dp-studio .dp-edition-drawer {{
  position: fixed; top: 0; right: 0; bottom: 0; margin: 0; padding: 0; width: min(580px, 100vw); max-width: 100vw;
  height: 100vh; border: 0; border-left: 1px solid var(--dp-rule);
  background: var(--dp-surface); color: var(--dp-ink); overflow-y: auto;
  box-shadow: -16px 0 48px rgb(20 37 44 / 0.18);
}}
.dp-edition-drawer-head {{
  position: sticky; top: 0; z-index: 2; background: var(--dp-surface);
  border-bottom: 1px solid var(--dp-rule); padding: 14px 24px;
  display: flex; align-items: center; justify-content: space-between; gap: 16px;
}}
.dp-edition-drawer-head span {{ font-weight: 600; font-size: 14px; }}
.dp-edition-dossier {{ padding: 24px; }}
.dp-edition-dossier h2 {{ font: 400 26px/1.3 var(--dp-display); margin: 8px 0 12px; }}
.dp-provenance {{ border-top: 1px solid var(--dp-rule); margin-top: 28px; padding-top: 8px; color: var(--dp-muted); font-size: 13px; }}
.dp-provenance summary {{ cursor: pointer; font-weight: 600; padding: 8px 0; }}
.dp-footer {{ display: flex; justify-content: space-between; flex-wrap: wrap; gap: 12px; margin: 0 40px; padding: 22px 0 36px; border-top: 1px solid var(--dp-rule); color: var(--dp-muted); font-size: 12.5px; }}
@media (max-width: 980px) {{
  .dp-masthead, .dp-notice, .dp-page {{ padding-inline: 20px; }}
  .dp-footer {{ margin-inline: 20px; }}
  .dp-edition-info-grid {{ grid-template-columns: 1fr; }}
}}
</style>
</head>
<body>
<div class="dp-studio dp-edition" id="tokprof-studio">
  <header class="dp-masthead">
    <a class="dp-brand" href="#preview-content">
      <span class="dp-brand-mark">T</span>
      <span>tokprof <small>Edition Dossier &middot; LLM Tokenomics</small></span>
    </a>
    <nav class="dp-switcher" aria-label="Quick section navigation">
      <a href="#edition-builder" aria-current="page"><span>01</span> Composition</a>
      <a href="#sec-sessions"><span>02</span> Sessions</a>
      <a href="#sec-turns"><span>03</span> Turns</a>
      <a href="#sec-calls"><span>04</span> Call Inspector</a>
      <a href="#sec-topology"><span>05</span> Topology &amp; Findings</a>
    </nav>
    <button type="button" onclick="document.getElementById('tokprof-studio').classList.toggle('dp-dark')" title="Toggle Edition Light / Dark paper theme">Paper / Night</button>
  </header>

  <div class="dp-notice">
    <strong>ZERO-CODE TELEMETRY &middot; EDITION REPORT</strong>
    <span>Sessions: {sess_cnt} &middot; Turns: {turns_cnt} &middot; Calls: {calls_cnt} &middot; Total Input: {in_tot} tok &middot; Cached: {cached_tot} ({cache_pct:.1}%) &middot; Output: {out_tot} tok</span>
  </div>

  <main id="preview-content" tabindex="-1">
    <div class="dp-page dp-edition">
      <div class="dp-edition-lead">
        <header class="dp-heading">
          <span class="dp-kicker">TOKPROF / EDITION DOSSIER</span>
          <h1>{title_esc}</h1>
          <p class="sub">Zero-code LLM input token decomposition &mdash; system prompt, conversation history, user prompt, and tools, carefully composed.</p>
        </header>
        <span class="dp-status dp-status-ready">{calls_cnt} calls profiled &middot; {cache_pct:.1}% cache hit</span>
      </div>

      <div class="dp-edition-choice cards" role="group" aria-label="Key token metrics">
        <section class="dp-edition-paper card"><span class="dp-kicker k-label">01 / SESSIONS &amp; TURNS</span><strong class="k-val">{sess_cnt} / {turns_cnt}</strong><span class="dp-edition-small k-sub">{calls_cnt} total LLM call(s)</span></section>
        <section class="dp-edition-paper card"><span class="dp-kicker k-label">02 / INPUT TOKENS</span><strong class="k-val">{in_tot}</strong><span class="dp-edition-small k-sub">median {med_in} / call</span></section>
        <section class="dp-edition-paper card"><span class="dp-kicker k-label">03 / CACHED INPUT</span><strong class="k-val">{cached_tot}</strong><span class="dp-edition-small k-sub">{cache_pct:.1}% cache hit rate</span></section>
        <section class="dp-edition-paper card"><span class="dp-kicker k-label">04 / OUTPUT TOKENS</span><strong class="k-val">{out_tot}</strong><span class="dp-edition-small k-sub">generated by model</span></section>
        <section class="dp-edition-paper card"><span class="dp-kicker k-label">05 / FIXED OVERHEAD</span><strong class="k-val">{first_in}</strong><span class="dp-edition-small k-sub">first call input</span></section>
        <section class="dp-edition-paper card"><span class="dp-kicker k-label">06 / FAN-OUT</span><strong class="k-val">{med_fo}</strong><span class="dp-edition-small k-sub">median calls/turn (max {max_fo})</span></section>
        {cost_card}
      </div>

      <details class="dp-edition-builder" id="edition-builder" open>
        <summary><span>1. Input Token Breakdown (Total) &amp; Diagnostic Dossier</span></summary>
        <div class="dp-edition-builder-body">
          <div class="stack-bar">{stack_html}</div>
          <div class="dp-edition-info-grid">
            <section class="dp-edition-paper panel">
              <span class="dp-kicker">01 / COMPOSITION</span>
              <h3>1. Input Token Breakdown (Total)</h3>
              <div class="dp-table-wrap" style="margin-top:12px">
                <table class="dp-table">
                  <thead><tr><th>Component</th><th class="num">Tokens</th><th class="num">Share</th><th>Distribution</th></tr></thead>
                  <tbody>{bd_html}</tbody>
                </table>
              </div>
            </section>
            <section class="dp-edition-paper panel">
              <span class="dp-kicker">DIAGNOSTIC DOSSIER / FACTS</span>
              <h3>Findings &amp; Recommendations ({findings_cnt})</h3>
              <dl class="dp-edition-facts">
                <div><dt>First-Call Fixed Overhead</dt><dd class="dp-mono">{first_in} tok</dd></div>
                <div><dt>Median Input per Call</dt><dd class="dp-mono">{med_in} tok</dd></div>
                <div><dt>Prompt Cache Efficiency</dt><dd class="dp-mono">{cache_pct:.1}% ({cached_tot} / {in_tot})</dd></div>
                <div><dt>Turn Fan-out (Median / Max)</dt><dd class="dp-mono">{med_fo} / {max_fo} calls</dd></div>
              </dl>
              {findings_html}
            </section>
          </div>
        </div>
      </details>

      <nav class="dp-edition-nav" id="edition-navigation" aria-label="Edition workspace views">
        <button type="button" data-area-btn="all" aria-pressed="true" onclick="showEditionArea('all')">All Dossiers</button>
        <button type="button" data-area-btn="sessions" aria-pressed="false" onclick="showEditionArea('sessions')">2. Per-Session Summary ({sess_cnt})</button>
        <button type="button" data-area-btn="turns" aria-pressed="false" onclick="showEditionArea('turns')">3. Per-Turn Breakdown ({turns_cnt})</button>
        <button type="button" data-area-btn="calls" aria-pressed="false" onclick="showEditionArea('calls')">4. Call Inspector ({shown_calls})</button>
        <button type="button" data-area-btn="topology" aria-pressed="false" onclick="showEditionArea('topology')">5. Steps &amp; Endpoints</button>
      </nav>

      <div class="dp-edition-layout">
        <section class="dp-edition-canvas" aria-label="Token flow lineage canvas">
          <div class="dp-edition-canvas-head">
            <div>
              <span class="dp-kicker">EDITION LINEAGE / TOKEN FLOW</span>
              <h2>Baseline &rarr; Turn Progression &rarr; Call Execution</h2>
            </div>
            <span class="dp-edition-caption" role="status">{sess_cnt} session(s) &middot; {turns_cnt} turn(s) &middot; select a call node to open its dossier</span>
          </div>
          <div class="dp-edition-scroll" role="region" aria-label="Token lineage map" tabindex="0">
            <div class="dp-edition-map">
              <div class="dp-edition-map-labels" aria-hidden="true"><span>ORIGIN / 01</span><span>TURNS / 02</span><span>EXECUTION / 03</span></div>
              <svg class="dp-edition-lines" width="790" height="368" viewBox="0 0 790 368" aria-hidden="true">{canvas_svg_html}</svg>
              {canvas_nodes_html}
            </div>
          </div>
        </section>
      </div>

      <div id="call-dossier-backdrop" class="dp-edition-backdrop" onclick="if(event.target===this)closeCallDossier()">
        <aside id="call-dossier-drawer" class="dp-edition-drawer" role="dialog" aria-modal="true" aria-labelledby="edition-drawer-title">
          <header class="dp-edition-drawer-head">
            <span id="edition-drawer-title">Call Dossier &middot; Prompt Inspector</span>
            <button class="dp-edition-drawer-close" type="button" onclick="closeCallDossier()">Close</button>
          </header>
          <section class="dp-edition-dossier" id="call-dossier-body"></section>
        </aside>
      </div>

      <section class="dp-section dp-edition-workspace" id="edition-workspace" tabindex="-1">
        <div data-area-sec="sessions" id="sec-sessions">
          <header class="dp-edition-section-title">
            <div><span class="dp-kicker">02 / SESSION DOSSIER</span><h2>2. Per-Session Summary</h2></div>
            <a href="#edition-navigation">&uarr; Navigation</a>
          </header>
          <div class="dp-table-wrap panel">
            <table class="dp-table">
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
        </div>

        <div data-area-sec="turns" id="sec-turns">
          <header class="dp-edition-section-title">
            <div><span class="dp-kicker">03 / TURN LINEAGE</span><h2>3. Per-Turn Breakdown (System Prompt vs. Conversation History vs. User Prompt)</h2></div>
            <a href="#edition-navigation">&uarr; Navigation</a>
          </header>
          <div class="dp-table-wrap panel">
            <table class="dp-table">
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
        </div>

        <div data-area-sec="calls" id="sec-calls">
          <header class="dp-edition-section-title">
            <div><span class="dp-kicker">04 / CALL INSPECTOR</span><h2>4. Per-Call Timeline &amp; Full Prompt Inspector ({shown_calls} of {total_calls} call(s))</h2></div>
            <a href="#edition-navigation">&uarr; Navigation</a>
          </header>
          <div class="dp-toolbar">
            <label>Filter Calls<input type="search" id="call-search" oninput="filterCalls()" placeholder="Model, turn, or prompt text..." /></label>
            <label>Session<select id="call-session-filter" onchange="filterCalls()"><option value="all">All sessions</option>{sess_opts_html}</select></label>
            <output id="call-filter-count" aria-live="polite">{shown_calls} call(s) shown &middot; click #call to open Dossier Drawer</output>
          </div>
          <div class="dp-table-wrap panel">
            <table class="dp-table dp-edition-release-table" id="calls-table">
              <thead>
                <tr>
                  <th class="num">#</th><th>Turn</th><th>Model / Step</th>
                  <th class="num">System</th><th class="num">History</th><th class="num">User</th><th class="num">Tools</th>
                  <th class="num">Input</th><th class="num">Cached</th><th class="num">Out</th><th class="num">Time</th><th>Prompts &amp; Response</th>
                </tr>
              </thead>
              <tbody>"##
    );

    if shown_cnt == 0 {
        doc.push_str(r##"<tr><td colspan="12" class="sub">No calls recorded</td></tr>"##);
    } else {
        let mut sec_cache: HashMap<(&'static str, i64, u64, u64, usize), String> =
            HashMap::with_capacity(64);
        for cd in s.call_details.iter().take(150) {
            let sess_esc = html_escape(&cd.session);
            let model_esc = html_escape(&cd.model);
            let lat = cd
                .latency_ms
                .map(|ms| format!("{} ms", fmt_num(ms)))
                .unwrap_or_else(|| "-".to_string());
            let _ = write!(
                doc,
                r##"<tr class="call-row" data-call-idx="{idx}" data-session="{sess_esc}" data-turn="{sess_esc}·T{turn}" data-model="{model_esc}" data-input="{in_t}" data-cached="{ca_t}" data-output="{out_t}" data-system="{sys_t}" data-history="{his_t}" data-user="{usr_t}" data-tools="{tls_t}" data-latency="{lat}"><td class="num"><button type="button" class="dp-edition-version" onclick="openCallDossier({idx})" title="Open Call #{idx} in Edition Dossier Drawer">#{idx}</button></td><td><code>{sess_esc}·T{turn}</code></td><td><code>{model_esc}</code>"##,
                idx = cd.index,
                turn = cd.turn,
                in_t = fmt_num(cd.input_total),
                ca_t = fmt_num(cd.cached),
                out_t = fmt_num(cd.output),
                sys_t = fmt_num(cd.system),
                his_t = fmt_num(cd.history),
                usr_t = fmt_num(cd.user_prompt),
                tls_t = fmt_num(cd.tools_schema),
            );
            if let Some(ref sl) = cd.step_label {
                if sl != &cd.model {
                    let _ = write!(
                        doc,
                        r##"<span class="step-sub" title="{0}">{0}</span>"##,
                        html_escape(sl)
                    );
                }
            }
            let _ = write!(
                doc,
                r##"</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono"><strong>{}</strong></td><td class="num dp-mono">{}</td><td class="num dp-mono">{}</td><td class="num dp-mono">{lat}</td><td><div class="call-actions"><div class="call-bar">"##,
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
                        r##"<span style="width:{w:.2}%;background:{col}" title="{k_name}: {} tok"></span>"##,
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
                let btn_label = if cd.index == 1 {
                    "&#9662; Prompts"
                } else {
                    "&#9656; Prompts"
                };
                let _ = write!(
                    doc,
                    r##"<button type="button" class="dp-inline-btn" id="call-toggle-btn-{idx}" onclick="toggleCallDetail({idx})">{btn_label}</button><button type="button" class="dp-inline-btn" onclick="openCallDossier({idx})" title="Open in right-side Dossier Drawer">Drawer &nearr;</button></div></td></tr>"##,
                    idx = cd.index
                );
                let disp = if cd.index == 1 { "" } else { r#" style="display:none""# };
                let _ = write!(
                    doc,
                    r##"<tr class="call-detail-row" id="call-detail-row-{idx}" data-parent-idx="{idx}"{disp}><td colspan="12" class="call-detail-cell"><div class="p-box" id="call-prompts-{idx}">"##,
                    idx = cd.index
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
                    (&p.response, "Assistant Output", cd.output, "#296e48"),
                ] {
                    if !val.is_empty() {
                        let (h1, h2) = fast_hash_pair(val.as_bytes());
                        let ckey = (label, tok_cnt, h1, h2, val.len());
                        if let Some(cached_sec) = sec_cache.get(&ckey) {
                            doc.push_str(cached_sec);
                        } else {
                            let sec_html = format!(
                                r##"<div class="p-sec"><div class="p-head"><span class="dot" style="background:{col}"></span><strong>{label}</strong> <span class="sub">({} tokens)</span></div><pre class="p-pre">{}</pre></div>"##,
                                fmt_num(tok_cnt),
                                html_escape(val)
                            );
                            doc.push_str(&sec_html);
                            sec_cache.insert(ckey, sec_html);
                        }
                    }
                }
                doc.push_str("</div></td></tr>");
            } else {
                doc.push_str(
                    r##"<span class="sub">no text stored (re-capture to inspect full prompt text)</span></div></td></tr>"##,
                );
            }
        }
    }

    let _ = write!(
        doc,
        r##"</tbody>
            </table>
          </div>
        </div>

        <div data-area-sec="topology" id="sec-topology">
          <header class="dp-edition-section-title">
            <div><span class="dp-kicker">05 / TOPOLOGY &amp; ENDPOINTS</span><h2>Per-Step Token Map &amp; Detected Endpoints</h2></div>
            <a href="#edition-navigation">&uarr; Navigation</a>
          </header>
          <div class="dp-edition-info-grid">
            <section class="dp-edition-paper panel">
              <span class="dp-kicker">STEP SIGNATURES</span>
              <h3>Per-Step Token Map</h3>
              <div class="dp-table-wrap" style="margin-top:12px">
                <table class="dp-table">
                  <thead><tr><th>Step</th><th>Label / Model</th><th class="num">Calls</th><th class="num">Input</th><th class="num">Fixed/Call</th></tr></thead>
                  <tbody>{steps_html}</tbody>
                </table>
              </div>
            </section>
            <section class="dp-edition-paper panel">
              <span class="dp-kicker">ENDPOINT DISCOVERY</span>
              <h3>Detected Endpoints</h3>
              <div class="dp-table-wrap" style="margin-top:12px">
                <table class="dp-table">
                  <thead><tr><th>Host</th><th>Class</th><th>API</th><th class="num">Calls</th><th class="num">Input</th></tr></thead>
                  <tbody>{eps_html}</tbody>
                </table>
              </div>
            </section>
          </div>
        </div>

        <details class="dp-provenance">
          <summary>Provenance &amp; Measurement Methodology</summary>
          <p>Segment token counts are local tokenizer estimates (tiktoken BPE / heuristic); <strong>Unattributed</strong> is provider-reported input tokens minus counted segments (message framing, images, or hidden tokens). HTTP Authorization headers and API keys are never stored, and secrets matching token patterns are automatically redacted before writing.</p>
        </details>
      </section>
    </div>
  </main>

  <footer class="dp-footer">
    <span><strong>tokprof</strong> / Edition Diagnostic Dossier</span>
    <span>Zero-code LLM Token Profiler &middot; Local self-contained HTML artifact</span>
  </footer>
</div>
<script>
function showEditionArea(area) {{
  document.querySelectorAll('[data-area-btn]').forEach(function(btn) {{
    btn.setAttribute('aria-pressed', btn.getAttribute('data-area-btn') === area ? 'true' : 'false');
  }});
  document.querySelectorAll('[data-area-sec]').forEach(function(sec) {{
    sec.style.display = (area === 'all' || sec.getAttribute('data-area-sec') === area) ? '' : 'none';
  }});
}}
function toggleCallDetail(idx) {{
  var drow = document.getElementById('call-detail-row-' + idx);
  var btn = document.getElementById('call-toggle-btn-' + idx);
  if (!drow) return;
  var hidden = drow.style.display === 'none';
  drow.style.display = hidden ? '' : 'none';
  if (btn) btn.innerHTML = hidden ? '&#9662; Prompts' : '&#9656; Prompts';
}}
function filterCalls() {{
  var q = (document.getElementById('call-search').value || '').toLowerCase().trim();
  var sess = document.getElementById('call-session-filter').value;
  var rows = document.querySelectorAll('#calls-table tbody tr.call-row');
  var shown = 0;
  rows.forEach(function(r) {{
    var idx = r.getAttribute('data-call-idx');
    var drow = document.getElementById('call-detail-row-' + idx);
    var dtext = drow ? drow.textContent.toLowerCase() : '';
    var okSess = (sess === 'all' || r.getAttribute('data-session') === sess);
    var okQ = (!q || r.textContent.toLowerCase().indexOf(q) !== -1 || dtext.indexOf(q) !== -1);
    var vis = okSess && okQ;
    r.style.display = vis ? '' : 'none';
    if (drow && !vis) drow.style.display = 'none';
    if (vis) shown++;
  }});
  var out = document.getElementById('call-filter-count');
  if (out) out.textContent = shown + ' call(s) shown · click ▾ View prompts or #call Drawer';
}}
function openCallDossier(idx) {{
  var row = document.querySelector('tr.call-row[data-call-idx="' + idx + '"]');
  var backdrop = document.getElementById('call-dossier-backdrop');
  var body = document.getElementById('call-dossier-body');
  if (!row || !backdrop || !body) return;
  var promptsEl = document.getElementById('call-prompts-' + idx);
  var promptsHtml = promptsEl ? promptsEl.innerHTML : '<p class="sub">No prompt text stored in this JSONL record (captured before prompt-text storage was enabled or with --no-content).</p>';
  body.innerHTML =
    '<div class="dp-kicker">CALL DOSSIER / #' + idx + '</div>' +
    '<span class="dp-status dp-status-ready">' + row.getAttribute('data-turn') + '</span>' +
    '<h2>Call #' + idx + ' &middot; ' + row.getAttribute('data-model') + '</h2>' +
    '<dl class="dp-dossier">' +
      '<div><dt>Session &middot; Turn</dt><dd class="dp-mono">' + row.getAttribute('data-turn') + '</dd></div>' +
      '<div><dt>Total Input / Cached / Output</dt><dd class="dp-mono">' + row.getAttribute('data-input') + ' / ' + row.getAttribute('data-cached') + ' / ' + row.getAttribute('data-output') + ' tok</dd></div>' +
      '<div><dt>System / History / User / Tools</dt><dd class="dp-mono">' + row.getAttribute('data-system') + ' / ' + row.getAttribute('data-history') + ' / ' + row.getAttribute('data-user') + ' / ' + row.getAttribute('data-tools') + ' tok</dd></div>' +
      '<div><dt>Latency</dt><dd class="dp-mono">' + row.getAttribute('data-latency') + '</dd></div>' +
    '</dl>' +
    '<div class="p-box" style="grid-template-columns:1fr">' + promptsHtml + '</div>';
  backdrop.classList.add('is-open');
}}
function closeCallDossier() {{
  var backdrop = document.getElementById('call-dossier-backdrop');
  if (backdrop) backdrop.classList.remove('is-open');
}}
document.addEventListener('keydown', function(e) {{ if (e.key === 'Escape') closeCallDossier(); }});
</script>
</body>
</html>
"##
    );
    doc
}
