use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use tiktoken_rs::CoreBPE;

const MAX_CACHE_TEXT_LEN: usize = 65_536;
const MAX_CACHE_ENTRIES: usize = 2048;

static O200K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::o200k_base().ok());
static CL100K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::cl100k_base().ok());
static P50K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::p50k_base().ok());
static R50K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::r50k_base().ok());

static COUNT_CACHE: Lazy<Mutex<HashMap<(String, String), i64>>> =
    Lazy::new(|| Mutex::new(HashMap::with_capacity(512)));

#[derive(Debug, Clone)]
pub enum CounterKind {
    Heuristic,
    Tiktoken { encoding: String, approx: bool },
}

#[derive(Debug, Clone)]
pub struct TokenCounter {
    pub kind: CounterKind,
    pub method: String,
}

impl TokenCounter {
    pub fn heuristic() -> Self {
        Self {
            kind: CounterKind::Heuristic,
            method: "heuristic".to_string(),
        }
    }

    pub fn tiktoken(encoding: &str, approx: bool) -> Option<Self> {
        let avail = match encoding {
            "o200k_base" => O200K_BPE.is_some(),
            "cl100k_base" => CL100K_BPE.is_some(),
            "p50k_base" => P50K_BPE.is_some(),
            "r50k_base" => R50K_BPE.is_some(),
            _ => false,
        };
        if !avail {
            return None;
        }
        let method = if approx {
            format!("tiktoken:{encoding}~approx")
        } else {
            format!("tiktoken:{encoding}")
        };
        Some(Self {
            kind: CounterKind::Tiktoken {
                encoding: encoding.to_string(),
                approx,
            },
            method,
        })
    }

    pub fn count(&self, text: &str) -> i64 {
        if text.is_empty() {
            return 0;
        }
        match &self.kind {
            CounterKind::Heuristic => heuristic_count(text),
            CounterKind::Tiktoken { encoding, .. } => {
                if text.len() <= MAX_CACHE_TEXT_LEN {
                    if let Ok(guard) = COUNT_CACHE.lock() {
                        if let Some(&cached) = guard.get(&(encoding.clone(), text.to_string())) {
                            return cached;
                        }
                    }
                }
                let bpe_opt = match encoding.as_str() {
                    "o200k_base" => O200K_BPE.as_ref(),
                    "cl100k_base" => CL100K_BPE.as_ref(),
                    "p50k_base" => P50K_BPE.as_ref(),
                    "r50k_base" => R50K_BPE.as_ref(),
                    _ => None,
                };
                let n = match bpe_opt {
                    Some(bpe) => bpe.encode_ordinary(text).len() as i64,
                    None => heuristic_count(text),
                };
                if text.len() <= MAX_CACHE_TEXT_LEN {
                    if let Ok(mut guard) = COUNT_CACHE.lock() {
                        if guard.len() >= MAX_CACHE_ENTRIES {
                            let to_drop: Vec<(String, String)> =
                                guard.keys().take(MAX_CACHE_ENTRIES / 4).cloned().collect();
                            for k in to_drop {
                                guard.remove(&k);
                            }
                        }
                        guard.insert((encoding.clone(), text.to_string()), n);
                    }
                }
                n
            }
        }
    }
}

pub fn heuristic_count(text: &str) -> i64 {
    if text.is_empty() {
        return 0;
    }
    if text.is_ascii() {
        return ((text.len() as i64) + 3) >> 2;
    }
    let mut ascii_chars: i64 = 0;
    let mut other_chars: i64 = 0;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii_chars += 1;
        } else {
            other_chars += 1;
        }
    }
    ((ascii_chars + 3) >> 2) + other_chars
}

const OPENAI_PREFIXES: &[&str] = &[
    "gpt-",
    "o1",
    "o3",
    "o4",
    "chatgpt",
    "text-embedding",
    "ft:gpt",
    "computer-use",
    "codex-",
];

pub fn encoding_for(model: Option<&str>) -> (&'static str, bool) {
    let raw = model.unwrap_or("").trim().to_ascii_lowercase();
    let m = raw.rsplit('/').next().unwrap_or(raw.as_str());
    if (m.starts_with("gpt-3.5") || m.starts_with("gpt-4-") || m.starts_with("gpt-4"))
        && !(m.starts_with("gpt-4o")
            || m.starts_with("gpt-4.1")
            || m.starts_with("gpt-4.5")
            || m.starts_with("gpt-5"))
    {
        return ("cl100k_base", false);
    }
    if OPENAI_PREFIXES.iter().any(|p| m.starts_with(p))
        || m.contains("gpt-4o")
        || m.contains("gpt-4.1")
        || m.contains("gpt-5")
    {
        return ("o200k_base", false);
    }
    ("o200k_base", true)
}

pub fn get_counter(model: Option<&str>, prefer_tiktoken: bool) -> TokenCounter {
    if let Ok(custom) = std::env::var("TOKPROF_TOKENIZER") {
        let custom = custom.trim();
        if matches!(
            custom,
            "o200k_base" | "cl100k_base" | "p50k_base" | "r50k_base"
        ) {
            if let Some(c) = TokenCounter::tiktoken(custom, false) {
                return c;
            }
        }
    }
    if prefer_tiktoken {
        let (enc, approx) = encoding_for(model);
        if let Some(c) = TokenCounter::tiktoken(enc, approx) {
            return c;
        }
    }
    TokenCounter::heuristic()
}
