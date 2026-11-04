use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;
use tiktoken_rs::CoreBPE;

const MAX_CACHE_TEXT_LEN: usize = 65_536;
const SHARD_COUNT: usize = 16;
const MAX_SHARD_ENTRIES: usize = 256;

static O200K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::o200k_base().ok());
static CL100K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::cl100k_base().ok());
static P50K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::p50k_base().ok());
static R50K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::r50k_base().ok());

type CacheKey = (u8, usize, u64, u64);
static COUNT_SHARDS: Lazy<[Mutex<HashMap<CacheKey, i64>>; SHARD_COUNT]> = Lazy::new(|| {
    std::array::from_fn(|_| Mutex::new(HashMap::with_capacity(MAX_SHARD_ENTRIES)))
});

#[inline]
pub fn fast_hash_pair(bytes: &[u8]) -> (u64, u64) {
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x84222325cbf29ce4;
    let len = bytes.len();
    if len <= 512 {
        for &b in bytes {
            h1 = (h1 ^ (b as u64)).wrapping_mul(0x100000001b3);
            h2 = (h2.wrapping_add(b as u64)).wrapping_mul(0x517cc1b727220a95);
        }
    } else {
        for &b in &bytes[..128] {
            h1 = (h1 ^ (b as u64)).wrapping_mul(0x100000001b3);
        }
        for &b in &bytes[len - 128..] {
            h2 = (h2 ^ (b as u64)).wrapping_mul(0x517cc1b727220a95);
        }
        let step = (len - 256) / 64;
        if step > 0 {
            let mut idx = 128;
            while idx < len - 128 {
                let b = bytes[idx];
                h1 = (h1 ^ (b as u64)).wrapping_mul(0x100000001b3);
                h2 = (h2.rotate_left(5) ^ (b as u64)).wrapping_mul(0x517cc1b727220a95);
                idx += step;
            }
        }
    }
    (h1, h2)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingId {
    O200k = 1,
    Cl100k = 2,
    P50k = 3,
    R50k = 4,
}

impl EncodingId {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "o200k_base" => Some(Self::O200k),
            "cl100k_base" => Some(Self::Cl100k),
            "p50k_base" => Some(Self::P50k),
            "r50k_base" => Some(Self::R50k),
            _ => None,
        }
    }

    pub fn bpe(self) -> Option<&'static CoreBPE> {
        match self {
            Self::O200k => O200K_BPE.as_ref(),
            Self::Cl100k => CL100K_BPE.as_ref(),
            Self::P50k => P50K_BPE.as_ref(),
            Self::R50k => R50K_BPE.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum CounterKind {
    Heuristic,
    Tiktoken { enc_id: EncodingId, approx: bool },
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
        let enc_id = EncodingId::from_name(encoding)?;
        enc_id.bpe()?;
        let method = if approx {
            format!("tiktoken:{encoding}~approx")
        } else {
            format!("tiktoken:{encoding}")
        };
        Some(Self {
            kind: CounterKind::Tiktoken { enc_id, approx },
            method,
        })
    }

    pub fn count(&self, text: &str) -> i64 {
        if text.is_empty() {
            return 0;
        }
        match self.kind {
            CounterKind::Heuristic => heuristic_count(text),
            CounterKind::Tiktoken { enc_id, .. } => {
                if text.len() <= MAX_CACHE_TEXT_LEN {
                    let (h1, h2) = fast_hash_pair(text.as_bytes());
                    let key: CacheKey = (enc_id as u8, text.len(), h1, h2);
                    let shard_idx = (h1 as usize) & (SHARD_COUNT - 1);
                    if let Ok(guard) = COUNT_SHARDS[shard_idx].lock() {
                        if let Some(&cached) = guard.get(&key) {
                            return cached;
                        }
                    }
                    let n = match enc_id.bpe() {
                        Some(bpe) => bpe.encode_ordinary(text).len() as i64,
                        None => heuristic_count(text),
                    };
                    if let Ok(mut guard) = COUNT_SHARDS[shard_idx].lock() {
                        if guard.len() >= MAX_SHARD_ENTRIES {
                            guard.clear();
                        }
                        guard.insert(key, n);
                    }
                    return n;
                }
                match enc_id.bpe() {
                    Some(bpe) => bpe.encode_ordinary(text).len() as i64,
                    None => heuristic_count(text),
                }
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
    for &b in text.as_bytes() {
        if b < 0x80 {
            ascii_chars += 1;
        } else if (b & 0xC0) != 0x80 {
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
