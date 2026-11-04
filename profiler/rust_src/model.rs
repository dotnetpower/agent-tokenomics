use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

pub const SCHEMA_VERSION: u32 = 1;

fn de_i64_flex<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Value::deserialize(deserializer)?;
    match v {
        Value::Number(n) => Ok(n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f.round() as i64))
            .unwrap_or(0)),
        Value::String(s) => Ok(s.trim().parse::<f64>().map(|f| f.round() as i64).unwrap_or(0)),
        _ => Ok(0),
    }
}

fn de_opt_i64_flex<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(deserializer)?;
    let Some(val) = v else {
        return Ok(None);
    };
    match val {
        Value::Null => Ok(None),
        Value::Number(n) => Ok(n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64))),
        Value::String(s) => Ok(s.trim().parse::<f64>().ok().map(|f| f.round() as i64)),
        _ => Ok(None),
    }
}

fn de_usize_flex<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Value::deserialize(deserializer)?;
    match v {
        Value::Number(n) => Ok(n
            .as_u64()
            .or_else(|| n.as_f64().map(|f| f.max(0.0).round() as u64))
            .unwrap_or(0) as usize),
        _ => Ok(0),
    }
}

fn de_vec_string_flex<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Vec<Value>>::deserialize(deserializer)?;
    let Some(arr) = v else {
        return Ok(Vec::new());
    };
    Ok(arr
        .into_iter()
        .filter_map(|x| match x {
            Value::String(s) if !s.is_empty() => Some(s),
            _ => None,
        })
        .collect())
}

pub const KINDS: &[&str] = &[
    "system",
    "history_user",
    "history_assistant",
    "user_prompt",
    "tool_def",
    "tool_call",
    "tool_result",
    "attachment",
    "response_schema",
    "raw_prompt",
    "other",
];

pub const TOOL_KINDS: &[&str] = &["tool_def", "tool_call", "tool_result"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawSegment {
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub turn: Option<i64>,
    #[serde(default)]
    pub opaque: bool,
}

impl RawSegment {
    pub fn new(kind: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            text: text.into(),
            role: None,
            name: None,
            turn: None,
            opaque: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    #[serde(default)]
    pub kind: String,
    #[serde(default, deserialize_with = "de_i64_flex")]
    pub tokens: i64,
    #[serde(default, deserialize_with = "de_usize_flex")]
    pub chars: usize,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default, deserialize_with = "de_opt_i64_flex")]
    pub turn: Option<i64>,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub opaque: bool,
    #[serde(default)]
    pub text: Option<String>,
}

fn default_method() -> String {
    "heuristic".to_string()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<i64>,
}

impl Usage {
    pub fn sanitized(&self) -> Self {
        Self {
            input_total: self.input_total.filter(|&v| v >= 0),
            cached: self.cached.filter(|&v| v >= 0),
            cache_write: self.cache_write.filter(|&v| v >= 0),
            output: self.output.filter(|&v| v >= 0),
            reasoning: self.reasoning.filter(|&v| v >= 0),
        }
    }

    pub fn to_map(&self) -> Map<String, Value> {
        let s = self.sanitized();
        let mut m = Map::new();
        if let Some(v) = s.input_total {
            m.insert("input_total".to_string(), Value::from(v));
        }
        if let Some(v) = s.cached {
            m.insert("cached".to_string(), Value::from(v));
        }
        if let Some(v) = s.cache_write {
            m.insert("cache_write".to_string(), Value::from(v));
        }
        if let Some(v) = s.output {
            m.insert("output".to_string(), Value::from(v));
        }
        if let Some(v) = s.reasoning {
            m.insert("reasoning".to_string(), Value::from(v));
        }
        m
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecord {
    #[serde(default = "default_version")]
    pub v: u32,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub ts: f64,
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub host_class: Option<String>,
    #[serde(default)]
    pub host_id: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub detect: Option<Value>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default, deserialize_with = "de_opt_i64_flex")]
    pub status: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_i64_flex")]
    pub latency_ms: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_i64_flex")]
    pub ttfb_ms: Option<i64>,
    #[serde(default)]
    pub conv: Option<String>,
    #[serde(default)]
    pub sysfp: Option<String>,
    #[serde(default)]
    pub sys_label: Option<String>,
    #[serde(default)]
    pub usage: Map<String, Value>,
    #[serde(default)]
    pub segments: Vec<Segment>,
    #[serde(default, deserialize_with = "de_i64_flex")]
    pub est_input: i64,
    #[serde(default, deserialize_with = "de_opt_i64_flex")]
    pub residual: Option<i64>,
    #[serde(default)]
    pub quality: Option<String>,
    #[serde(default, deserialize_with = "de_vec_string_flex")]
    pub tools_defined: Vec<String>,
    #[serde(default, deserialize_with = "de_vec_string_flex")]
    pub tool_calls: Vec<String>,
    #[serde(default, deserialize_with = "de_opt_i64_flex")]
    pub output_est: Option<i64>,
    #[serde(default)]
    pub response_text: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

fn default_version() -> u32 {
    SCHEMA_VERSION
}
