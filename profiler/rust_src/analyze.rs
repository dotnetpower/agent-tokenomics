use crate::model::{CallRecord, KINDS};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const DOCS_BASE: &str = "https://dotnetpower.github.io/agent-tokenomics";

pub fn doc_link(stage: &str) -> String {
    match stage {
        "concepts" => format!("{DOCS_BASE}/agent-tokenomics/00-concepts/"),
        "diagnose" => format!("{DOCS_BASE}/agent-tokenomics/01-diagnose/"),
        "isolate" => format!("{DOCS_BASE}/agent-tokenomics/02-isolate/"),
        "improve" => format!("{DOCS_BASE}/agent-tokenomics/04-improve/"),
        _ => DOCS_BASE.to_string(),
    }
}

pub fn rec_input(r: &CallRecord) -> i64 {
    r.usage
        .get("input_total")
        .and_then(|v| v.as_i64())
        .unwrap_or(r.est_input)
}

pub fn rec_cached(r: &CallRecord) -> i64 {
    r.usage
        .get("cached")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

pub fn rec_output(r: &CallRecord) -> i64 {
    r.usage
        .get("output")
        .and_then(|v| v.as_i64())
        .or(r.output_est)
        .unwrap_or(0)
}

pub fn median_i64(vals: &[i64]) -> i64 {
    if vals.is_empty() {
        return 0;
    }
    let mut s = vals.to_vec();
    s.sort_unstable();
    let mid = s.len() / 2;
    if s.len() % 2 == 1 {
        s[mid]
    } else {
        (s[mid - 1] + s[mid]) / 2
    }
}

pub fn median_f64(vals: &[usize]) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    let mut s = vals.to_vec();
    s.sort_unstable();
    let mid = s.len() / 2;
    if s.len() % 2 == 1 {
        s[mid] as f64
    } else {
        (s[mid - 1] as f64 + s[mid] as f64) / 2.0
    }
}

pub fn is_sorted_by_ts(records: &[CallRecord]) -> bool {
    if records.len() <= 1 {
        return true;
    }
    let mut prev = if records[0].ts.is_finite() {
        records[0].ts
    } else {
        0.0
    };
    for r in &records[1..] {
        let cur = if r.ts.is_finite() { r.ts } else { 0.0 };
        if cur < prev {
            return false;
        }
        prev = cur;
    }
    true
}

pub fn sorted_records(records: &[CallRecord]) -> Vec<CallRecord> {
    let mut out = records.to_vec();
    if !is_sorted_by_ts(&out) {
        out.sort_by(|a, b| {
            let ta = if a.ts.is_finite() { a.ts } else { 0.0 };
            let tb = if b.ts.is_finite() { b.ts } else { 0.0 };
            ta.total_cmp(&tb)
        });
    }
    out
}

pub fn group_requests_ref<'a>(ordered: &'a [CallRecord], gap_s: f64) -> Vec<Vec<&'a CallRecord>> {
    let mut groups: Vec<Vec<&'a CallRecord>> = Vec::new();
    let mut last_end: Option<f64> = None;
    let mut seen_step: HashMap<(&str, &str), Option<&str>> = HashMap::new();

    for r in ordered {
        let start = if r.ts.is_finite() { r.ts } else { 0.0 };
        let end = start + (r.latency_ms.unwrap_or(0).max(0) as f64) / 1000.0;
        let step_key = (
            r.sysfp.as_deref().unwrap_or(""),
            r.model.as_deref().unwrap_or(""),
        );
        let conv_ref = r.conv.as_deref();
        let mut new_turn = false;
        if let Some(le) = last_end {
            if start - le > gap_s {
                new_turn = true;
            } else if r.sysfp.is_some() {
                if let Some(&prev_conv) = seen_step.get(&step_key) {
                    if prev_conv != conv_ref && (start - le) >= 1.0 {
                        new_turn = true;
                    }
                }
            }
        } else {
            new_turn = true;
        }
        if new_turn {
            groups.push(Vec::new());
            seen_step.clear();
        }
        if r.sysfp.is_some() {
            seen_step.insert(step_key, conv_ref);
        }
        if let Some(last_g) = groups.last_mut() {
            last_g.push(r);
        }
        last_end = Some(last_end.map_or(end, |le| le.max(end)));
    }
    groups
}

pub fn group_requests(records: &[CallRecord], gap_s: f64) -> Vec<Vec<CallRecord>> {
    let ordered = sorted_records(records);
    group_requests_ref(&ordered, gap_s)
        .into_iter()
        .map(|g| g.into_iter().cloned().collect())
        .collect()
}

pub fn infer_history_deltas(turns: &[Vec<&CallRecord>]) -> HashMap<(usize, usize), i64> {
    let has_explicit = turns.iter().any(|g| {
        g.iter().any(|r| {
            r.segments.iter().any(|s| {
                (s.kind == "history_user" || s.kind == "history_assistant") && s.tokens > 0
            })
        })
    });
    if has_explicit || turns.len() < 2 {
        return HashMap::new();
    }
    let mut base_user: HashMap<(&str, &str), i64> = HashMap::new();
    for r in &turns[0] {
        if let Some(ref sfp) = r.sysfp {
            let sk = (sfp.as_str(), r.model.as_deref().unwrap_or(""));
            base_user.entry(sk).or_insert_with(|| {
                r.segments
                    .iter()
                    .filter(|s| s.kind == "user_prompt" || s.kind == "raw_prompt")
                    .map(|s| s.tokens)
                    .sum()
            });
        }
    }
    if base_user.is_empty() {
        return HashMap::new();
    }
    let mut inferred = HashMap::new();
    for (ti, g) in turns.iter().enumerate().skip(1) {
        let mut deltas = Vec::new();
        let mut call_deltas = Vec::new();
        for (ci, r) in g.iter().enumerate() {
            let sk = (
                r.sysfp.as_deref().unwrap_or(""),
                r.model.as_deref().unwrap_or(""),
            );
            let u_tok: i64 = r
                .segments
                .iter()
                .filter(|s| s.kind == "user_prompt" || s.kind == "raw_prompt")
                .map(|s| s.tokens)
                .sum();
            if let Some(&b_tok) = base_user.get(&sk) {
                if u_tok > b_tok {
                    let d = u_tok - b_tok;
                    call_deltas.push((ci, d, u_tok));
                    if d >= 10 {
                        deltas.push(d);
                    }
                }
            }
        }
        if !deltas.is_empty() {
            let common = median_i64(&deltas);
            for (ci, d, u_tok) in call_deltas {
                if d >= 10 {
                    let est = d
                        .min(common.max((d as f64 * 0.85) as i64))
                        .min((u_tok - 1).max(0));
                    inferred.insert((ti, ci), est);
                }
            }
        }
    }
    inferred
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRow {
    pub step: String,
    pub label: Option<String>,
    pub model: Option<String>,
    pub calls: usize,
    pub input: i64,
    pub fixed_per_call: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointRow {
    pub class: String,
    pub id: String,
    pub host: Option<String>,
    pub api: String,
    pub calls: usize,
    pub input: i64,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub severity: String,
    pub text: String,
    pub see: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CallPrompts {
    pub system: String,
    pub history: String,
    pub user_prompt: String,
    pub tools_schema: String,
    pub other: String,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallDetail {
    pub index: usize,
    pub id: String,
    pub session: String,
    pub turn: usize,
    pub ts: f64,
    pub model: String,
    pub provider: String,
    pub step: String,
    pub step_label: Option<String>,
    pub system: i64,
    pub history: i64,
    pub history_user: i64,
    pub history_assistant: i64,
    pub user_prompt: i64,
    pub tools_schema: i64,
    pub tool_def: i64,
    pub tool_call: i64,
    pub tool_result: i64,
    pub response_schema: i64,
    pub other: i64,
    pub residual: i64,
    pub input_total: i64,
    pub cached: i64,
    pub output: i64,
    pub latency_ms: Option<i64>,
    pub status: Option<i64>,
    pub inferred_history: bool,
    pub prompts: CallPrompts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSummary {
    pub session: String,
    pub turn: usize,
    pub calls_count: usize,
    pub models: Vec<String>,
    pub system: i64,
    pub history: i64,
    pub history_user: i64,
    pub history_assistant: i64,
    pub user_prompt: i64,
    pub tools_schema: i64,
    pub other: i64,
    pub residual: i64,
    pub input_total: i64,
    pub cached_total: i64,
    pub output_total: i64,
    pub latency_ms: i64,
    pub user_text: String,
    pub history_text: String,
    pub response_text: String,
    pub calls: Vec<CallDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session: String,
    pub turns_count: usize,
    pub calls_count: usize,
    pub models: Vec<String>,
    pub system: i64,
    pub history: i64,
    pub user_prompt: i64,
    pub tools_schema: i64,
    pub other: i64,
    pub residual: i64,
    pub input_total: i64,
    pub cached_total: i64,
    pub output_total: i64,
    pub turns: Vec<TurnSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryReport {
    pub calls: usize,
    pub input_total: i64,
    pub output_total: i64,
    pub cached_total: i64,
    pub by_kind: BTreeMap<String, i64>,
    pub steps: Vec<StepRow>,
    pub endpoints: Vec<EndpointRow>,
    pub sessions: Vec<SessionSummary>,
    pub turns: Vec<TurnSummary>,
    pub call_details: Vec<CallDetail>,
    pub fan_out: Vec<usize>,
    pub first_call_input: i64,
    pub median_input: i64,
    pub findings: Vec<Finding>,
    pub kinds_order: Vec<String>,
}

fn format_with_commas_i64(n: i64) -> String {
    let abs = n.unsigned_abs() as usize;
    let s = crate::segment::format_with_commas(abs);
    if n < 0 {
        format!("-{s}")
    } else {
        s
    }
}

pub fn step_map(records: &[CallRecord]) -> Vec<StepRow> {
    struct Acc {
        calls: usize,
        input: i64,
        fixed: Vec<i64>,
        label: Option<String>,
        model: Option<String>,
    }
    let mut map: HashMap<String, Acc> = HashMap::new();
    for r in records {
        let key = r
            .sysfp
            .clone()
            .unwrap_or_else(|| "(no system prompt)".to_string());
        let e = map.entry(key).or_insert_with(|| Acc {
            calls: 0,
            input: 0,
            fixed: Vec::new(),
            label: None,
            model: None,
        });
        e.calls += 1;
        e.input += rec_input(r);
        if e.label.is_none() {
            e.label = r.sys_label.clone();
        }
        if e.model.is_none() {
            e.model = r.model.clone();
        }
        let fixed: i64 = r
            .segments
            .iter()
            .filter(|s| {
                matches!(
                    s.kind.as_str(),
                    "system" | "tool_def" | "response_schema"
                )
            })
            .map(|s| s.tokens)
            .sum();
        e.fixed.push(fixed);
    }
    let mut rows: Vec<StepRow> = map
        .into_iter()
        .map(|(k, a)| StepRow {
            step: k.chars().take(6).collect(),
            label: a.label,
            model: a.model,
            calls: a.calls,
            input: a.input,
            fixed_per_call: median_i64(&a.fixed),
        })
        .collect();
    rows.sort_by_key(|b| std::cmp::Reverse(b.input));
    rows
}

pub fn endpoint_map(records: &[CallRecord]) -> Vec<EndpointRow> {
    struct EpAcc {
        cls: String,
        id: String,
        host: Option<String>,
        api: String,
        calls: usize,
        input: i64,
        models: BTreeSet<String>,
    }
    let mut groups: HashMap<(String, String, String), EpAcc> = HashMap::new();
    for r in records {
        let cls = r
            .host_class
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let hid = r.host_id.clone().unwrap_or_default();
        let api = r.provider.clone();
        let key = (cls.clone(), hid.clone(), api.clone());
        let g = groups.entry(key).or_insert_with(|| EpAcc {
            cls,
            id: hid.chars().take(6).collect(),
            host: r.host.clone(),
            api,
            calls: 0,
            input: 0,
            models: BTreeSet::new(),
        });
        g.calls += 1;
        g.input += rec_input(r);
        if let Some(ref m) = r.model {
            g.models.insert(m.clone());
        }
    }
    let mut out: Vec<EndpointRow> = groups
        .into_values()
        .map(|g| EndpointRow {
            class: g.cls,
            id: g.id,
            host: g.host,
            api: g.api,
            calls: g.calls,
            input: g.input,
            models: g.models.into_iter().collect(),
        })
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.input));
    out
}

#[inline]
fn append_seg_text(buf: &mut String, prefix: Option<&str>, name: Option<&str>, txt: &str) {
    if !buf.is_empty() {
        buf.push_str("\n\n");
    }
    if let Some(p) = prefix {
        buf.push_str(p);
    }
    if let Some(nm) = name {
        buf.push('[');
        buf.push_str(nm);
        buf.push_str("]\n");
    }
    buf.push_str(txt);
}

fn build_call_detail(
    rec: &CallRecord,
    call_index: usize,
    session_id: &str,
    turn_index: usize,
    inferred_hist: i64,
    sys_cache: &mut HashMap<String, String>,
) -> CallDetail {
    let mut sys_tok = 0i64;
    let mut hist_u = 0i64;
    let mut hist_a = 0i64;
    let mut user_tok = 0i64;
    let mut td_tok = 0i64;
    let mut tc_tok = 0i64;
    let mut tr_tok = 0i64;
    let mut rs_tok = 0i64;
    let mut other_tok = 0i64;

    let cached_sys = rec.sysfp.as_ref().and_then(|sfp| sys_cache.get(sfp)).cloned();
    let mut s_sys = cached_sys.clone().unwrap_or_default();
    let mut s_hist = String::new();
    let mut s_user = String::new();
    let mut s_tools = String::new();
    let mut s_other = String::new();

    for s in &rec.segments {
        let k = s.kind.as_str();
        let tok = s.tokens;
        let nm = s.name.as_deref();
        match k {
            "system" => {
                sys_tok += tok;
                if cached_sys.is_none() {
                    if let Some(ref txt) = s.text {
                        append_seg_text(&mut s_sys, None, nm, txt);
                    }
                }
            }
            "history_user" => {
                hist_u += tok;
                if let Some(ref txt) = s.text {
                    append_seg_text(&mut s_hist, Some("(User/Context) "), nm, txt);
                }
            }
            "history_assistant" => {
                hist_a += tok;
                if let Some(ref txt) = s.text {
                    append_seg_text(&mut s_hist, Some("(Assistant) "), nm, txt);
                }
            }
            "user_prompt" | "raw_prompt" => {
                user_tok += tok;
                if let Some(ref txt) = s.text {
                    append_seg_text(&mut s_user, None, nm, txt);
                }
            }
            "tool_def" => {
                td_tok += tok;
                if let Some(ref txt) = s.text {
                    append_seg_text(&mut s_tools, Some("(tool_def) "), nm, txt);
                }
            }
            "tool_call" => {
                tc_tok += tok;
                if let Some(ref txt) = s.text {
                    append_seg_text(&mut s_tools, Some("(tool_call) "), nm, txt);
                }
            }
            "tool_result" => {
                tr_tok += tok;
                if let Some(ref txt) = s.text {
                    append_seg_text(&mut s_tools, Some("(tool_result) "), nm, txt);
                }
            }
            "response_schema" => {
                rs_tok += tok;
                if let Some(ref txt) = s.text {
                    append_seg_text(&mut s_tools, Some("(response_schema) "), nm, txt);
                }
            }
            _ => {
                other_tok += tok;
                if let Some(ref txt) = s.text {
                    append_seg_text(&mut s_other, None, nm, txt);
                }
            }
        }
    }
    if cached_sys.is_none() && !s_sys.is_empty() {
        if let Some(ref sfp) = rec.sysfp {
            sys_cache.insert(sfp.clone(), s_sys.clone());
        }
    }

    if inferred_hist > 0 && hist_u + hist_a == 0 && user_tok > inferred_hist {
        hist_u = inferred_hist;
        user_tok -= inferred_hist;
    }
    let hist_tok = hist_u + hist_a;
    let tools_tok = td_tok + tc_tok + tr_tok + rs_tok;
    let step_short: String = rec
        .sysfp
        .as_deref()
        .unwrap_or("")
        .chars()
        .take(6)
        .collect();

    CallDetail {
        index: call_index,
        id: if rec.id.is_empty() {
            format!("c{call_index}")
        } else {
            rec.id.clone()
        },
        session: session_id.to_string(),
        turn: turn_index,
        ts: rec.ts,
        model: rec
            .model
            .clone()
            .unwrap_or_else(|| {
                if rec.provider.is_empty() {
                    "-".to_string()
                } else {
                    rec.provider.clone()
                }
            }),
        provider: if rec.provider.is_empty() {
            "-".to_string()
        } else {
            rec.provider.clone()
        },
        step: if step_short.is_empty() {
            "-".to_string()
        } else {
            step_short
        },
        step_label: rec.sys_label.clone(),
        system: sys_tok,
        history: hist_tok,
        history_user: hist_u,
        history_assistant: hist_a,
        user_prompt: user_tok,
        tools_schema: tools_tok,
        tool_def: td_tok,
        tool_call: tc_tok,
        tool_result: tr_tok,
        response_schema: rs_tok,
        other: other_tok,
        residual: rec.residual.unwrap_or(0),
        input_total: rec_input(rec),
        cached: rec_cached(rec),
        output: rec_output(rec),
        latency_ms: rec.latency_ms,
        status: rec.status,
        inferred_history: inferred_hist > 0,
        prompts: CallPrompts {
            system: s_sys,
            history: s_hist,
            user_prompt: s_user,
            tools_schema: s_tools,
            other: s_other,
            response: rec.response_text.clone().unwrap_or_default(),
        },
    }
}

pub fn session_turn_breakdown(
    turns: &[Vec<&CallRecord>],
    inferred: &HashMap<(usize, usize), i64>,
) -> (Vec<SessionSummary>, Vec<TurnSummary>, Vec<CallDetail>) {
    if turns.is_empty() {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let mut session_groups: Vec<Vec<(usize, &Vec<&CallRecord>)>> = Vec::new();
    let mut prev_sysfps: HashSet<&str> = HashSet::new();
    let mut prev_convs: HashSet<&str> = HashSet::new();
    let mut prev_had_history = false;

    for (ti, g) in turns.iter().enumerate() {
        let cur_sysfps: HashSet<&str> = g.iter().filter_map(|r| r.sysfp.as_deref()).collect();
        let cur_convs: HashSet<&str> = g.iter().filter_map(|r| r.conv.as_deref()).collect();
        let cur_hist: i64 = g
            .iter()
            .enumerate()
            .map(|(ci, r)| {
                let explicit: i64 = r
                    .segments
                    .iter()
                    .filter(|s| s.kind == "history_user" || s.kind == "history_assistant")
                    .map(|s| s.tokens)
                    .sum();
                explicit + inferred.get(&(ti, ci)).copied().unwrap_or(0)
            })
            .sum();

        if session_groups.is_empty() {
            session_groups.push(vec![(ti, g)]);
        } else {
            let same_conv = !cur_convs.is_disjoint(&prev_convs);
            let shared_pipeline = !cur_sysfps.is_disjoint(&prev_sysfps);
            let history_reset = prev_had_history && cur_hist == 0 && !same_conv;
            if (same_conv || shared_pipeline) && !history_reset {
                if let Some(last_s) = session_groups.last_mut() {
                    last_s.push((ti, g));
                }
            } else {
                session_groups.push(vec![(ti, g)]);
                prev_sysfps.clear();
                prev_convs.clear();
                prev_had_history = false;
            }
        }
        prev_sysfps.extend(cur_sysfps);
        prev_convs.extend(cur_convs);
        if cur_hist > 0 {
            prev_had_history = true;
        }
    }

    let total_calls: usize = turns.iter().map(|g| g.len()).sum();
    let mut sessions_out = Vec::with_capacity(session_groups.len());
    let mut turns_out = Vec::with_capacity(turns.len());
    let mut calls_out = Vec::with_capacity(total_calls);
    let mut sys_cache: HashMap<String, String> = HashMap::with_capacity(16);
    let mut call_idx = 0usize;

    for (si, s_turns) in session_groups.into_iter().enumerate() {
        let sid = format!("S{}", si + 1);
        let turns_count = s_turns.len();
        let mut s_calls_count = 0usize;
        let mut s_models: BTreeSet<String> = BTreeSet::new();
        let mut s_sys = 0i64;
        let mut s_hist = 0i64;
        let mut s_user = 0i64;
        let mut s_tools = 0i64;
        let mut s_other = 0i64;
        let mut s_res = 0i64;
        let mut s_in = 0i64;
        let mut s_cached = 0i64;
        let mut s_out = 0i64;

        for (local_ti, (orig_ti, g)) in s_turns.into_iter().enumerate() {
            let turn_num = local_ti + 1;
            let t_start = calls_out.len();
            for (ci, r) in g.iter().enumerate() {
                call_idx += 1;
                let inf = inferred.get(&(orig_ti, ci)).copied().unwrap_or(0);
                let cd = build_call_detail(r, call_idx, &sid, turn_num, inf, &mut sys_cache);
                calls_out.push(cd);
            }
            let t_slice = &calls_out[t_start..];
            let user_preview = t_slice
                .iter()
                .find(|c| !c.prompts.user_prompt.is_empty())
                .map(|c| c.prompts.user_prompt.clone())
                .unwrap_or_default();
            let hist_preview = t_slice
                .iter()
                .find(|c| !c.prompts.history.is_empty())
                .map(|c| c.prompts.history.clone())
                .unwrap_or_default();
            let resp_preview = t_slice
                .iter()
                .rev()
                .find(|c| !c.prompts.response.is_empty())
                .map(|c| c.prompts.response.clone())
                .unwrap_or_default();
            let mut models: BTreeSet<String> = BTreeSet::new();
            for c in t_slice {
                models.insert(c.model.clone());
                s_models.insert(c.model.clone());
            }
            let t_sys: i64 = t_slice.iter().map(|c| c.system).sum();
            let t_hist: i64 = t_slice.iter().map(|c| c.history).sum();
            let t_hist_u: i64 = t_slice.iter().map(|c| c.history_user).sum();
            let t_hist_a: i64 = t_slice.iter().map(|c| c.history_assistant).sum();
            let t_user: i64 = t_slice.iter().map(|c| c.user_prompt).sum();
            let t_tools: i64 = t_slice.iter().map(|c| c.tools_schema).sum();
            let t_other: i64 = t_slice.iter().map(|c| c.other).sum();
            let t_res: i64 = t_slice.iter().map(|c| c.residual).sum();
            let t_in: i64 = t_slice.iter().map(|c| c.input_total).sum();
            let t_cached: i64 = t_slice.iter().map(|c| c.cached).sum();
            let t_out: i64 = t_slice.iter().map(|c| c.output).sum();
            let t_lat: i64 = t_slice.iter().map(|c| c.latency_ms.unwrap_or(0)).sum();

            s_calls_count += t_slice.len();
            s_sys += t_sys;
            s_hist += t_hist;
            s_user += t_user;
            s_tools += t_tools;
            s_other += t_other;
            s_res += t_res;
            s_in += t_in;
            s_cached += t_cached;
            s_out += t_out;

            turns_out.push(TurnSummary {
                session: sid.clone(),
                turn: turn_num,
                calls_count: t_slice.len(),
                models: models.into_iter().collect(),
                system: t_sys,
                history: t_hist,
                history_user: t_hist_u,
                history_assistant: t_hist_a,
                user_prompt: t_user,
                tools_schema: t_tools,
                other: t_other,
                residual: t_res,
                input_total: t_in,
                cached_total: t_cached,
                output_total: t_out,
                latency_ms: t_lat,
                user_text: user_preview,
                history_text: hist_preview,
                response_text: resp_preview,
                calls: Vec::new(),
            });
        }
        sessions_out.push(SessionSummary {
            session: sid,
            turns_count,
            calls_count: s_calls_count,
            models: s_models.into_iter().collect(),
            system: s_sys,
            history: s_hist,
            user_prompt: s_user,
            tools_schema: s_tools,
            other: s_other,
            residual: s_res,
            input_total: s_in,
            cached_total: s_cached,
            output_total: s_out,
            turns: Vec::new(),
        });
    }

    (sessions_out, turns_out, calls_out)
}

pub fn compute_by_kind(
    turns: &[Vec<&CallRecord>],
    inferred: &HashMap<(usize, usize), i64>,
) -> (BTreeMap<String, i64>, i64) {
    let mut totals: BTreeMap<String, i64> = BTreeMap::new();
    let mut reported_total = 0i64;
    for (ti, g) in turns.iter().enumerate() {
        for (ci, r) in g.iter().enumerate() {
            let mut u_call = 0i64;
            for s in &r.segments {
                *totals.entry(s.kind.clone()).or_insert(0) += s.tokens;
                if s.kind == "user_prompt" || s.kind == "raw_prompt" {
                    u_call += s.tokens;
                }
            }
            let inf = inferred.get(&(ti, ci)).copied().unwrap_or(0);
            if inf > 0
                && u_call >= inf
                && totals.get("user_prompt").copied().unwrap_or(0) >= inf
            {
                *totals.entry("user_prompt".to_string()).or_insert(0) -= inf;
                *totals.entry("history_user".to_string()).or_insert(0) += inf;
            }
            if let Some(res) = r.residual {
                *totals.entry("residual".to_string()).or_insert(0) += res;
            }
            reported_total += rec_input(r);
        }
    }
    totals.retain(|_, v| *v != 0);
    (totals, reported_total)
}

pub fn findings(
    ordered: &[CallRecord],
    turns: &[Vec<&CallRecord>],
    by_kind: &BTreeMap<String, i64>,
    min_cacheable: i64,
) -> Vec<Finding> {
    let mut out = Vec::new();
    if ordered.is_empty() {
        return out;
    }
    let total_in = ordered.iter().map(rec_input).sum::<i64>().max(1);

    let mut called: HashSet<String> = HashSet::new();
    for r in ordered {
        for tc in &r.tool_calls {
            called.insert(tc.clone());
        }
        for s in &r.segments {
            if s.kind == "tool_call" {
                if let Some(ref nm) = s.name {
                    called.insert(nm.clone());
                }
            }
        }
    }

    let mut unused: HashMap<String, i64> = HashMap::new();
    struct BlockEntry {
        tokens: i64,
        name: Option<String>,
        kind: String,
        calls: usize,
    }
    let mut seen_blocks: HashMap<String, BlockEntry> = HashMap::new();
    let mut cached = 0i64;
    let mut have_cache = false;
    let mut short = 0usize;
    let mut failed_count = 0usize;
    let mut failed_tokens = 0i64;
    let mut sys_per_call: Vec<(i64, String)> = Vec::new();
    let mut by_conv: BTreeMap<String, Vec<&CallRecord>> = BTreeMap::new();
    let mut q_counts: HashMap<String, usize> = HashMap::new();
    let mut neg_poor = 0usize;

    for r in ordered {
        let r_in = rec_input(r);
        if r_in < min_cacheable {
            short += 1;
        }
        if let Some(c_val) = r.usage.get("cached").and_then(|v| v.as_i64()) {
            have_cache = true;
            cached += c_val;
        }
        if r.error.is_some() || r.status.unwrap_or(200) >= 400 {
            failed_count += 1;
            failed_tokens += r_in;
        }
        if let Some(ref qual) = r.quality {
            *q_counts.entry(qual.clone()).or_insert(0) += 1;
            if qual == "poor" && r.residual.unwrap_or(0) < 0 {
                neg_poor += 1;
            }
        }
        let conv_key = r.conv.clone().unwrap_or_else(|| "_default".to_string());
        by_conv.entry(conv_key).or_default().push(r);

        let defined_set: Option<HashSet<&str>> = if !r.tools_defined.is_empty() {
            Some(r.tools_defined.iter().map(|s| s.as_str()).collect())
        } else {
            None
        };
        let mut sys_tok_sum = 0i64;
        let mut sys_hashes = Vec::new();
        for s in &r.segments {
            if s.kind == "tool_def" {
                if let Some(ref nm) = s.name {
                    let is_def = defined_set
                        .as_ref()
                        .is_none_or(|ds| ds.contains(nm.as_str()));
                    if is_def && !called.contains(nm) {
                        *unused.entry(nm.clone()).or_insert(0) += s.tokens;
                    }
                }
            }
            if matches!(
                s.kind.as_str(),
                "system" | "tool_def" | "response_schema" | "other"
            ) && s.tokens >= 200
                && !s.hash.is_empty()
            {
                let be = seen_blocks
                    .entry(s.hash.clone())
                    .or_insert_with(|| BlockEntry {
                        tokens: s.tokens,
                        name: s.name.clone(),
                        kind: s.kind.clone(),
                        calls: 0,
                    });
                be.calls += 1;
            }
            if s.kind == "system" {
                sys_tok_sum += s.tokens;
                sys_hashes.push(s.hash.clone());
            }
        }
        if !sys_hashes.is_empty() {
            sys_per_call.push((sys_tok_sum, sys_hashes.join("|")));
        }
    }

    if !unused.is_empty() {
        let paid: i64 = unused.values().sum();
        let mut top_unused: Vec<(&String, &i64)> = unused.iter().collect();
        top_unused.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let names: Vec<&str> = top_unused
            .into_iter()
            .take(5)
            .map(|(n, _)| n.as_str())
            .collect();
        let ratio = (paid as f64) / (total_in as f64);
        out.push(Finding {
            severity: if ratio > 0.1 { "high" } else { "medium" }.to_string(),
            text: format!(
                "{} tool definition(s) were sent but never called ({} tokens, {:.1}% of input): {}",
                unused.len(),
                format_with_commas_i64(paid),
                100.0 * ratio,
                names.join(", ")
            ),
            see: doc_link("isolate"),
        });
    }

    let mut rep: Vec<&BlockEntry> = seen_blocks.values().filter(|e| e.calls >= 2).collect();
    rep.sort_by_key(|b| std::cmp::Reverse(b.tokens * (b.calls as i64)));
    for e in rep.into_iter().take(3) {
        let paid = e.tokens * (e.calls as i64);
        let ratio = (paid as f64) / (total_in as f64);
        if ratio >= 0.05 {
            let nm_part = e
                .name
                .as_ref()
                .map(|n| format!(" '{n}'"))
                .unwrap_or_default();
            out.push(Finding {
                severity: if ratio >= 0.25 { "high" } else { "medium" }.to_string(),
                text: format!(
                    "A {}{} block of {} tokens was sent in {} calls ({} tokens, {:.1}% of input)",
                    e.kind,
                    nm_part,
                    format_with_commas_i64(e.tokens),
                    e.calls,
                    format_with_commas_i64(paid),
                    100.0 * ratio
                ),
                see: doc_link("isolate"),
            });
        }
    }

    let tr = by_kind.get("tool_result").copied().unwrap_or(0);
    if (tr as f64) / (total_in as f64) > 0.4 {
        out.push(Finding {
            severity: "high".to_string(),
            text: format!(
                "Tool results are {:.1}% of input tokens: truncate, paginate, or search before reading",
                100.0 * (tr as f64) / (total_in as f64)
            ),
            see: doc_link("improve"),
        });
    }

    if have_cache && ordered.len() > 3 {
        let pct = 100.0 * (cached as f64) / (total_in as f64);
        if pct < 30.0 {
            out.push(Finding {
                severity: "medium".to_string(),
                text: format!(
                    "Cache hit rate is {pct:.1}%. Check that stable content comes first and the history is append-only"
                ),
                see: doc_link("concepts"),
            });
        }
    }

    if short > 0 && short == ordered.len() {
        out.push(Finding {
            severity: "low".to_string(),
            text: format!(
                "All calls are under {min_cacheable} tokens; provider prompt caching typically needs a longer prefix (check your provider's minimum)"
            ),
            see: doc_link("concepts"),
        });
    }

    let sizes: Vec<usize> = turns.iter().map(|g| g.len()).collect();
    if !sizes.is_empty() {
        let med_sz = median_f64(&sizes);
        let max_sz = sizes.iter().copied().max().unwrap_or(0);
        if med_sz >= 4.0 {
            let med_str = if (med_sz - med_sz.round()).abs() < 1e-6 {
                format!("{}", med_sz as usize)
            } else {
                format!("{med_sz:.1}")
            };
            out.push(Finding {
                severity: "medium".to_string(),
                text: format!(
                    "Each user request triggers a median of {med_str} LLM calls (max {max_sz}): look for steps to skip, merge, or run on a smaller model"
                ),
                see: doc_link("improve"),
            });
        }
    }

    if failed_count >= 2 {
        let ratio = (failed_tokens as f64) / (total_in as f64);
        out.push(Finding {
            severity: if failed_count >= 3 || ratio >= 0.15 {
                "high"
            } else {
                "medium"
            }
            .to_string(),
            text: format!(
                "{failed_count} call(s) failed with an error or HTTP >= 400 ({} input tokens): check retry/backoff or failure-loop guards",
                format_with_commas_i64(failed_tokens)
            ),
            see: doc_link("improve"),
        });
    }

    for conv_recs in by_conv.values() {
        let mut streak_name: Option<String> = None;
        let mut streak_len = 0usize;
        let mut streak_tokens = 0i64;
        for r in conv_recs {
            let tcs: Vec<String> = if !r.tool_calls.is_empty() {
                r.tool_calls.clone()
            } else {
                r.segments
                    .iter()
                    .filter(|s| s.kind == "tool_call")
                    .filter_map(|s| s.name.clone())
                    .collect()
            };
            let sig = if tcs.is_empty() {
                None
            } else {
                Some(tcs.join(","))
            };
            if sig.is_some() && sig == streak_name {
                streak_len += 1;
                streak_tokens += rec_input(r);
            } else {
                if let Some(ref sn) = streak_name {
                    if streak_len >= 3 {
                        out.push(Finding {
                            severity: "high".to_string(),
                            text: format!(
                                "Tool call '{sn}' was repeated {streak_len} times in a row ({} input tokens): add a loop guard that redirects after identical calls",
                                format_with_commas_i64(streak_tokens)
                            ),
                            see: doc_link("improve"),
                        });
                    }
                }
                if sig.is_some() {
                    streak_name = sig;
                    streak_len = 1;
                    streak_tokens = rec_input(r);
                } else {
                    streak_name = None;
                    streak_len = 0;
                    streak_tokens = 0;
                }
            }
        }
        if let Some(ref sn) = streak_name {
            if streak_len >= 3 {
                out.push(Finding {
                    severity: "high".to_string(),
                    text: format!(
                        "Tool call '{sn}' was repeated {streak_len} times in a row ({} input tokens): add a loop guard that redirects after identical calls",
                        format_with_commas_i64(streak_tokens)
                    ),
                    see: doc_link("improve"),
                });
            }
        }
    }

    if sys_per_call.len() >= 3 {
        let toks: Vec<i64> = sys_per_call.iter().map(|(t, _)| *t).collect();
        let hashes: HashSet<&str> = sys_per_call
            .iter()
            .filter(|(_, h)| !h.is_empty())
            .map(|(_, h)| h.as_str())
            .collect();
        let med = median_i64(&toks);
        let max_t = toks.iter().copied().max().unwrap_or(0);
        let min_t = toks.iter().copied().min().unwrap_or(0);
        let min_hashes = 3usize.max((0.8 * (sys_per_call.len() as f64)) as usize);
        let max_spread = 32i64.max((0.08 * (med as f64)) as i64);
        if med >= 200 && hashes.len() >= min_hashes && (max_t - min_t) <= max_spread {
            out.push(Finding {
                severity: "high".to_string(),
                text: format!(
                    "System prompt hash changed on {} of {} calls while its size stayed ~{} tokens: a dynamic value (timestamp, UUID, or unsorted keys) is busting the prefix cache",
                    hashes.len(),
                    sys_per_call.len(),
                    format_with_commas_i64(med)
                ),
                see: doc_link("improve"),
            });
        }
    }

    for conv_recs in by_conv.values() {
        if conv_recs.len() < 4 {
            continue;
        }
        let hist_series: Vec<i64> = conv_recs
            .iter()
            .map(|r| {
                r.segments
                    .iter()
                    .filter(|s| {
                        matches!(
                            s.kind.as_str(),
                            "history_user" | "history_assistant" | "tool_result"
                        )
                    })
                    .map(|s| s.tokens)
                    .sum()
            })
            .collect();
        let non_zero: Vec<i64> = hist_series.iter().copied().filter(|&h| h > 0).collect();
        let last_h = *hist_series.last().unwrap_or(&0);
        if non_zero.len() >= 3 && last_h >= 1500 && last_h >= 3 * non_zero[0] {
            let ratio = (last_h as f64) / (non_zero[0].max(1) as f64);
            out.push(Finding {
                severity: if last_h >= 8000 || ratio >= 6.0 {
                    "high"
                } else {
                    "medium"
                }
                .to_string(),
                text: format!(
                    "Conversation history grew {ratio:.1}x across {} turns (from {} to {} tokens): compact older turns or isolate sub-tasks in fresh sessions",
                    conv_recs.len(),
                    format_with_commas_i64(non_zero[0]),
                    format_with_commas_i64(last_h)
                ),
                see: doc_link("improve"),
            });
            break;
        }
    }

    let poor = q_counts.get("poor").copied().unwrap_or(0);
    let q_total: usize = q_counts.values().sum();
    if poor > 0 && (poor as f64) / (q_total.max(1) as f64) > 0.3 {
        let note = if neg_poor > 0 {
            " (reported input is smaller than counted segments on some calls; servers such as Ollama report only newly evaluated tokens in prompt_eval_count and omit KV-cached prefix tokens)"
        } else {
            " (opaque content, hidden tokens, or an approximate tokenizer)"
        };
        out.push(Finding {
            severity: "info".to_string(),
            text: format!(
                "{poor} of {q_total} calls have a large unexplained residual{note}; treat the breakdown as approximate"
            ),
            see: doc_link("diagnose"),
        });
    }

    out
}

pub fn summarize(records: &[CallRecord], gap_s: f64) -> SummaryReport {
    let owned_ordered;
    let ordered: &[CallRecord] = if is_sorted_by_ts(records) {
        records
    } else {
        owned_ordered = sorted_records(records);
        &owned_ordered
    };
    let turns = group_requests_ref(ordered, gap_s);
    let inferred = infer_history_deltas(&turns);
    let (by_kind, input_total) = compute_by_kind(&turns, &inferred);
    let inputs: Vec<i64> = ordered.iter().map(rec_input).collect();
    let first_call_input = ordered.first().map(rec_input).unwrap_or(0);
    let (sessions, turn_summaries, call_details) = session_turn_breakdown(&turns, &inferred);
    let fan_out: Vec<usize> = turns.iter().map(|g| g.len()).collect();
    let mut kinds_order: Vec<String> = KINDS.iter().map(|s| (*s).to_string()).collect();
    kinds_order.push("residual".to_string());

    SummaryReport {
        calls: ordered.len(),
        input_total,
        output_total: ordered
            .iter()
            .filter_map(|r| r.usage.get("output").and_then(|v| v.as_i64()))
            .sum(),
        cached_total: ordered.iter().map(rec_cached).sum(),
        by_kind: by_kind.clone(),
        steps: step_map(ordered),
        endpoints: endpoint_map(ordered),
        sessions,
        turns: turn_summaries,
        call_details,
        fan_out,
        first_call_input,
        median_input: median_i64(&inputs),
        findings: findings(ordered, &turns, &by_kind, 1024),
        kinds_order,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub total: f64,
    pub models_without_price: Vec<String>,
}

fn val_as_f64(v: Option<&Value>) -> Option<f64> {
    let val = v?;
    if let Some(f) = val.as_f64() {
        return if f.is_finite() { Some(f.max(0.0)) } else { None };
    }
    if let Some(s) = val.as_str() {
        if let Ok(f) = s.trim().parse::<f64>() {
            return if f.is_finite() { Some(f.max(0.0)) } else { None };
        }
    }
    None
}

pub fn estimate_cost(records: &[CallRecord], prices: &Map<String, Value>) -> CostEstimate {
    let mut total = 0.0f64;
    let mut missing = BTreeSet::new();
    for r in records {
        let model_key = r.model.as_deref().unwrap_or("");
        let p_obj = prices
            .get(model_key)
            .or_else(|| prices.get("default"))
            .and_then(|v| v.as_object());
        let in_total = r.usage.get("input_total").and_then(|v| v.as_i64());
        let (Some(p), Some(in_tok)) = (p_obj, in_total) else {
            missing.insert(if model_key.is_empty() {
                "?".to_string()
            } else {
                model_key.to_string()
            });
            continue;
        };
        let cached = rec_cached(r).max(0);
        let uncached = (in_tok - cached).max(0);
        let out_tok = r
            .usage
            .get("output")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .max(0);
        let p_in = val_as_f64(p.get("input")).unwrap_or(0.0);
        let p_cached = val_as_f64(p.get("cached")).unwrap_or(p_in);
        let p_out = val_as_f64(p.get("output")).unwrap_or(0.0);
        total += ((uncached as f64) * p_in + (cached as f64) * p_cached + (out_tok as f64) * p_out)
            / 1_000_000.0;
    }
    CostEstimate {
        total,
        models_without_price: missing.into_iter().collect(),
    }
}

pub fn bootstrap_diff(a: &[f64], b: &[f64], n: usize, seed: u64) -> Option<(f64, f64, f64)> {
    if a.is_empty() || b.is_empty() || n == 0 {
        return None;
    }
    let mut state = seed.wrapping_add(0x9E3779B97F4A7C15);
    let mut next_u64 = || -> u64 {
        state = state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    };

    let na = a.len();
    let nb = b.len();
    let inv_a = 1.0 / (na as f64);
    let inv_b = 1.0 / (nb as f64);
    let mut diffs = Vec::with_capacity(n);
    for _ in 0..n {
        let mut sa = 0.0;
        for _ in 0..na {
            let idx = (next_u64() as usize) % na;
            sa += a[idx];
        }
        let mut sb = 0.0;
        for _ in 0..nb {
            let idx = (next_u64() as usize) % nb;
            sb += b[idx];
        }
        diffs.push(sb * inv_b - sa * inv_a);
    }
    diffs.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let mean_a = a.iter().sum::<f64>() * inv_a;
    let mean_b = b.iter().sum::<f64>() * inv_b;
    let point = mean_b - mean_a;
    let lo_idx = ((0.025 * (n as f64)) as usize).min(n - 1);
    let hi_idx = (((0.975 * (n as f64)) as usize).saturating_sub(1)).min(n - 1);
    Some((point, diffs[lo_idx], diffs[hi_idx]))
}

pub fn compare(before: &[CallRecord], after: &[CallRecord], gap_s: f64) -> Value {
    let ga = group_requests(before, gap_s);
    let gb = group_requests(after, gap_s);

    let in_a: Vec<f64> = ga
        .iter()
        .map(|g| g.iter().map(rec_input).sum::<i64>() as f64)
        .collect();
    let in_b: Vec<f64> = gb
        .iter()
        .map(|g| g.iter().map(rec_input).sum::<i64>() as f64)
        .collect();
    let calls_a: Vec<f64> = ga.iter().map(|g| g.len() as f64).collect();
    let calls_b: Vec<f64> = gb.iter().map(|g| g.len() as f64).collect();

    let mut metrics = Map::new();
    for (name, a, b) in [
        ("input_per_request", &in_a, &in_b),
        ("calls_per_request", &calls_a, &calls_b),
    ] {
        let mean_a = if a.is_empty() {
            0.0
        } else {
            a.iter().sum::<f64>() / (a.len() as f64)
        };
        let mean_b = if b.is_empty() {
            0.0
        } else {
            b.iter().sum::<f64>() / (b.len() as f64)
        };
        if let Some((diff, lo, hi)) = bootstrap_diff(a, b, 2000, 0) {
            let rel = if mean_a != 0.0 {
                Value::from(100.0 * diff / mean_a)
            } else {
                Value::Null
            };
            metrics.insert(
                name.to_string(),
                json!({
                    "before_mean": mean_a,
                    "after_mean": mean_b,
                    "diff": diff,
                    "ci95": [lo, hi],
                    "relative_change_pct": rel,
                    "significant": lo > 0.0 || hi < 0.0,
                }),
            );
        } else {
            metrics.insert(
                name.to_string(),
                json!({
                    "before_mean": mean_a,
                    "after_mean": mean_b,
                    "diff": Value::Null,
                    "ci95": Value::Null,
                    "relative_change_pct": Value::Null,
                    "significant": false,
                }),
            );
        }
    }

    let reliable = ga.len().min(gb.len()) >= 5;
    json!({
        "requests": {
            "before": ga.len(),
            "after": gb.len(),
        },
        "metrics": metrics,
        "reliable": reliable,
    })
}

pub fn to_legacy(records: &[CallRecord], gap_s: f64) -> Vec<Value> {
    let mut rows = Vec::new();
    for (gi, g) in group_requests(records, gap_s).into_iter().enumerate() {
        let turn_num = gi + 1;
        for r in g {
            let defs: Vec<Value> = r
                .segments
                .iter()
                .filter(|s| s.kind == "tool_def")
                .map(|s| {
                    json!({
                        "name": s.name.as_deref().unwrap_or("tool"),
                        "tokens": s.tokens,
                    })
                })
                .collect();
            let tcs: Vec<Value> = r
                .tool_calls
                .iter()
                .map(|n| {
                    json!({
                        "name": n,
                        "error": Value::Null,
                        "output_tokens": 0,
                    })
                })
                .collect();
            let sess = r
                .conv
                .clone()
                .or_else(|| r.run.clone())
                .unwrap_or_else(|| "s".to_string());
            rows.push(json!({
                "session": sess,
                "turn": turn_num,
                "model": r.model,
                "input_tokens": rec_input(&r),
                "cached_input_tokens": rec_cached(&r),
                "output_tokens": rec_output(&r),
                "tool_defs": defs,
                "tool_calls": tcs,
            }));
        }
    }
    rows
}
