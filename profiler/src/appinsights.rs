use crate::counting::get_counter;
use crate::detect::{classify_host, parse_url_parts, Detector};
use crate::gateway::{decode_content_encoding, BodyFramer};
use crate::model::{CallRecord, Usage, SCHEMA_VERSION};
use crate::parse_request::{parse_request, ANTHROPIC, OPENAI_CHAT};
use crate::parse_response::{parse_response_body, ParsedResponse};
use crate::segment::{build_record, hash_text, BuildRecordArgs};
use crate::upstream::{connect_tcp_timeout, connect_via_proxy, make_upstream_tls_config, proxy_for};
use base64::Engine;
use flate2::read::GzDecoder;
use rustls::pki_types::ServerName;
use rustls::{ClientConnection, StreamOwned};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IngestStats {
    pub total_rows: usize,
    pub tier1_full_calls: usize,
    pub tier2_usage_only_calls: usize,
    pub lossless_chunk_calls: usize,
    pub truncated_repaired_calls: usize,
    pub sampled_calls: usize,
    pub max_item_count: i64,
    pub deduplicated_spans: usize,
    pub upgraded_from_usage_only: usize,
    pub table_rbac_fallback: bool,
    pub partial_error_warning: Option<String>,
    pub max_ingestion_lag_ms: i64,
    pub median_ingestion_lag_ms: i64,
    pub pending_late_spans: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullCheckpoint {
    pub workspace: Option<String>,
    pub app_insights: Option<String>,
    pub resource_id: Option<String>,
    pub cloud: Option<String>,
    pub out_path: String,
    pub last_ts: f64,
    pub total_calls: usize,
    pub usage_only_calls: usize,
    pub updated_epoch_s: f64,
}

#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub salt: Vec<u8>,
    pub components: HashMap<String, String>,
    pub use_tiktoken: bool,
    pub include_text: Option<bool>,
    pub usage_only: bool,
    pub run_id: Option<String>,
    pub conv_filter: Option<String>,
    pub comm_filter: Option<String>,
}

impl Default for IngestOptions {
    fn default() -> Self {
        Self {
            salt: b"tokprof-az".to_vec(),
            components: HashMap::new(),
            use_tiktoken: true,
            include_text: None,
            usage_only: false,
            run_id: Some("az-pull".to_string()),
            conv_filter: None,
            comm_filter: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentInspection {
    pub mode: String,
    pub total_calls: usize,
    pub full_segment_calls: usize,
    pub usage_only_calls: usize,
    pub total_segments: usize,
    pub segments_with_text: usize,
    pub stored_prompt_chars: usize,
    pub stored_response_chars: usize,
    pub preview_32k_capped_segments: usize,
    pub appinsights_32kb_repaired_calls: usize,
    pub lossless_chunk_calls: usize,
}

pub fn strip_records_content(records: &mut [CallRecord]) {
    for r in records {
        r.response_text = None;
        for s in &mut r.segments {
            s.text = None;
        }
    }
}

pub fn inspect_records_content(records: &[CallRecord]) -> ContentInspection {
    let mut info = ContentInspection {
        total_calls: records.len(),
        ..Default::default()
    };
    for r in records {
        if r.quality.as_deref() == Some("usage_only") || r.segments.is_empty() {
            info.usage_only_calls += 1;
        } else {
            info.full_segment_calls += 1;
        }
        if let Some(ref rt) = r.response_text {
            info.stored_response_chars += rt.len();
        }
        if let Some(det) = r.detect.as_ref().and_then(|v| v.as_object()) {
            if det
                .get("truncated_repaired")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                info.appinsights_32kb_repaired_calls += 1;
            }
            if det.get("source").and_then(|v| v.as_str()) == Some("tokprof_chunk_bridge") {
                info.lossless_chunk_calls += 1;
            }
        }
        for s in &r.segments {
            info.total_segments += 1;
            if let Some(ref txt) = s.text {
                if !txt.is_empty() {
                    info.segments_with_text += 1;
                    info.stored_prompt_chars += txt.len();
                    if txt.contains("... [truncated, ") {
                        info.preview_32k_capped_segments += 1;
                    }
                }
            }
        }
    }
    info.mode = if info.total_calls > 0 && info.usage_only_calls == info.total_calls {
        "usage-only".to_string()
    } else if info.segments_with_text == 0 && info.stored_response_chars == 0 {
        "no-content".to_string()
    } else {
        "full-prompts".to_string()
    };
    info
}

pub fn is_cloud_shell() -> bool {
    if let Ok(v) = std::env::var("CLOUD_SHELL") {
        let vl = v.trim().to_ascii_lowercase();
        if vl == "true" || vl == "1" {
            return true;
        }
    }
    if std::env::var("ACC_CLOUD")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    if let Ok(v) = std::env::var("AZUREPS_HOST_ENVIRONMENT") {
        if v.to_ascii_lowercase().contains("cloud-shell") {
            return true;
        }
    }
    false
}

/// Returns a persistent `.tokprof` directory that survives Azure Cloud Shell 20-30m session timeouts.
/// Prefers `$TOKPROF_CLOUDDRIVE_DIR/.tokprof` or `~/clouddrive/.tokprof` when in Cloud Shell.
pub fn cloud_shell_persistent_dir() -> Option<std::path::PathBuf> {
    if let Ok(cd) = std::env::var("TOKPROF_CLOUDDRIVE_DIR") {
        let p = std::path::PathBuf::from(cd.trim());
        if !cd.trim().is_empty() && p.exists() {
            return Some(p.join(".tokprof"));
        }
    }
    if !is_cloud_shell() {
        return None;
    }
    if let Ok(home) = std::env::var("HOME") {
        let clouddrive = std::path::PathBuf::from(&home).join("clouddrive");
        if clouddrive.is_dir() {
            return Some(clouddrive.join(".tokprof"));
        }
        return Some(std::path::PathBuf::from(home).join(".tokprof"));
    }
    None
}

pub fn checkpoint_path(explicit_dir: Option<&str>) -> String {
    if let Some(d) = explicit_dir.filter(|s| !s.is_empty()) {
        return std::path::Path::new(d)
            .join("pull-checkpoint.json")
            .to_string_lossy()
            .into_owned();
    }
    if let Some(cs_dir) = cloud_shell_persistent_dir() {
        return cs_dir
            .join("pull-checkpoint.json")
            .to_string_lossy()
            .into_owned();
    }
    ".tokprof/pull-checkpoint.json".to_string()
}

pub fn save_pull_checkpoint(cp: &PullCheckpoint, path: &str) {
    if let Ok(txt) = serde_json::to_string_pretty(cp) {
        let _ = crate::store::atomic_write_0600(path, &txt);
    }
}

pub fn load_pull_checkpoint(path: &str) -> Option<PullCheckpoint> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<PullCheckpoint>(&raw).ok()
}

pub fn report_candidate_ports(preferred: Option<u16>, cloud_shell: bool) -> Vec<u16> {
    let mut ports = Vec::with_capacity(24);
    if let Some(p) = preferred.filter(|&p| p > 0) {
        ports.push(p);
    }
    if let Ok(env_p) = std::env::var("TOKPROF_REPORT_PORT") {
        if let Ok(p) = env_p.trim().parse::<u16>() {
            if p > 0 && !ports.contains(&p) {
                ports.push(p);
            }
        }
    }
    if cloud_shell {
        // Azure Cloud Shell Web Preview only proxies ports 8000..=8099 and 8888
        for p in [8080u16, 8081, 8082, 8000, 8001, 8088, 8888] {
            if !ports.contains(&p) {
                ports.push(p);
            }
        }
        for p in 8083..=8090 {
            if !ports.contains(&p) {
                ports.push(p);
            }
        }
    }
    for p in 18765..=18775 {
        if !ports.contains(&p) {
            ports.push(p);
        }
    }
    ports
}

pub fn build_app_insights_kql(
    since: &str,
    conv_filter: Option<&str>,
    comm_filter: Option<&str>,
) -> String {
    build_app_insights_kql_ext(since, conv_filter, comm_filter, None, None)
}

pub fn build_app_insights_kql_ext(
    since: &str,
    conv_filter: Option<&str>,
    comm_filter: Option<&str>,
    min_ts_secs: Option<f64>,
    limit: Option<usize>,
) -> String {
    let dur = normalize_kql_timespan(since);
    let mut filters = String::new();
    if let Some(min_ts) = min_ts_secs.filter(|&t| t > 0.0) {
        let secs = (min_ts.floor() as i64).max(0);
        filters.push_str(&format!(
            "\n| where TimeGenerated >= unixtime_seconds_todatetime({secs})"
        ));
    }
    if let Some(cv) = conv_filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let safe = cv.replace('\'', "");
        filters.push_str(&format!(
            "\n| where OperationId == '{safe}' or tostring(Properties['gen_ai.conversation.id']) == '{safe}' or tostring(Properties['session.id']) == '{safe}'"
        ));
    }
    if let Some(cm) = comm_filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let safe = cm.replace('\'', "");
        filters.push_str(&format!(
            "\n| where AppRoleName has '{safe}' or Name has '{safe}' or Target has '{safe}'"
        ));
    }
    let lim_clause = limit
        .filter(|&n| n > 0)
        .map(|n| format!("\n| take {n}"))
        .unwrap_or_default();
    format!(
        "union isfuzzy=true \
        (AppDependencies | where TimeGenerated > ago({dur}) | project TimeGenerated, IngestionTime=tostring(ingestion_time()), RecordKind='dependency', OperationId=tostring(OperationId), SpanId=tostring(Id), ParentId=tostring(ParentId), Name=tostring(Name), Target=tostring(Target), ResultCode=tostring(ResultCode), DurationMs=todouble(DurationMs), Message='', Properties= todynamic(Properties), ItemCount=toint(coalesce(ItemCount, 1)), AppRoleName=tostring(AppRoleName)), \
        (AppTraces | where TimeGenerated > ago({dur}) | project TimeGenerated, IngestionTime=tostring(ingestion_time()), RecordKind='trace', OperationId=tostring(OperationId), SpanId='', ParentId=tostring(ParentId), Name='', Target='', ResultCode='', DurationMs=0.0, Message=tostring(Message), Properties=todynamic(Properties), ItemCount=toint(coalesce(ItemCount, 1)), AppRoleName=tostring(AppRoleName)), \
        (dependencies | where timestamp > ago({dur}) | project TimeGenerated=timestamp, IngestionTime=tostring(ingestion_time()), RecordKind='dependency', OperationId=tostring(operation_Id), SpanId=tostring(id), ParentId=tostring(operation_ParentId), Name=tostring(name), Target=tostring(target), ResultCode=tostring(resultCode), DurationMs=todouble(duration), Message='', Properties=todynamic(customDimensions), ItemCount=toint(coalesce(itemCount, 1)), AppRoleName=tostring(cloud_RoleName)), \
        (traces | where timestamp > ago({dur}) | project TimeGenerated=timestamp, IngestionTime=tostring(ingestion_time()), RecordKind='trace', OperationId=tostring(operation_Id), SpanId='', ParentId=tostring(operation_ParentId), Name='', Target='', ResultCode='', DurationMs=0.0, Message=tostring(message), Properties=todynamic(customDimensions), ItemCount=toint(coalesce(itemCount, 1)), AppRoleName=tostring(cloud_RoleName)) \
        | where RecordKind == 'trace' or Name has_any ('chat', 'completions', 'responses', 'messages', 'openai', 'anthropic', 'invoke', 'generate') or Target has_any ('openai', 'anthropic', 'cognitiveservices', 'services.ai.azure.com', 'models.ai.azure.com') or isnotempty(tostring(Properties['gen_ai.system'])) or isnotempty(tostring(Properties['gen_ai.request.model'])) or isnotempty(tostring(Properties['llm.model_name'])) or Message startswith '[TOKPROF_'{filters}\n| order by TimeGenerated asc{lim_clause}"
    )
}

/// Fallback KQL query used when Table-level RBAC denies access to `AppTraces` / `traces`
/// (common when organizations restrict raw user-message logs while permitting `AppDependencies`).
pub fn build_dependencies_only_kql(
    since: &str,
    conv_filter: Option<&str>,
    comm_filter: Option<&str>,
) -> String {
    build_dependencies_only_kql_ext(since, conv_filter, comm_filter, None, None)
}

pub fn build_dependencies_only_kql_ext(
    since: &str,
    conv_filter: Option<&str>,
    comm_filter: Option<&str>,
    min_ts_secs: Option<f64>,
    limit: Option<usize>,
) -> String {
    let dur = normalize_kql_timespan(since);
    let mut filters = String::new();
    if let Some(min_ts) = min_ts_secs.filter(|&t| t > 0.0) {
        let secs = (min_ts.floor() as i64).max(0);
        filters.push_str(&format!(
            "\n| where TimeGenerated >= unixtime_seconds_todatetime({secs})"
        ));
    }
    if let Some(cv) = conv_filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let safe = cv.replace('\'', "");
        filters.push_str(&format!(
            "\n| where OperationId == '{safe}' or tostring(Properties['gen_ai.conversation.id']) == '{safe}' or tostring(Properties['session.id']) == '{safe}'"
        ));
    }
    if let Some(cm) = comm_filter.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let safe = cm.replace('\'', "");
        filters.push_str(&format!(
            "\n| where AppRoleName has '{safe}' or Name has '{safe}' or Target has '{safe}'"
        ));
    }
    let lim_clause = limit
        .filter(|&n| n > 0)
        .map(|n| format!("\n| take {n}"))
        .unwrap_or_default();
    format!(
        "union isfuzzy=true \
        (AppDependencies | where TimeGenerated > ago({dur}) | project TimeGenerated, IngestionTime=tostring(ingestion_time()), RecordKind='dependency', OperationId=tostring(OperationId), SpanId=tostring(Id), ParentId=tostring(ParentId), Name=tostring(Name), Target=tostring(Target), ResultCode=tostring(ResultCode), DurationMs=todouble(DurationMs), Message='', Properties=todynamic(Properties), ItemCount=toint(coalesce(ItemCount, 1)), AppRoleName=tostring(AppRoleName)), \
        (dependencies | where timestamp > ago({dur}) | project TimeGenerated=timestamp, IngestionTime=tostring(ingestion_time()), RecordKind='dependency', OperationId=tostring(operation_Id), SpanId=tostring(id), ParentId=tostring(operation_ParentId), Name=tostring(name), Target=tostring(target), ResultCode=tostring(resultCode), DurationMs=todouble(duration), Message='', Properties=todynamic(customDimensions), ItemCount=toint(coalesce(itemCount, 1)), AppRoleName=tostring(cloud_RoleName)) \
        | where Name has_any ('chat', 'completions', 'responses', 'messages', 'openai', 'anthropic', 'invoke', 'generate') or Target has_any ('openai', 'anthropic', 'cognitiveservices', 'services.ai.azure.com', 'models.ai.azure.com') or isnotempty(tostring(Properties['gen_ai.system'])) or isnotempty(tostring(Properties['gen_ai.request.model'])) or isnotempty(tostring(Properties['llm.model_name'])){filters}\n| order by TimeGenerated asc{lim_clause}"
    )
}

fn normalize_kql_timespan(spec: &str) -> String {
    let s = spec.trim().to_ascii_lowercase();
    if s.is_empty() {
        return "24h".to_string();
    }
    let valid = s
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, 's' | 'm' | 'h' | 'd'));
    if valid && s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        if s.ends_with(|c: char| c.is_ascii_digit()) {
            format!("{s}h")
        } else {
            s
        }
    } else {
        "24h".to_string()
    }
}

pub fn parse_or_repair_json(raw: &str) -> Option<(Value, bool)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return None;
    }
    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
        return Some((val, false));
    }
    let repaired = repair_truncated_json_unchecked(trimmed);
    if let Ok(val) = serde_json::from_str::<Value>(&repaired) {
        return Some((val, true));
    }
    if let Some(idx) = trimmed.rfind([',', ':']) {
        let rep2 = repair_truncated_json_unchecked(&trimmed[..idx]);
        if let Ok(val) = serde_json::from_str::<Value>(&rep2) {
            return Some((val, true));
        }
    }
    None
}

/// Repairs a JSON object/array string that was truncated by Application Insights' 32KB/8KB property limit.
/// Returns `(repaired_json, was_repaired)`.
pub fn repair_truncated_json(raw: &str) -> (String, bool) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (String::new(), false);
    }
    if serde_json::from_str::<Value>(trimmed).is_ok() {
        return (trimmed.to_string(), false);
    }
    if !trimmed.starts_with('{') && !trimmed.starts_with('[') {
        return (trimmed.to_string(), false);
    }
    let out = repair_truncated_json_unchecked(trimmed);
    if serde_json::from_str::<Value>(&out).is_ok() {
        return (out, true);
    }
    if let Some(idx) = trimmed.rfind([',', ':']) {
        let rep2 = repair_truncated_json_unchecked(&trimmed[..idx]);
        if serde_json::from_str::<Value>(&rep2).is_ok() {
            return (rep2, true);
        }
    }
    (out, true)
}

fn repair_truncated_json_unchecked(trimmed: &str) -> String {
    // Track stack of open containers: ('{' or '[', seen_colon_in_current_entry)
    let mut stack: Vec<(char, bool)> = Vec::with_capacity(32);
    let mut in_string = false;
    let mut escape = false;
    let mut last_closed_string_in_obj_key = false;
    let mut out = String::with_capacity(trimmed.len() + 64);

    for ch in trimmed.chars() {
        out.push(ch);
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
                if let Some(&(top_c, seen_colon)) = stack.last() {
                    if top_c == '{' && !seen_colon {
                        last_closed_string_in_obj_key = true;
                    }
                }
            }
        } else {
            match ch {
                '"' => {
                    in_string = true;
                    last_closed_string_in_obj_key = false;
                }
                ':' => {
                    last_closed_string_in_obj_key = false;
                    if let Some(last) = stack.last_mut() {
                        if last.0 == '{' {
                            last.1 = true;
                        }
                    }
                }
                ',' => {
                    last_closed_string_in_obj_key = false;
                    if let Some(last) = stack.last_mut() {
                        if last.0 == '{' {
                            last.1 = false;
                        }
                    }
                }
                '{' | '[' => {
                    last_closed_string_in_obj_key = false;
                    stack.push((ch, false));
                }
                '}' => {
                    last_closed_string_in_obj_key = false;
                    if stack.last().map(|x| x.0) == Some('{') {
                        stack.pop();
                    }
                }
                ']' => {
                    last_closed_string_in_obj_key = false;
                    if stack.last().map(|x| x.0) == Some('[') {
                        stack.pop();
                    }
                }
                c if !c.is_whitespace() => {
                    last_closed_string_in_obj_key = false;
                }
                _ => {}
            }
        }
    }

    if in_string {
        if escape && out.ends_with('\\') {
            out.pop();
        }
        out.push_str("... [truncated by App Insights]\"");
        if let Some(&(top_c, seen_colon)) = stack.last() {
            if top_c == '{' && !seen_colon {
                last_closed_string_in_obj_key = true;
            }
        }
    }

    loop {
        let t = out.trim_end();
        if t.len() < out.len() {
            out.truncate(t.len());
        }
        if out.ends_with(',') {
            out.pop();
            last_closed_string_in_obj_key = false;
            continue;
        }
        if out.ends_with(':') {
            out.push_str("null");
            last_closed_string_in_obj_key = false;
            break;
        }
        break;
    }

    if last_closed_string_in_obj_key {
        out.push_str(":null");
    }

    while let Some((open, _)) = stack.pop() {
        out.push(if open == '{' { '}' } else { ']' });
    }
    out
}

#[derive(Debug, Default)]
struct ChunkAssembly {
    total: usize,
    parts: BTreeMap<usize, String>,
}

#[derive(Debug, Default, Clone)]
struct NormalizedRow {
    ts: f64,
    ingestion_lag_ms: Option<i64>,
    record_kind: String, // "dependency" | "trace"
    operation_id: String,
    span_id: String,
    parent_id: String,
    name: String,
    target: String,
    result_code: Option<i64>,
    duration_ms: Option<i64>,
    message: String,
    props: Map<String, Value>,
    item_count: i64,
}

fn parse_timestamp_secs(v: Option<&Value>) -> Option<f64> {
    let val = v?;
    if let Some(f) = val.as_f64() {
        if f > 1e12 {
            return Some(f / 1000.0);
        }
        return Some(f);
    }
    let s = val.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(f) = s.parse::<f64>() {
        return Some(if f > 1e12 { f / 1000.0 } else { f });
    }
    // Parse ISO-8601 `YYYY-MM-DDTHH:MM:SS(.sss)Z`
    parse_iso8601_utc(s)
}

fn parse_iso8601_utc(s: &str) -> Option<f64> {
    let clean = s.trim().trim_end_matches('Z');
    let (date_part, time_part) = clean.split_once('T').or_else(|| clean.split_once(' '))?;
    let mut d_it = date_part.split('-');
    let year: i64 = d_it.next()?.parse().ok()?;
    let month: i64 = d_it.next()?.parse().ok()?;
    let day: i64 = d_it.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Strip timezone offset (+09:00 or -05:00) if present
    let (t_core, tz_offset_secs) = if let Some((tp, off)) = time_part.split_once('+') {
        (tp, parse_tz_offset(off).unwrap_or(0.0))
    } else if let Some((tp, off)) = time_part.rsplit_once('-') {
        if off.contains(':') || off.len() == 4 {
            (tp, -parse_tz_offset(off).unwrap_or(0.0))
        } else {
            (time_part, 0.0)
        }
    } else {
        (time_part, 0.0)
    };
    let mut t_it = t_core.split(':');
    let hour: i64 = t_it.next()?.parse().ok()?;
    let min: i64 = t_it.next()?.parse().ok()?;
    let sec_f: f64 = t_it.next().unwrap_or("0").parse().ok()?;

    // Days from 1970-01-01 (civil calendar algorithm)
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m_adj = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * m_adj + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    let total = (days as f64) * 86400.0 + (hour as f64) * 3600.0 + (min as f64) * 60.0 + sec_f
        - tz_offset_secs;
    Some(total)
}

fn parse_tz_offset(off: &str) -> Option<f64> {
    let (h, m) = if let Some((hs, ms)) = off.split_once(':') {
        (hs.parse::<i64>().ok()?, ms.parse::<i64>().ok()?)
    } else if off.len() == 4 {
        (off[..2].parse::<i64>().ok()?, off[2..].parse::<i64>().ok()?)
    } else {
        return None;
    };
    Some((h * 3600 + m * 60) as f64)
}

fn val_to_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn val_to_i64(v: Option<&Value>) -> Option<i64> {
    let val = v?;
    if let Some(n) = val.as_i64() {
        return Some(n);
    }
    if let Some(f) = val.as_f64() {
        if f.is_finite() {
            return Some(f.round() as i64);
        }
    }
    if let Some(s) = val.as_str() {
        if let Ok(n) = s.trim().parse::<i64>() {
            return Some(n);
        }
        if let Ok(f) = s.trim().parse::<f64>() {
            if f.is_finite() {
                return Some(f.round() as i64);
            }
        }
    }
    None
}

fn parse_props_map(v: Option<&Value>) -> Map<String, Value> {
    match v {
        Some(Value::Object(m)) => m.clone(),
        Some(Value::String(s)) => {
            if let Some((Value::Object(m), _)) = parse_or_repair_json(s) {
                return m;
            }
            Map::new()
        }
        _ => Map::new(),
    }
}

fn normalize_row_object(obj: &Map<String, Value>, fallback_idx: usize) -> NormalizedRow {
    let ts = parse_timestamp_secs(
        obj.get("TimeGenerated")
            .or_else(|| obj.get("timestamp"))
            .or_else(|| obj.get("ts"))
            .or_else(|| obj.get("time")),
    )
    .unwrap_or(1_700_000_000.0 + (fallback_idx as f64) * 0.1);

    let mut props = parse_props_map(
        obj.get("Properties")
            .or_else(|| obj.get("customDimensions"))
            .or_else(|| obj.get("properties"))
            .or_else(|| obj.get("attributes")),
    );
    // Also copy top-level gen_ai.* or llm.* keys into props
    for (k, v) in obj {
        if k.starts_with("gen_ai.")
            || k.starts_with("llm.")
            || matches!(
                k.as_str(),
                "request_body"
                    | "response_body"
                    | "RequestBody"
                    | "ResponseBody"
                    | "request"
                    | "response"
                    | "event.name"
            )
        {
            props.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    let message = val_to_string(
        obj.get("Message")
            .or_else(|| obj.get("message"))
            .or_else(|| obj.get("body")),
    );
    let name = val_to_string(obj.get("Name").or_else(|| obj.get("name")));
    let target = val_to_string(obj.get("Target").or_else(|| obj.get("target")));

    let explicit_kind = val_to_string(
        obj.get("RecordKind")
            .or_else(|| obj.get("itemType"))
            .or_else(|| obj.get("Type")),
    )
    .to_ascii_lowercase();

    let record_kind = if explicit_kind.contains("trace") || explicit_kind.contains("event") {
        "trace".to_string()
    } else if explicit_kind.contains("dep") || explicit_kind.contains("req") || explicit_kind.contains("span") {
        "dependency".to_string()
    } else if !message.is_empty()
        && (message.starts_with("[TOKPROF_")
            || message.starts_with("gen_ai.")
            || props.contains_key("gen_ai.prompt")
            || props.contains_key("gen_ai.completion")
            || props.get("event.name").is_some())
        && name.is_empty()
    {
        "trace".to_string()
    } else {
        "dependency".to_string()
    };

    let operation_id = val_to_string(
        obj.get("OperationId")
            .or_else(|| obj.get("operation_Id"))
            .or_else(|| obj.get("TraceId"))
            .or_else(|| obj.get("trace_id"))
            .or_else(|| props.get("gen_ai.conversation.id"))
            .or_else(|| props.get("session.id")),
    );
    let span_id = val_to_string(
        obj.get("SpanId")
            .or_else(|| obj.get("Id"))
            .or_else(|| obj.get("id"))
            .or_else(|| obj.get("span_id")),
    );
    let parent_id = val_to_string(
        obj.get("ParentId")
            .or_else(|| obj.get("operation_ParentId"))
            .or_else(|| obj.get("parent_id")),
    );
    let result_code = val_to_i64(
        obj.get("ResultCode")
            .or_else(|| obj.get("resultCode"))
            .or_else(|| obj.get("status"))
            .or_else(|| props.get("http.response.status_code"))
            .or_else(|| props.get("http.status_code")),
    );
    let duration_ms = val_to_i64(
        obj.get("DurationMs")
            .or_else(|| obj.get("duration"))
            .or_else(|| obj.get("latency_ms")),
    );
    let item_count = val_to_i64(obj.get("ItemCount").or_else(|| obj.get("itemCount")))
        .unwrap_or(1)
        .max(1);
    let ingestion_lag_ms = parse_timestamp_secs(
        obj.get("IngestionTime")
            .or_else(|| obj.get("_ingestion_time"))
            .or_else(|| obj.get("ingestion_time")),
    )
    .map(|ing_ts| ((ing_ts - ts) * 1000.0).round() as i64)
    .filter(|&lag| lag >= 0);

    NormalizedRow {
        ts,
        ingestion_lag_ms,
        record_kind,
        operation_id,
        span_id,
        parent_id,
        name,
        target,
        result_code,
        duration_ms,
        message,
        props,
        item_count,
    }
}

#[derive(Default)]
struct TableColMap {
    ts: Option<usize>,
    ingest_ts: Option<usize>,
    kind: Option<usize>,
    op_id: Option<usize>,
    span_id: Option<usize>,
    parent_id: Option<usize>,
    name: Option<usize>,
    target: Option<usize>,
    result_code: Option<usize>,
    duration_ms: Option<usize>,
    message: Option<usize>,
    props: Option<usize>,
    item_count: Option<usize>,
}

impl TableColMap {
    fn from_columns(cols: &[Value]) -> Self {
        let names: Vec<&str> = cols
            .iter()
            .map(|c| {
                c.as_object()
                    .and_then(|co| co.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
            })
            .collect();
        Self::from_header_names(&names)
    }

    fn from_header_names(names: &[&str]) -> Self {
        let mut m = Self::default();
        for (idx, &cname_raw) in names.iter().enumerate() {
            let cname = cname_raw.trim().trim_matches('"');
            match cname {
                "TimeGenerated" | "timestamp" | "ts" | "time" | "TimeGenerated [UTC]" => {
                    m.ts = Some(idx)
                }
                "IngestionTime" | "_ingestion_time" | "ingestion_time" => m.ingest_ts = Some(idx),
                "RecordKind" | "itemType" | "Type" => m.kind = Some(idx),
                "OperationId" | "operation_Id" | "TraceId" | "trace_id" => m.op_id = Some(idx),
                "SpanId" | "Id" | "id" | "span_id" => m.span_id = Some(idx),
                "ParentId" | "operation_ParentId" | "parent_id" => m.parent_id = Some(idx),
                "Name" | "name" => m.name = Some(idx),
                "Target" | "target" => m.target = Some(idx),
                "ResultCode" | "resultCode" | "status" => m.result_code = Some(idx),
                "DurationMs" | "duration" | "latency_ms" => m.duration_ms = Some(idx),
                "Message" | "message" | "body" => m.message = Some(idx),
                "Properties" | "customDimensions" | "properties" | "attributes" => {
                    m.props = Some(idx)
                }
                "ItemCount" | "itemCount" => m.item_count = Some(idx),
                _ => {}
            }
        }
        m
    }

    #[inline]
    fn cell<'a>(&self, cells: &'a [Value], idx: Option<usize>) -> Option<&'a Value> {
        cells.get(idx?)
    }

    fn normalize_cells(&self, cells: &[Value], fallback_idx: usize) -> NormalizedRow {
        let ts = parse_timestamp_secs(self.cell(cells, self.ts))
            .unwrap_or(1_700_000_000.0 + (fallback_idx as f64) * 0.1);
        let props = parse_props_map(self.cell(cells, self.props));
        let message = val_to_string(self.cell(cells, self.message));
        let name = val_to_string(self.cell(cells, self.name));
        let target = val_to_string(self.cell(cells, self.target));
        let explicit_kind = val_to_string(self.cell(cells, self.kind)).to_ascii_lowercase();
        let record_kind = if explicit_kind.contains("trace") || explicit_kind.contains("event") {
            "trace".to_string()
        } else if explicit_kind.contains("dep")
            || explicit_kind.contains("req")
            || explicit_kind.contains("span")
        {
            "dependency".to_string()
        } else if !message.is_empty()
            && (message.starts_with("[TOKPROF_")
                || message.starts_with("gen_ai.")
                || props.contains_key("gen_ai.prompt")
                || props.contains_key("gen_ai.completion")
                || props.get("event.name").is_some())
            && name.is_empty()
        {
            "trace".to_string()
        } else {
            "dependency".to_string()
        };
        let mut operation_id = val_to_string(self.cell(cells, self.op_id));
        if operation_id.is_empty() {
            operation_id = val_to_string(
                props
                    .get("gen_ai.conversation.id")
                    .or_else(|| props.get("session.id")),
            );
        }
        let span_id = val_to_string(self.cell(cells, self.span_id));
        let parent_id = val_to_string(self.cell(cells, self.parent_id));
        let result_code = val_to_i64(
            self.cell(cells, self.result_code)
                .or_else(|| props.get("http.response.status_code"))
                .or_else(|| props.get("http.status_code")),
        );
        let duration_ms = val_to_i64(self.cell(cells, self.duration_ms));
        let item_count = val_to_i64(self.cell(cells, self.item_count))
            .unwrap_or(1)
            .max(1);
        let ingestion_lag_ms = parse_timestamp_secs(self.cell(cells, self.ingest_ts))
            .map(|ing_ts| ((ing_ts - ts) * 1000.0).round() as i64)
            .filter(|&lag| lag >= 0);

        NormalizedRow {
            ts,
            ingestion_lag_ms,
            record_kind,
            operation_id,
            span_id,
            parent_id,
            name,
            target,
            result_code,
            duration_ms,
            message,
            props,
            item_count,
        }
    }
}

fn extract_normalized_rows_from_json_value(val: &Value) -> Vec<NormalizedRow> {
    let mut out = Vec::new();
    match val {
        Value::Object(map) => {
            if let Some(Value::Array(tables)) = map.get("tables") {
                for tbl in tables {
                    let Some(tbl_obj) = tbl.as_object() else {
                        continue;
                    };
                    let Some(Value::Array(cols)) = tbl_obj.get("columns") else {
                        continue;
                    };
                    let cmap = TableColMap::from_columns(cols);
                    if let Some(Value::Array(raw_rows)) = tbl_obj.get("rows") {
                        out.reserve(raw_rows.len());
                        if raw_rows.len() >= 1024 {
                            let n_workers = std::thread::available_parallelism()
                                .map(|n| n.get())
                                .unwrap_or(4)
                                .clamp(2, 6)
                                .min(raw_rows.len());
                            let chunk_sz = raw_rows.len().div_ceil(n_workers);
                            let cmap_ref = &cmap;
                            let chunks_out: Vec<Vec<NormalizedRow>> = std::thread::scope(|s| {
                                let mut handles = Vec::with_capacity(n_workers);
                                for (c_idx, slice) in raw_rows.chunks(chunk_sz).enumerate() {
                                    let base_idx = c_idx * chunk_sz;
                                    handles.push(s.spawn(move || {
                                        let mut local = Vec::with_capacity(slice.len());
                                        for (off, r) in slice.iter().enumerate() {
                                            if let Value::Array(cells) = r {
                                                local.push(
                                                    cmap_ref.normalize_cells(cells, base_idx + off),
                                                );
                                            } else if let Value::Object(ro) = r {
                                                local.push(normalize_row_object(
                                                    ro,
                                                    base_idx + off,
                                                ));
                                            }
                                        }
                                        local
                                    }));
                                }
                                handles
                                    .into_iter()
                                    .filter_map(|h| h.join().ok())
                                    .collect()
                            });
                            for ch in chunks_out {
                                out.extend(ch);
                            }
                        } else {
                            for (idx, r) in raw_rows.iter().enumerate() {
                                if let Value::Array(cells) = r {
                                    out.push(cmap.normalize_cells(cells, idx));
                                } else if let Value::Object(ro) = r {
                                    out.push(normalize_row_object(ro, idx));
                                }
                            }
                        }
                    }
                }
                return out;
            }
            if let Some(Value::Array(arr)) = map.get("value") {
                out.reserve(arr.len());
                for (idx, item) in arr.iter().enumerate() {
                    if let Value::Object(ro) = item {
                        out.push(normalize_row_object(ro, idx));
                    }
                }
                return out;
            }
            out.push(normalize_row_object(map, 0));
        }
        Value::Array(arr) => {
            out.reserve(arr.len());
            for (idx, item) in arr.iter().enumerate() {
                if let Value::Object(ro) = item {
                    out.push(normalize_row_object(ro, idx));
                }
            }
        }
        _ => {}
    }
    out
}

/// Parses an RFC-4180 CSV export from Azure Portal (Log Analytics / Application Insights) into `NormalizedRow`s.
fn try_parse_csv_export(raw: &str) -> Option<Vec<NormalizedRow>> {
    let first_line = raw.lines().next()?.trim();
    if !first_line.contains(',')
        || !(first_line.contains("TimeGenerated")
            || first_line.contains("timestamp")
            || first_line.contains("OperationId")
            || first_line.contains("operation_Id")
            || first_line.contains("Properties")
            || first_line.contains("customDimensions"))
    {
        return None;
    }

    let mut records: Vec<Vec<String>> = Vec::new();
    let mut cur_row: Vec<String> = Vec::new();
    let mut cur_field = String::new();
    let mut in_quotes = false;
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    let _ = chars.next();
                    cur_field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cur_field.push(ch);
            }
        } else {
            match ch {
                '"' => in_quotes = true,
                ',' => {
                    cur_row.push(std::mem::take(&mut cur_field));
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        let _ = chars.next();
                    }
                    cur_row.push(std::mem::take(&mut cur_field));
                    if cur_row.iter().any(|f| !f.trim().is_empty()) {
                        records.push(std::mem::take(&mut cur_row));
                    } else {
                        cur_row.clear();
                    }
                }
                '\n' => {
                    cur_row.push(std::mem::take(&mut cur_field));
                    if cur_row.iter().any(|f| !f.trim().is_empty()) {
                        records.push(std::mem::take(&mut cur_row));
                    } else {
                        cur_row.clear();
                    }
                }
                _ => cur_field.push(ch),
            }
        }
    }
    if !cur_field.is_empty() || !cur_row.is_empty() {
        cur_row.push(cur_field);
        if cur_row.iter().any(|f| !f.trim().is_empty()) {
            records.push(cur_row);
        }
    }
    if records.len() < 2 {
        return None;
    }
    let header_refs: Vec<&str> = records[0].iter().map(|s| s.as_str()).collect();
    let cmap = TableColMap::from_header_names(&header_refs);
    let mut out = Vec::with_capacity(records.len() - 1);
    for (idx, row_strs) in records.into_iter().skip(1).enumerate() {
        let cells: Vec<Value> = row_strs.into_iter().map(Value::String).collect();
        out.push(cmap.normalize_cells(&cells, idx));
    }
    Some(out)
}

fn extract_rows_from_json_value(val: &Value) -> Vec<Map<String, Value>> {
    let mut rows = Vec::new();
    match val {
        Value::Object(map) => {
            // 1. Standard Log Analytics / App Insights `tables` response
            if let Some(Value::Array(tables)) = map.get("tables") {
                for tbl in tables {
                    let Some(tbl_obj) = tbl.as_object() else {
                        continue;
                    };
                    let Some(Value::Array(cols)) = tbl_obj.get("columns") else {
                        continue;
                    };
                    let col_names: Vec<String> = cols
                        .iter()
                        .map(|c| {
                            c.as_object()
                                .and_then(|co| co.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string()
                        })
                        .collect();
                    if let Some(Value::Array(raw_rows)) = tbl_obj.get("rows") {
                        for r in raw_rows {
                            if let Value::Array(cells) = r {
                                let mut row_map = Map::new();
                                for (idx, cell) in cells.iter().enumerate() {
                                    if let Some(cname) = col_names.get(idx).filter(|s| !s.is_empty()) {
                                        row_map.insert(cname.clone(), cell.clone());
                                    }
                                }
                                rows.push(row_map);
                            } else if let Value::Object(ro) = r {
                                rows.push(ro.clone());
                            }
                        }
                    }
                }
                return rows;
            }
            // 2. `{ "value": [ ... ] }`
            if let Some(Value::Array(arr)) = map.get("value") {
                for item in arr {
                    if let Value::Object(ro) = item {
                        rows.push(ro.clone());
                    }
                }
                return rows;
            }
            // 3. Single row object
            rows.push(map.clone());
        }
        Value::Array(arr) => {
            for item in arr {
                if let Value::Object(ro) = item {
                    rows.push(ro.clone());
                }
            }
        }
        _ => {}
    }
    rows
}

fn try_decode_chunk_bridge(
    chunks_by_id: &BTreeMap<String, ChunkAssembly>,
    opts: &IngestOptions,
    detector: &Detector,
    stats: &mut IngestStats,
) -> Vec<CallRecord> {
    let mut out = Vec::new();
    for (cid, asm) in chunks_by_id {
        if asm.total == 0 || asm.parts.len() != asm.total {
            continue;
        }
        let mut b64 = String::new();
        for seq in 0..asm.total {
            if let Some(p) = asm.parts.get(&seq) {
                b64.push_str(p.trim());
            }
        }
        let Ok(compressed) = base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()) else {
            continue;
        };
        let mut dec = GzDecoder::new(&compressed[..]);
        let mut json_str = String::new();
        let payload_str = if dec.read_to_string(&mut json_str).is_ok() && !json_str.is_empty() {
            json_str
        } else {
            String::from_utf8_lossy(&compressed).into_owned()
        };
        let Ok(val) = serde_json::from_str::<Value>(&payload_str) else {
            continue;
        };
        if let Some(mut rec) = payload_to_call_record(&val, opts, detector, false) {
            if rec.id.is_empty() {
                rec.id = cid.clone();
            }
            rec.detect = Some(json!({
                "score": 100,
                "source": "tokprof_chunk_bridge",
                "evidence": ["lossless_chunk_bridge"],
            }));
            stats.lossless_chunk_calls += 1;
            stats.tier1_full_calls += 1;
            out.push(rec);
        }
    }
    out
}

fn payload_to_call_record(
    msg: &Value,
    opts: &IngestOptions,
    detector: &Detector,
    was_repaired: bool,
) -> Option<CallRecord> {
    let obj = msg.as_object()?;
    let path = obj
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("/v1/chat/completions");
    let host = obj
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or("openai.azure.com");
    let port = obj
        .get("port")
        .and_then(|v| v.as_u64())
        .map(|n| n as u16)
        .unwrap_or(443);

    let (req_val, req_repaired) = match obj.get("request") {
        Some(Value::Object(m)) => (Value::Object(m.clone()), false),
        Some(Value::String(s)) => {
            if let Ok(v) = serde_json::from_str::<Value>(s) {
                (v, false)
            } else {
                let (rep, rflag) = repair_truncated_json(s);
                (serde_json::from_str::<Value>(&rep).ok()?, rflag)
            }
        }
        _ => return None,
    };

    let mut verdict = detector.inspect("POST", host, port, path, Some(&req_val));
    let api = verdict
        .api
        .clone()
        .unwrap_or_else(|| OPENAI_CHAT.to_string());
    let mut parsed = parse_request(&api, &req_val).ok()?;
    if parsed.model.is_none() {
        parsed.model = verdict
            .model_hint
            .clone()
            .or_else(|| obj.get("model").and_then(|v| v.as_str()).map(|s| s.to_string()));
    }
    if parsed.conv_hint.is_none() {
        parsed.conv_hint = obj
            .get("conv")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
    }

    let resp_text = match obj.get("response") {
        Some(Value::String(s)) => s.clone(),
        Some(v @ Value::Object(_)) => v.to_string(),
        _ => String::new(),
    };
    let sse = obj.get("sse").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut response = if !resp_text.is_empty() {
        let mut r = parse_response_body(&parsed.provider, &resp_text, sse);
        if !r.has_usage && r.text.is_empty() && !sse {
            let (rep_resp, _) = repair_truncated_json(&resp_text);
            r = parse_response_body(&parsed.provider, &rep_resp, false);
        }
        r
    } else {
        ParsedResponse::default()
    };

    // Merge explicit usage overrides if response body lacked usage
    if !response.has_usage {
        if let Some(u_obj) = obj.get("usage").and_then(|v| v.as_object()) {
            let in_t = val_to_i64(u_obj.get("input_total").or_else(|| u_obj.get("prompt_tokens")).or_else(|| u_obj.get("input_tokens")));
            let out_t = val_to_i64(u_obj.get("output").or_else(|| u_obj.get("completion_tokens")).or_else(|| u_obj.get("output_tokens")));
            let cached_t = val_to_i64(u_obj.get("cached").or_else(|| u_obj.get("cached_tokens")));
            let reas_t = val_to_i64(u_obj.get("reasoning").or_else(|| u_obj.get("reasoning_tokens")));
            if in_t.is_some() || out_t.is_some() {
                response.usage = Usage {
                    input_total: in_t,
                    cached: cached_t,
                    cache_write: None,
                    output: out_t,
                    reasoning: reas_t,
                };
                response.has_usage = true;
            }
        }
    }

    let counter = get_counter(parsed.model.as_deref(), opts.use_tiktoken);
    let ts = obj
        .get("ts")
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0)
        });
    let status = val_to_i64(obj.get("status"));
    let latency_ms = val_to_i64(obj.get("latency_ms"));
    let ttfb_ms = val_to_i64(obj.get("ttfb_ms"));
    let err = obj
        .get("error")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut rec = build_record(BuildRecordArgs {
        parsed: &parsed,
        response: Some(&response),
        counter: &counter,
        salt: &opts.salt,
        components: Some(&opts.components),
        endpoint: path,
        status,
        latency_ms,
        ttfb_ms,
        run: opts.run_id.clone(),
        error: err,
        ts: Some(ts),
        include_text: opts.include_text,
    });
    if verdict.host_class.is_empty() {
        verdict.host_class = classify_host(host, port).cls.to_string();
    }
    rec.host_class = Some(verdict.host_class);
    rec.host_id = Some(hash_text(&format!("{host}:{port}"), &opts.salt));
    if was_repaired || req_repaired {
        rec.detect = Some(json!({
            "score": verdict.score,
            "source": "app_insights_tier1",
            "truncated_repaired": true,
        }));
    }
    Some(rec)
}

fn handle_bridge_message_line(
    line: &str,
    chunks_by_id: &mut BTreeMap<String, ChunkAssembly>,
    direct_events: &mut Vec<Value>,
) -> bool {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("[TOKPROF_CHUNK]") {
        if let Ok(obj) = serde_json::from_str::<Value>(rest.trim()) {
            let id = val_to_string(obj.get("id"));
            let seq = val_to_i64(obj.get("seq")).unwrap_or(0) as usize;
            let total = val_to_i64(obj.get("total")).unwrap_or(0) as usize;
            let data = val_to_string(obj.get("data"));
            if !id.is_empty() && total > 0 && !data.is_empty() {
                let entry = chunks_by_id.entry(id).or_default();
                entry.total = total;
                entry.parts.insert(seq, data);
                return true;
            }
        }
    } else if let Some(rest) = trimmed.strip_prefix("[TOKPROF_EVENT]") {
        if let Ok(obj) = serde_json::from_str::<Value>(rest.trim()) {
            direct_events.push(obj);
            return true;
        }
    }
    false
}

fn extract_partial_error(val: &Value) -> Option<String> {
    let obj = val.as_object()?;
    let err = obj.get("error").and_then(|v| v.as_object())?;
    let code = val_to_string(err.get("code"));
    let msg = val_to_string(err.get("message"));
    let inner = err
        .get("details")
        .and_then(|d| d.as_array())
        .and_then(|a| a.first())
        .and_then(|first| first.as_object())
        .map(|o| {
            format!(
                "{}: {}",
                val_to_string(o.get("code")),
                val_to_string(o.get("message"))
            )
        })
        .unwrap_or_default();
    if code.is_empty() && msg.is_empty() {
        return None;
    }
    if inner.is_empty() {
        Some(format!("{code}: {msg}"))
    } else {
        Some(format!("{code}: {msg} ({inner})"))
    }
}

pub fn parse_app_insights_payload(
    raw_text: &str,
    opts: &IngestOptions,
) -> Result<(Vec<CallRecord>, IngestStats), String> {
    let mut stats = IngestStats::default();
    let detector = Detector::default();
    let mut chunks_by_id: BTreeMap<String, ChunkAssembly> = BTreeMap::new();
    let mut direct_events: Vec<Value> = Vec::new();

    let trimmed = raw_text.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok((Vec::new(), stats));
    }

    // Overlap cold tiktoken BPE initialization (~80ms) in the background while parsing JSON/CSV rows
    if opts.use_tiktoken && trimmed.len() >= 4096 {
        let _ = std::thread::spawn(|| {
            let _ = get_counter(Some("gpt-4o"), true);
        });
    }

    let mut direct_records: Vec<CallRecord> = Vec::new();
    let mut normalized_rows: Vec<NormalizedRow> = Vec::new();
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<Value>(trimmed).is_ok()
    {
        let val = serde_json::from_str::<Value>(trimmed).unwrap();
        stats.partial_error_warning = extract_partial_error(&val);
        let extracted = extract_normalized_rows_from_json_value(&val);
        stats.total_rows += extracted.len();
        for nr in extracted {
            if handle_bridge_message_line(&nr.message, &mut chunks_by_id, &mut direct_events) {
                continue;
            }
            normalized_rows.push(nr);
        }
    } else if let Some(csv_rows) = try_parse_csv_export(trimmed) {
        stats.total_rows += csv_rows.len();
        for nr in csv_rows {
            if handle_bridge_message_line(&nr.message, &mut chunks_by_id, &mut direct_events) {
                continue;
            }
            normalized_rows.push(nr);
        }
    } else {
        // Line-by-line NDJSON, existing tokprof JSONL, or container console logs
        for (idx, line) in trimmed.lines().enumerate() {
            let ln = line.trim();
            if ln.is_empty() {
                continue;
            }
            if handle_bridge_message_line(ln, &mut chunks_by_id, &mut direct_events) {
                stats.total_rows += 1;
                continue;
            }
            if ln.contains("\"segments\"") {
                if let Ok(mut rec) = serde_json::from_str::<CallRecord>(ln) {
                    stats.total_rows += 1;
                    if opts.usage_only {
                        let rep_in = rec
                            .usage
                            .get("input_total")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(rec.est_input);
                        rec.segments.clear();
                        rec.est_input = 0;
                        rec.residual = Some(rep_in);
                        rec.quality = Some("usage_only".to_string());
                        rec.response_text = None;
                        stats.tier2_usage_only_calls += 1;
                    } else {
                        if opts.include_text == Some(false) {
                            rec.response_text = None;
                            for s in &mut rec.segments {
                                s.text = None;
                            }
                        }
                        if rec.quality.as_deref() == Some("usage_only") || rec.segments.is_empty() {
                            stats.tier2_usage_only_calls += 1;
                        } else {
                            stats.tier1_full_calls += 1;
                        }
                    }
                    direct_records.push(rec);
                    continue;
                }
            }
            if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(ln) {
                stats.total_rows += 1;
                let nr = normalize_row_object(&m, idx);
                if handle_bridge_message_line(&nr.message, &mut chunks_by_id, &mut direct_events) {
                    continue;
                }
                normalized_rows.push(nr);
            }
        }
    }

    let mut records = direct_records;
    for ev in &direct_events {
        if let Some(mut rec) = payload_to_call_record(ev, opts, &detector, false) {
            rec.detect = Some(json!({
                "score": 100,
                "source": "tokprof_event_bridge",
            }));
            stats.lossless_chunk_calls += 1;
            stats.tier1_full_calls += 1;
            records.push(rec);
        }
    }
    records.extend(try_decode_chunk_bridge(
        &chunks_by_id,
        opts,
        &detector,
        &mut stats,
    ));

    // Group remaining AppDependencies and AppTraces by (OperationId, SpanId)
    let mut raw_dep_rows: Vec<NormalizedRow> = Vec::new();
    let mut traces_by_parent: HashMap<(String, String), Vec<NormalizedRow>> = HashMap::new();
    let mut traces_by_op: HashMap<String, Vec<NormalizedRow>> = HashMap::new();
    let mut lag_samples: Vec<i64> = Vec::new();

    for nr in normalized_rows {
        if let Some(lag) = nr.ingestion_lag_ms {
            lag_samples.push(lag);
        }
        if let Some(ref cf) = opts.conv_filter {
            if !cf.is_empty() && nr.operation_id != *cf {
                let conv_prop = val_to_string(
                    nr.props
                        .get("gen_ai.conversation.id")
                        .or_else(|| nr.props.get("session.id")),
                );
                if conv_prop != *cf {
                    continue;
                }
            }
        }
        if nr.record_kind == "trace" {
            if !nr.parent_id.is_empty() {
                traces_by_parent
                    .entry((nr.operation_id.clone(), nr.parent_id.clone()))
                    .or_default()
                    .push(nr.clone());
            }
            traces_by_op
                .entry(nr.operation_id.clone())
                .or_default()
                .push(nr);
        } else {
            raw_dep_rows.push(nr);
        }
    }

    // Deduplicate dependency spans with identical (OperationId, SpanId) across multi-replica / APIM+SDK / retry exports
    let mut dep_rows: Vec<NormalizedRow> = Vec::with_capacity(raw_dep_rows.len());
    let mut dep_idx_by_key: HashMap<(String, String), usize> = HashMap::new();
    for nr in raw_dep_rows {
        if !nr.operation_id.is_empty() && !nr.span_id.is_empty() {
            let key = (nr.operation_id.clone(), nr.span_id.clone());
            if let Some(&existing_idx) = dep_idx_by_key.get(&key) {
                stats.deduplicated_spans += 1;
                let existing = &mut dep_rows[existing_idx];
                existing.item_count = existing.item_count.max(nr.item_count);
                if existing.ingestion_lag_ms.is_none() {
                    existing.ingestion_lag_ms = nr.ingestion_lag_ms;
                }
                for (k, v) in nr.props {
                    existing.props.entry(k).or_insert(v);
                }
                continue;
            }
            dep_idx_by_key.insert(key, dep_rows.len());
        }
        dep_rows.push(nr);
    }

    if !lag_samples.is_empty() {
        lag_samples.sort_unstable();
        stats.max_ingestion_lag_ms = *lag_samples.last().unwrap_or(&0);
        stats.median_ingestion_lag_ms = lag_samples[lag_samples.len() / 2];
    }

    // If there are only trace events and no dependency spans, synthesize one pseudo-dependency per OperationId/ParentId
    if dep_rows.is_empty() && !traces_by_op.is_empty() {
        for (op_id, evs) in &traces_by_op {
            if let Some(first) = evs.first() {
                let mut pseudo = first.clone();
                pseudo.record_kind = "dependency".to_string();
                pseudo.operation_id = op_id.clone();
                pseudo.span_id = if !first.parent_id.is_empty() {
                    first.parent_id.clone()
                } else {
                    format!("span-{op_id}")
                };
                dep_rows.push(pseudo);
            }
        }
    }

    let single_dep_ops: HashMap<String, usize> = {
        let mut counts = HashMap::new();
        for d in &dep_rows {
            *counts.entry(d.operation_id.clone()).or_insert(0usize) += 1;
        }
        counts
    };

    let mut work_items: Vec<(NormalizedRow, Vec<NormalizedRow>)> =
        Vec::with_capacity(dep_rows.len());
    for dep in dep_rows {
        let mut events = traces_by_parent
            .remove(&(dep.operation_id.clone(), dep.span_id.clone()))
            .unwrap_or_default();
        if events.is_empty() && single_dep_ops.get(&dep.operation_id) == Some(&1) {
            events = traces_by_op
                .remove(&dep.operation_id)
                .unwrap_or_default();
        }
        events.sort_by(|a, b| a.ts.total_cmp(&b.ts));
        work_items.push((dep, events));
    }

    if work_items.len() >= 16 {
        // Ensure global tiktoken BPE Lazy is warm before spawning parallel workers
        let _ = get_counter(Some("gpt-4o"), opts.use_tiktoken);
        let n_workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(2, 8)
            .min(work_items.len());
        let chunk_sz = work_items.len().div_ceil(n_workers);
        let det_ref = &detector;
        let partial_results: Vec<(Vec<CallRecord>, IngestStats)> = std::thread::scope(|s| {
            let mut handles = Vec::with_capacity(n_workers);
            for slice in work_items.chunks(chunk_sz) {
                handles.push(s.spawn(move || {
                    let mut local_recs = Vec::with_capacity(slice.len());
                    let mut local_st = IngestStats::default();
                    for (dep, events) in slice {
                        if let Some(rec) = build_record_from_span_and_events(
                            dep,
                            events,
                            opts,
                            det_ref,
                            &mut local_st,
                        ) {
                            local_recs.push(rec);
                        }
                    }
                    (local_recs, local_st)
                }));
            }
            handles
                .into_iter()
                .filter_map(|h| h.join().ok())
                .collect()
        });
        for (l_recs, l_st) in partial_results {
            records.extend(l_recs);
            stats.tier1_full_calls += l_st.tier1_full_calls;
            stats.tier2_usage_only_calls += l_st.tier2_usage_only_calls;
            stats.truncated_repaired_calls += l_st.truncated_repaired_calls;
            stats.sampled_calls += l_st.sampled_calls;
            stats.max_item_count = stats.max_item_count.max(l_st.max_item_count);
        }
    } else {
        for (dep, events) in &work_items {
            if let Some(rec) = build_record_from_span_and_events(
                dep,
                events,
                opts,
                &detector,
                &mut stats,
            ) {
                records.push(rec);
            }
        }
    }

    records.sort_by(|a, b| a.ts.total_cmp(&b.ts));
    stats.pending_late_spans = records
        .iter()
        .filter(|r| r.quality.as_deref() == Some("usage_only"))
        .count();
    Ok((records, stats))
}

/// Merges `incoming` records into `existing` records (keyed by `(conv, id)`).
/// If an existing record was `usage_only` (because `AppDependencies` arrived before `AppTraces` due to
/// Log Analytics ingestion lag) and the incoming record has full segments (`tier1_full`), upgrades it in-place.
pub fn merge_and_upgrade_records(
    existing: &[CallRecord],
    incoming: Vec<CallRecord>,
) -> (Vec<CallRecord>, usize) {
    if existing.is_empty() {
        return (incoming, 0);
    }
    let mut out: Vec<CallRecord> = existing.to_vec();
    let mut idx_by_key: HashMap<String, usize> = HashMap::with_capacity(out.len());
    for (idx, r) in out.iter().enumerate() {
        if !r.id.is_empty() {
            idx_by_key.insert(r.id.clone(), idx);
        }
    }
    let mut upgraded = 0usize;
    for inc in incoming {
        if !inc.id.is_empty() {
            if let Some(&pos) = idx_by_key.get(&inc.id) {
                let prev_usage_only = out[pos].quality.as_deref() == Some("usage_only")
                    || out[pos].segments.is_empty();
                let inc_has_segments = !inc.segments.is_empty()
                    && inc.quality.as_deref() != Some("usage_only");
                if prev_usage_only && inc_has_segments {
                    out[pos] = inc;
                    upgraded += 1;
                }
                continue;
            }
            idx_by_key.insert(inc.id.clone(), out.len());
        }
        out.push(inc);
    }
    out.sort_by(|a, b| a.ts.total_cmp(&b.ts));
    (out, upgraded)
}

fn build_record_from_span_and_events(
    dep: &NormalizedRow,
    events: &[NormalizedRow],
    opts: &IngestOptions,
    detector: &Detector,
    stats: &mut IngestStats,
) -> Option<CallRecord> {
    let mut max_item_count = dep.item_count;
    let mut max_lag_ms = dep.ingestion_lag_ms;
    for ev in events {
        max_item_count = max_item_count.max(ev.item_count);
        if let Some(l) = ev.ingestion_lag_ms {
            max_lag_ms = Some(max_lag_ms.map_or(l, |prev| prev.max(l)));
        }
    }
    if max_item_count > 1 {
        stats.sampled_calls += 1;
        stats.max_item_count = stats.max_item_count.max(max_item_count);
    }

    let props = &dep.props;
    let gen_system = val_to_string(
        props
            .get("gen_ai.system")
            .or_else(|| props.get("gen_ai.provider.name"))
            .or_else(|| props.get("llm.system"))
            .or_else(|| props.get("llm.provider")),
    )
    .to_ascii_lowercase();

    let model = val_to_string(
        props
            .get("gen_ai.response.model")
            .or_else(|| props.get("gen_ai.request.model"))
            .or_else(|| props.get("llm.model_name")),
    );
    let model_opt = if !model.is_empty() {
        Some(model)
    } else {
        extract_model_from_span_name(&dep.name)
    };

    let conv_id = val_to_string(
        props
            .get("gen_ai.conversation.id")
            .or_else(|| props.get("session.id"))
            .or_else(|| props.get("thread.id")),
    );
    let conv_opt = if !conv_id.is_empty() {
        Some(conv_id)
    } else if !dep.operation_id.is_empty() {
        Some(dep.operation_id.clone())
    } else {
        None
    };

    let (host, port, endpoint_path) = resolve_target_endpoint(&dep.target, &gen_system);

    let input_tok = val_to_i64(
        props
            .get("gen_ai.usage.input_tokens")
            .or_else(|| props.get("gen_ai.usage.prompt_tokens"))
            .or_else(|| props.get("llm.token_count.prompt")),
    );
    let output_tok = val_to_i64(
        props
            .get("gen_ai.usage.output_tokens")
            .or_else(|| props.get("gen_ai.usage.completion_tokens"))
            .or_else(|| props.get("llm.token_count.completion")),
    );
    let cached_tok = val_to_i64(
        props
            .get("gen_ai.usage.cached_tokens")
            .or_else(|| props.get("gen_ai.usage.input_tokens_details.cached_tokens"))
            .or_else(|| props.get("gen_ai.usage.cache_read_input_tokens")),
    );
    let cache_write_tok = val_to_i64(
        props
            .get("gen_ai.usage.cache_creation_input_tokens")
            .or_else(|| props.get("gen_ai.usage.cache_write_tokens")),
    );
    let reasoning_tok = val_to_i64(
        props
            .get("gen_ai.usage.reasoning_tokens")
            .or_else(|| props.get("gen_ai.usage.output_tokens_details.reasoning_tokens")),
    );

    // 1. Check if raw request JSON body is directly present (e.g. APIM diagnostic logs or custom telemetry)
    let raw_req_str = val_to_string(
        props
            .get("request_body")
            .or_else(|| props.get("RequestBody"))
            .or_else(|| props.get("request"))
            .or_else(|| props.get("gen_ai.request.body"))
            .or_else(|| props.get("llm.request.body")),
    );
    let raw_resp_str = val_to_string(
        props
            .get("response_body")
            .or_else(|| props.get("ResponseBody"))
            .or_else(|| props.get("response"))
            .or_else(|| props.get("gen_ai.response.body"))
            .or_else(|| props.get("llm.response.body")),
    );

    let mut was_repaired = false;
    let mut synthesized_req: Option<Value> = None;
    let mut synthesized_resp: Option<String> = None;

    if !opts.usage_only && !raw_req_str.trim().is_empty() {
        if let Some((v, rflag)) = parse_or_repair_json(&raw_req_str) {
            if v.is_object() {
                synthesized_req = Some(v);
                was_repaired |= rflag;
            }
        }
        if !raw_resp_str.trim().is_empty() {
            let (rep_resp, rflag2) = repair_truncated_json(&raw_resp_str);
            synthesized_resp = Some(rep_resp);
            was_repaired |= rflag2;
        }
    }

    // 2. If no raw request body, reconstruct from OTel GenAI events + OpenInference attributes
    let mut choice_texts: Vec<String> = Vec::new();
    let mut choice_tool_calls: Vec<Value> = Vec::new();

    if !opts.usage_only && synthesized_req.is_none() {
        let (msgs, tools, resp_texts, resp_tcs, ev_repaired) =
            reconstruct_messages_and_tools(props, events);
        was_repaired |= ev_repaired;
        choice_texts = resp_texts;
        choice_tool_calls = resp_tcs;
        if !msgs.is_empty() || !tools.is_empty() {
            let mut req_obj = Map::new();
            if let Some(ref m) = model_opt {
                req_obj.insert("model".to_string(), Value::String(m.clone()));
            }
            if !msgs.is_empty() {
                req_obj.insert("messages".to_string(), Value::Array(msgs));
            }
            if !tools.is_empty() {
                req_obj.insert("tools".to_string(), Value::Array(tools));
            }
            if let Some(ref cid) = conv_opt {
                req_obj.insert("conversation_id".to_string(), Value::String(cid.clone()));
            }
            synthesized_req = Some(Value::Object(req_obj));
        }
    }

    if was_repaired {
        stats.truncated_repaired_calls += 1;
    }

    // Tier 1: Full segment breakdown when request messages/tools exist (direct in-memory build without serialize-reparse)
    if let Some(req_val) = synthesized_req {
        let mut verdict = detector.inspect("POST", &host, port, &endpoint_path, Some(&req_val));
        let api = verdict
            .api
            .clone()
            .unwrap_or_else(|| OPENAI_CHAT.to_string());
        if let Ok(mut parsed) = parse_request(&api, &req_val) {
            if parsed.model.is_none() {
                parsed.model = verdict.model_hint.clone().or_else(|| model_opt.clone());
            }
            if parsed.conv_hint.is_none() {
                parsed.conv_hint = conv_opt.clone();
            }
            let response = if let Some(ref r_str) = synthesized_resp {
                let mut r = parse_response_body(&parsed.provider, r_str, false);
                if !r.has_usage && (input_tok.is_some() || output_tok.is_some()) {
                    r.usage = Usage {
                        input_total: input_tok,
                        cached: cached_tok,
                        cache_write: cache_write_tok,
                        output: output_tok,
                        reasoning: reasoning_tok,
                    };
                    r.has_usage = true;
                }
                r
            } else {
                let mut tcs_names = Vec::with_capacity(choice_tool_calls.len());
                for tc in &choice_tool_calls {
                    if let Some(nm) = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .or_else(|| tc.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        if !nm.is_empty() {
                            tcs_names.push(nm.to_string());
                        }
                    }
                }
                let has_u = input_tok.is_some() || output_tok.is_some();
                ParsedResponse {
                    usage: Usage {
                        input_total: input_tok,
                        cached: cached_tok,
                        cache_write: cache_write_tok,
                        output: output_tok,
                        reasoning: reasoning_tok,
                    },
                    tool_calls: tcs_names,
                    text: choice_texts.join("\n"),
                    has_usage: has_u,
                    error: None,
                }
            };
            let counter = get_counter(parsed.model.as_deref(), opts.use_tiktoken);
            let mut rec = build_record(BuildRecordArgs {
                parsed: &parsed,
                response: Some(&response),
                counter: &counter,
                salt: &opts.salt,
                components: Some(&opts.components),
                endpoint: &endpoint_path,
                status: Some(dep.result_code.unwrap_or(200)),
                latency_ms: dep.duration_ms,
                ttfb_ms: None,
                run: opts.run_id.clone(),
                error: if dep.result_code.unwrap_or(200) >= 400 {
                    Some(format!("http_{}", dep.result_code.unwrap_or(500)))
                } else {
                    None
                },
                ts: Some(dep.ts),
                include_text: opts.include_text,
            });
            if verdict.host_class.is_empty() {
                verdict.host_class = classify_host(&host, port).cls.to_string();
            }
            rec.host_class = Some(verdict.host_class);
            rec.host_id = Some(hash_text(&format!("{host}:{port}"), &opts.salt));
            if !dep.span_id.is_empty() {
                rec.id = dep.span_id.chars().take(12).collect();
            }
            if rec.conv.is_none() {
                rec.conv = conv_opt.as_ref().map(|c| hash_text(c, &opts.salt));
            }
            rec.detect = Some(json!({
                "score": 90,
                "source": "app_insights_tier1",
                "truncated_repaired": if was_repaired { Value::Bool(true) } else { Value::Null },
                "sampled_item_count": if max_item_count > 1 { Value::from(max_item_count) } else { Value::Null },
                "ingestion_lag_ms": max_lag_ms,
            }));
            stats.tier1_full_calls += 1;
            return Some(rec);
        }
    }

    // Tier 2: Usage-Only fallback when Content Recording is disabled in Application Insights
    if input_tok.is_none() && output_tok.is_none() && model_opt.is_none() && gen_system.is_empty() {
        return None;
    }

    let provider = if gen_system.contains("anthropic") {
        ANTHROPIC.to_string()
    } else {
        OPENAI_CHAT.to_string()
    };
    let usage = Usage {
        input_total: input_tok,
        cached: cached_tok,
        cache_write: None,
        output: output_tok,
        reasoning: reasoning_tok,
    };
    let h_class = classify_host(&host, port).cls;
    let rec = CallRecord {
        v: SCHEMA_VERSION,
        id: if !dep.span_id.is_empty() {
            dep.span_id.chars().take(12).collect()
        } else {
            hash_text(&format!("{}:{}", dep.operation_id, dep.ts), &opts.salt)
                .chars()
                .take(12)
                .collect()
        },
        ts: dep.ts,
        run: opts.run_id.clone(),
        provider,
        model: model_opt,
        endpoint: endpoint_path,
        host_class: Some(h_class.to_string()),
        host_id: Some(hash_text(&format!("{host}:{port}"), &opts.salt)),
        host: None,
        detect: Some(json!({
            "score": 60,
            "source": "app_insights_usage_only",
            "sampled_item_count": if max_item_count > 1 { Value::from(max_item_count) } else { Value::Null },
            "ingestion_lag_ms": max_lag_ms,
        })),
        stream: false,
        status: Some(dep.result_code.unwrap_or(200)),
        latency_ms: dep.duration_ms,
        ttfb_ms: None,
        conv: conv_opt.as_ref().map(|c| hash_text(c, &opts.salt)),
        sysfp: None,
        sys_label: Some("(content recording disabled)".to_string()),
        usage: usage.to_map(),
        segments: Vec::new(),
        est_input: 0,
        residual: input_tok,
        quality: Some("usage_only".to_string()),
        tools_defined: Vec::new(),
        tool_calls: Vec::new(),
        output_est: output_tok,
        response_text: None,
        error: if dep.result_code.unwrap_or(200) >= 400 {
            Some(format!("http_{}", dep.result_code.unwrap_or(500)))
        } else {
            None
        },
    };
    stats.tier2_usage_only_calls += 1;
    Some(rec)
}

fn extract_model_from_span_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    for prefix in ["chat ", "completions ", "responses ", "messages "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let m = rest.trim();
            if !m.is_empty() {
                return Some(m.to_string());
            }
        }
    }
    None
}

fn resolve_target_endpoint(target: &str, gen_system: &str) -> (String, u16, String) {
    let t = target.trim();
    if t.starts_with("http://") || t.starts_with("https://") {
        let (_, h, p, path) = parse_url_parts(t);
        let ep = if path.is_empty() || path == "/" {
            "/v1/chat/completions".to_string()
        } else {
            path
        };
        return (h, p, ep);
    }
    if !t.is_empty() && !t.contains(' ') {
        let host = t.split('/').next().unwrap_or(t).to_string();
        return (host, 443, "/v1/chat/completions".to_string());
    }
    if gen_system.contains("anthropic") {
        (
            "api.anthropic.com".to_string(),
            443,
            "/v1/messages".to_string(),
        )
    } else if gen_system.contains("az.ai") || gen_system.contains("azure") {
        (
            "aoai.openai.azure.com".to_string(),
            443,
            "/openai/deployments/model/chat/completions".to_string(),
        )
    } else {
        (
            "api.openai.com".to_string(),
            443,
            "/v1/chat/completions".to_string(),
        )
    }
}

#[allow(clippy::type_complexity)]
fn reconstruct_messages_and_tools(
    span_props: &Map<String, Value>,
    events: &[NormalizedRow],
) -> (Vec<Value>, Vec<Value>, Vec<String>, Vec<Value>, bool) {
    let mut messages: Vec<Value> = Vec::new();
    let mut tools: Vec<Value> = Vec::new();
    let mut resp_texts: Vec<String> = Vec::new();
    let mut resp_tool_calls: Vec<Value> = Vec::new();
    let mut repaired = false;

    // 1. Check OTel GenAI span events (`AppTraces` rows)
    for ev in events {
        let ev_name = {
            let n1 = val_to_string(ev.props.get("event.name"));
            if !n1.is_empty() {
                n1
            } else {
                ev.message.trim().to_string()
            }
        };
        let role_opt = match ev_name.as_str() {
            "gen_ai.system.message" => Some("system"),
            "gen_ai.user.message" => Some("user"),
            "gen_ai.assistant.message" => Some("assistant"),
            "gen_ai.tool.message" => Some("tool"),
            _ => None,
        };
        if let Some(role) = role_opt {
            let (msg_val, rflag) = extract_otel_event_message(role, &ev.props, &ev.message);
            repaired |= rflag;
            messages.push(msg_val);
            continue;
        }
        if ev_name == "gen_ai.choice" || ev_name == "gen_ai.content.completion" {
            let (txt, tcs, rflag) = extract_otel_choice(&ev.props, &ev.message);
            repaired |= rflag;
            if !txt.is_empty() {
                resp_texts.push(txt);
            }
            resp_tool_calls.extend(tcs);
            continue;
        }
        if ev_name == "gen_ai.content.prompt" || ev.props.contains_key("gen_ai.prompt") {
            let raw_p = val_to_string(
                ev.props
                    .get("gen_ai.prompt")
                    .or_else(|| ev.props.get("content")),
            );
            let (msgs, rflag) = parse_prompt_value_into_messages(&raw_p);
            repaired |= rflag;
            messages.extend(msgs);
        }
    }

    // 2. Check span-level `gen_ai.input.messages` / `gen_ai.prompt` / `llm.prompts`
    if messages.is_empty() {
        for key in ["gen_ai.input.messages", "gen_ai.prompt", "llm.prompts"] {
            if let Some(val) = span_props.get(key) {
                match val {
                    Value::Array(arr) => {
                        messages.extend(arr.iter().cloned());
                    }
                    Value::String(s) => {
                        let (msgs, rflag) = parse_prompt_value_into_messages(s);
                        repaired |= rflag;
                        messages.extend(msgs);
                    }
                    _ => {}
                }
                if !messages.is_empty() {
                    break;
                }
            }
        }
    }

    // 2b. Check Foundry / OpenAI Responses `gen_ai.request.instructions` / `gen_ai.system_instructions` / `llm.system_prompt`
    let has_sys = messages
        .iter()
        .any(|m| m.get("role").and_then(|v| v.as_str()) == Some("system"));
    if !has_sys {
        for sk in [
            "gen_ai.request.instructions",
            "gen_ai.system_instructions",
            "llm.system_prompt",
        ] {
            let sys_txt = val_to_string(span_props.get(sk));
            if !sys_txt.trim().is_empty() {
                messages.insert(
                    0,
                    json!({
                        "role": "system",
                        "content": sys_txt,
                    }),
                );
                break;
            }
        }
    }

    // 3. Check OpenInference flattened attributes (`llm.input_messages.0.message.role`, etc.)
    if messages.is_empty() {
        let mut by_idx: BTreeMap<usize, Map<String, Value>> = BTreeMap::new();
        for (k, v) in span_props {
            if let Some(rest) = k.strip_prefix("llm.input_messages.") {
                if let Some((idx_s, field)) = rest.split_once('.') {
                    if let Ok(idx) = idx_s.parse::<usize>() {
                        let entry = by_idx.entry(idx).or_default();
                        let short_f = field.strip_prefix("message.").unwrap_or(field);
                        entry.insert(short_f.to_string(), v.clone());
                    }
                }
            }
        }
        for (_, m) in by_idx {
            let role = val_to_string(m.get("role"));
            let content = val_to_string(m.get("content"));
            let mut msg_obj = Map::new();
            msg_obj.insert(
                "role".to_string(),
                Value::String(if role.is_empty() {
                    "user".to_string()
                } else {
                    role
                }),
            );
            msg_obj.insert("content".to_string(), Value::String(content));
            messages.push(Value::Object(msg_obj));
        }
    }

    // 4. Extract tool definitions from `gen_ai.request.tools`, `llm.tools`, or `llm.tools.N.tool.json_schema`
    if let Some(t_val) = span_props
        .get("gen_ai.request.tools")
        .or_else(|| span_props.get("gen_ai.tool.definitions"))
        .or_else(|| span_props.get("llm.tools"))
    {
        match t_val {
            Value::Array(arr) => tools.extend(arr.iter().cloned()),
            Value::String(s) => {
                if let Some((pv, rflag)) = parse_or_repair_json(s) {
                    repaired |= rflag;
                    match pv {
                        Value::Array(arr) => tools.extend(arr),
                        v @ Value::Object(_) => tools.push(v),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    if tools.is_empty() {
        let mut t_by_idx: BTreeMap<usize, Value> = BTreeMap::new();
        for (k, v) in span_props {
            if let Some(rest) = k.strip_prefix("llm.tools.") {
                if let Some((idx_s, _)) = rest.split_once('.') {
                    if let Ok(idx) = idx_s.parse::<usize>() {
                        if let Some(s) = v.as_str() {
                            if let Some((tv, rflag)) = parse_or_repair_json(s) {
                                repaired |= rflag;
                                t_by_idx.insert(idx, tv);
                            }
                        } else if v.is_object() {
                            t_by_idx.insert(idx, v.clone());
                        }
                    }
                }
            }
        }
        tools.extend(t_by_idx.into_values());
    }

    // 5. Extract completion text if present on span props
    if resp_texts.is_empty() {
        for k in [
            "gen_ai.completion",
            "gen_ai.output.messages",
            "llm.output_messages.0.message.content",
        ] {
            let s = val_to_string(span_props.get(k));
            if !s.is_empty() {
                resp_texts.push(s);
                break;
            }
        }
    }

    (messages, tools, resp_texts, resp_tool_calls, repaired)
}

fn extract_otel_event_message(
    default_role: &str,
    props: &Map<String, Value>,
    fallback_msg: &str,
) -> (Value, bool) {
    let mut repaired = false;
    let role = {
        let r = val_to_string(props.get("role").or_else(|| props.get("gen_ai.message.role")));
        if r.is_empty() {
            default_role.to_string()
        } else {
            r
        }
    };
    let mut content = val_to_string(
        props
            .get("content")
            .or_else(|| props.get("gen_ai.prompt"))
            .or_else(|| props.get("message.content"))
            .or_else(|| props.get("gen_ai.tool.call.result"))
            .or_else(|| props.get("body")),
    );
    if content.is_empty() && !fallback_msg.starts_with("gen_ai.") {
        content = fallback_msg.to_string();
    }
    // Check if content itself is a JSON object `{ "content": "...", "role": "..." }` or truncated JSON
    let trimmed = content.trim();
    if trimmed.starts_with('{') {
        let (rep, rflag) = repair_truncated_json(trimmed);
        if let Ok(Value::Object(mut m)) = serde_json::from_str::<Value>(&rep) {
            if m.contains_key("content") || m.contains_key("tool_calls") {
                m.entry("role".to_string())
                    .or_insert_with(|| Value::String(role));
                return (Value::Object(m), rflag);
            }
            if rflag {
                content = rep;
                repaired = true;
            }
        }
    }

    let mut msg_obj = Map::new();
    msg_obj.insert("role".to_string(), Value::String(role.clone()));
    msg_obj.insert("content".to_string(), Value::String(content));

    if let Some(tc_val) = props.get("tool_calls").or_else(|| props.get("gen_ai.tool.calls")) {
        match tc_val {
            Value::Array(arr) => {
                msg_obj.insert("tool_calls".to_string(), Value::Array(arr.clone()));
            }
            Value::String(s) => {
                if let Some((Value::Array(arr), rflag)) = parse_or_repair_json(s) {
                    repaired |= rflag;
                    msg_obj.insert("tool_calls".to_string(), Value::Array(arr));
                }
            }
            _ => {}
        }
    }
    if role == "tool" {
        let t_name = val_to_string(
            props
                .get("name")
                .or_else(|| props.get("gen_ai.tool.name"))
                .or_else(|| props.get("gen_ai.tool.call.name")),
        );
        if !t_name.is_empty() {
            msg_obj.insert("name".to_string(), Value::String(t_name));
        }
        let tc_id = val_to_string(
            props
                .get("id")
                .or_else(|| props.get("tool_call_id"))
                .or_else(|| props.get("gen_ai.tool.call.id")),
        );
        if !tc_id.is_empty() {
            msg_obj.insert("tool_call_id".to_string(), Value::String(tc_id));
        }
    }

    (Value::Object(msg_obj), repaired)
}

fn extract_otel_choice(
    props: &Map<String, Value>,
    fallback_msg: &str,
) -> (String, Vec<Value>, bool) {
    let mut repaired = false;
    let mut tcs = Vec::new();
    if let Some(msg_v) = props.get("message") {
        match msg_v {
            Value::Object(m) => {
                let txt = val_to_string(m.get("content"));
                if let Some(Value::Array(arr)) = m.get("tool_calls") {
                    tcs.extend(arr.iter().cloned());
                }
                return (txt, tcs, false);
            }
            Value::String(s) => {
                let (rep, rflag) = repair_truncated_json(s);
                repaired |= rflag;
                if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(&rep) {
                    let txt = val_to_string(m.get("content"));
                    if let Some(Value::Array(arr)) = m.get("tool_calls") {
                        tcs.extend(arr.iter().cloned());
                    }
                    return (txt, tcs, repaired);
                }
            }
            _ => {}
        }
    }
    let txt = val_to_string(
        props
            .get("content")
            .or_else(|| props.get("gen_ai.completion")),
    );
    if let Some(tc_v) = props.get("tool_calls") {
        match tc_v {
            Value::Array(arr) => tcs.extend(arr.iter().cloned()),
            Value::String(s) => {
                let (rep, rflag) = repair_truncated_json(s);
                repaired |= rflag;
                if let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&rep) {
                    tcs.extend(arr);
                }
            }
            _ => {}
        }
    }
    let final_txt = if !txt.is_empty() {
        txt
    } else if !fallback_msg.starts_with("gen_ai.") {
        fallback_msg.to_string()
    } else {
        String::new()
    };
    (final_txt, tcs, repaired)
}

fn parse_prompt_value_into_messages(raw: &str) -> (Vec<Value>, bool) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (Vec::new(), false);
    }
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        let (rep, rflag) = repair_truncated_json(trimmed);
        if let Ok(val) = serde_json::from_str::<Value>(&rep) {
            match val {
                Value::Array(arr) => return (arr, rflag),
                Value::Object(mut m) => {
                    if let Some(Value::Array(arr)) = m.remove("messages") {
                        return (arr, rflag);
                    }
                    return (vec![Value::Object(m)], rflag);
                }
                _ => {}
            }
        }
    }
    (
        vec![json!({
            "role": "user",
            "content": trimmed,
        })],
        false,
    )
}

pub fn resolve_azure_cloud_endpoints(cloud: Option<&str>) -> (&'static str, &'static str) {
    let raw = cloud
        .map(|s| s.to_string())
        .or_else(|| std::env::var("TOKPROF_AZURE_CLOUD").ok())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "usgov" | "gov" | "azureusgovernment" => (
            "https://api.loganalytics.us",
            "https://api.applicationinsights.us",
        ),
        "china" | "cn" | "azurechinacloud" => (
            "https://api.loganalytics.azure.cn",
            "https://api.applicationinsights.azure.cn",
        ),
        _ => (
            "https://api.loganalytics.io",
            "https://api.applicationinsights.io",
        ),
    }
}

pub fn classify_azure_error(status: u16, body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    if status == 401
        || lower.contains("expiredauthenticationtoken")
        || lower.contains("invalidauthenticationtoken")
    {
        if lower.contains("tenant") || lower.contains("invalidauthenticationtokentenant") {
            return format!(
                "Azure tenant or subscription mismatch (HTTP {status}): {}\n  \
                Remediation: verify your active tenant/subscription with `az account show`, then run:\n    \
                az account set --subscription <SUBSCRIPTION_ID>\n    \
                az login --tenant <TENANT_ID>",
                body.trim()
            );
        }
        return format!(
            "Azure authentication failed or token expired (HTTP {status}): {}\n  \
            Remediation: refresh your session with `az login` or `az account get-access-token`.",
            body.trim()
        );
    }
    if status == 403
        || lower.contains("insufficientaccesserror")
        || lower.contains("authorizationfailed")
        || lower.contains("accessdenied")
    {
        return format!(
            "Azure RBAC permission denied (HTTP {status}): {}\n  \
            Remediation:\n    \
            1) Grant Log Analytics Reader or Monitoring Reader on the workspace/app:\n       \
            az role assignment create --role \"Log Analytics Reader\" --assignee <USER_OR_SP> --scope <RESOURCE_ID>\n    \
            2) If you only have RBAC on the Application Insights resource (not the Log Analytics Workspace), \
            query by ARM Resource ID directly:\n       \
            tokprof pull --resource-id /subscriptions/<SUB>/resourceGroups/<RG>/providers/Microsoft.Insights/components/<APP>\n    \
            3) If you have no Azure Monitor RBAC, export container stdout logs with `TOKPROF_STDOUT_LOG=1` and run `tokprof import`.",
            body.trim()
        );
    }
    if status == 404
        || lower.contains("workspacenotfound")
        || lower.contains("applicationnotfound")
        || lower.contains("resourcenotfound")
        || lower.contains("subscriptionnotfound")
    {
        return format!(
            "Azure workspace, application, or subscription not found (HTTP {status}): {}\n  \
            Remediation: check `az account show` for the active subscription, verify the Workspace/App GUID, \
            or pass the full ARM resource path via `--resource-id /subscriptions/...` (for Sovereign Clouds pass `--cloud usgov|china`).",
            body.trim()
        );
    }
    if status == 429 || lower.contains("throttled") || lower.contains("toomanyrequests") {
        return format!(
            "Azure Log Analytics API rate limit / throttling (HTTP {status}): {}\n  \
            Remediation: narrow the time window with `--since 1h` or filter to one session with `--conv <OPERATION_ID>`.",
            body.trim()
        );
    }
    format!(
        "Azure query API returned HTTP {status}: {}",
        body.trim()
    )
}

pub fn is_table_rbac_denial(status: u16, body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    let mentions_traces = lower.contains("apptraces") || lower.contains("traces");
    let mentions_access = status == 403
        || lower.contains("insufficientaccesserror")
        || lower.contains("tableaccessdenied")
        || lower.contains("accessdenied")
        || lower.contains("authorizationfailed")
        || lower.contains("partialerror");
    mentions_traces && mentions_access
}

pub fn is_result_set_too_large(status: u16, body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    matches!(status, 200 | 400 | 413 | 502 | 504)
        && (lower.contains("responsepayloadtoolarge")
            || lower.contains("queryresulttoolarge")
            || lower.contains("e_query_result_set_too_large")
            || lower.contains("resultsettoolarge")
            || lower.contains("exceeded the maximum"))
}

pub fn obtain_azure_bearer_token(explicit: Option<&str>, resource: &str) -> Result<String, String> {
    if let Some(tok) = explicit.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        return Ok(tok.to_string());
    }
    if let Ok(env_tok) = std::env::var("TOKPROF_AZURE_TOKEN") {
        let t = env_tok.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    let out = Command::new("az")
        .args([
            "account",
            "get-access-token",
            "--resource",
            resource,
            "--query",
            "accessToken",
            "-o",
            "tsv",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            format!("cannot execute `az` CLI ({e}); run in Azure Cloud Shell, run `az login`, or pass --token <BEARER_TOKEN>")
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(classify_azure_error(401, stderr.trim()));
    }
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if token.is_empty() {
        return Err("`az account get-access-token` returned an empty token".to_string());
    }
    Ok(token)
}

pub fn query_azure_logs_http(
    base_endpoint: Option<&str>,
    workspace_id: Option<&str>,
    app_id: Option<&str>,
    bearer_token: &str,
    kql: &str,
) -> Result<String, String> {
    query_azure_logs_with_resilience(
        base_endpoint,
        workspace_id,
        app_id,
        None,
        None,
        bearer_token,
        kql,
        None,
    )
    .map(|(body, _)| body)
}

#[allow(clippy::too_many_arguments)]
pub fn query_azure_logs_with_resilience(
    base_endpoint: Option<&str>,
    workspace_id: Option<&str>,
    app_id: Option<&str>,
    resource_id: Option<&str>,
    cloud: Option<&str>,
    bearer_token: &str,
    kql: &str,
    fallback_dep_only_kql: Option<&str>,
) -> Result<(String, bool), String> {
    let (la_base, ai_base) = resolve_azure_cloud_endpoints(cloud);
    let arm_res = resource_id
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            workspace_id.filter(|s| s.trim_start().to_ascii_lowercase().starts_with("/subscriptions/"))
        })
        .or_else(|| {
            app_id.filter(|s| s.trim_start().to_ascii_lowercase().starts_with("/subscriptions/"))
        });

    let (default_base, api_path) = if let Some(res) = arm_res {
        let clean = res.trim().trim_start_matches('/');
        (la_base, format!("/v1/{clean}/query"))
    } else if let Some(ws) = workspace_id.filter(|s| !s.is_empty()) {
        (la_base, format!("/v1/workspaces/{ws}/query"))
    } else if let Some(app) = app_id.filter(|s| !s.is_empty()) {
        (ai_base, format!("/v1/apps/{app}/query"))
    } else {
        return Err(
            "one of --workspace <WORKSPACE_ID>, --app-insights <APP_ID>, or --resource-id <ARM_RESOURCE_ID> is required"
                .to_string(),
        );
    };

    let base = base_endpoint
        .map(|s| s.trim().trim_end_matches('/'))
        .filter(|s| !s.is_empty())
        .unwrap_or(default_base);
    let full_url = format!("{base}{api_path}");

    match execute_query_with_retries(&full_url, bearer_token, kql) {
        Ok(body_str) => {
            if let Some(fb_kql) = fallback_dep_only_kql {
                if is_table_rbac_denial(200, &body_str) {
                    if let Ok(val) = serde_json::from_str::<Value>(&body_str) {
                        if extract_rows_from_json_value(&val).is_empty() {
                            if let Ok(fb_body) =
                                execute_query_with_retries(&full_url, bearer_token, fb_kql)
                            {
                                return Ok((fb_body, true));
                            }
                        }
                    }
                }
            }
            Ok((body_str, false))
        }
        Err((status, err_body)) => {
            if let Some(fb_kql) = fallback_dep_only_kql {
                if is_table_rbac_denial(status, &err_body) {
                    if let Ok(fb_body) = execute_query_with_retries(&full_url, bearer_token, fb_kql)
                    {
                        return Ok((fb_body, true));
                    }
                }
            }
            if is_result_set_too_large(status, &err_body) {
                if !kql.contains("| take ") {
                    let capped_kql = format!("{kql}\n| take 20000");
                    if let Ok(capped_body) =
                        execute_query_with_retries(&full_url, bearer_token, &capped_kql)
                    {
                        return Ok((capped_body, false));
                    }
                }
                if let Some(fb_kql) = fallback_dep_only_kql {
                    if let Ok(fb_body) = execute_query_with_retries(&full_url, bearer_token, fb_kql)
                    {
                        return Ok((fb_body, true));
                    }
                }
            }
            Err(classify_azure_error(status, &err_body))
        }
    }
}

fn execute_query_with_retries(
    full_url: &str,
    bearer_token: &str,
    kql: &str,
) -> Result<String, (u16, String)> {
    let (scheme, host, port, path) = parse_url_parts(full_url);
    let body_bytes = serde_json::to_vec(&json!({ "query": kql })).unwrap_or_default();
    let host_hdr = if port != 80 && port != 443 {
        format!("{host}:{port}")
    } else {
        host.clone()
    };
    let req_head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host_hdr}\r\nAuthorization: Bearer {bearer_token}\r\nContent-Type: application/json\r\nAccept: application/json\r\nAccept-Encoding: gzip, deflate, br\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );

    let max_attempts = 3usize;
    for attempt in 0..max_attempts {
        let timeout = Duration::from_secs(30);
        let mut raw_resp = Vec::new();
        let io_res: Result<(), String> = if scheme == "https" {
            (|| {
                let tcp = if let Some(proxy) = proxy_for(&host) {
                    connect_via_proxy(&proxy, &host, port, timeout).map_err(|e| e.to_string())?
                } else {
                    connect_tcp_timeout(&host, port, timeout).map_err(|e| e.to_string())?
                };
                let sname = ServerName::try_from(host.clone()).map_err(|e| e.to_string())?;
                let tls_cfg = make_upstream_tls_config();
                let conn = ClientConnection::new(tls_cfg, sname).map_err(|e| e.to_string())?;
                let mut tls_stream = StreamOwned::new(conn, tcp);
                tls_stream
                    .write_all(req_head.as_bytes())
                    .map_err(|e| e.to_string())?;
                tls_stream
                    .write_all(&body_bytes)
                    .map_err(|e| e.to_string())?;
                tls_stream.flush().map_err(|e| e.to_string())?;
                let _ = tls_stream.read_to_end(&mut raw_resp);
                Ok(())
            })()
        } else {
            (|| {
                let mut tcp = connect_tcp_timeout(&host, port, timeout).map_err(|e| e.to_string())?;
                tcp.write_all(req_head.as_bytes())
                    .map_err(|e| e.to_string())?;
                tcp.write_all(&body_bytes).map_err(|e| e.to_string())?;
                tcp.flush().map_err(|e| e.to_string())?;
                let _ = tcp.read_to_end(&mut raw_resp);
                Ok(())
            })()
        };

        if let Err(e) = io_res {
            if attempt + 1 < max_attempts {
                std::thread::sleep(Duration::from_millis(150 * (attempt as u64 + 1)));
                continue;
            }
            return Err((503, e));
        }

        let Some(sep_idx) = raw_resp.windows(4).position(|w| w == b"\r\n\r\n") else {
            if attempt + 1 < max_attempts {
                std::thread::sleep(Duration::from_millis(150));
                continue;
            }
            return Err((
                502,
                "invalid HTTP response from Log Analytics / Application Insights endpoint"
                    .to_string(),
            ));
        };
        let head_str = String::from_utf8_lossy(&raw_resp[..sep_idx]);
        let raw_body_bytes = &raw_resp[sep_idx + 4..];
        let mut hmap: HashMap<String, String> = HashMap::new();
        for line in head_str.lines().skip(1) {
            if let Some((k, v)) = line.split_once(':') {
                hmap.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let status_code = head_str
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(500);

        if matches!(status_code, 429 | 502 | 503 | 504) && attempt + 1 < max_attempts {
            let retry_ms = parse_retry_after_ms(&head_str)
                .unwrap_or(200 * (1u64 << attempt))
                .min(5000);
            std::thread::sleep(Duration::from_millis(retry_ms));
            continue;
        }
        let framed = if hmap
            .get("transfer-encoding")
            .map(|s| s.to_ascii_lowercase().contains("chunked"))
            .unwrap_or(false)
        {
            let mut framer = BodyFramer::new(&hmap, false);
            framer.feed(raw_body_bytes)
        } else {
            raw_body_bytes.to_vec()
        };
        let decoded = decode_content_encoding(&framed, hmap.get("content-encoding").map(|s| s.as_str()));
        let body_str = String::from_utf8_lossy(&decoded).into_owned();
        if status_code >= 400 {
            return Err((status_code, body_str.trim().to_string()));
        }
        return Ok(body_str);
    }
    Err((500, "exhausted retry attempts".to_string()))
}

fn parse_retry_after_ms(head_str: &str) -> Option<u64> {
    for line in head_str.lines() {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("retry-after") {
                if let Ok(secs) = v.trim().parse::<f64>() {
                    if secs >= 0.0 {
                        return Some((secs * 1000.0).round() as u64);
                    }
                }
            } else if k.trim().eq_ignore_ascii_case("x-ms-retry-after-ms") {
                if let Ok(ms) = v.trim().parse::<u64>() {
                    return Some(ms);
                }
            }
        }
    }
    None
}
