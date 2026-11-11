use once_cell::sync::Lazy;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;
use tiktoken_rs::CoreBPE;

const MAX_CACHE_TEXT_LEN: usize = 65_536;
const SHARD_COUNT: usize = 16;
const MAX_SHARD_ENTRIES: usize = 256;
const L1_SLOTS: usize = 64;

static O200K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::o200k_base().ok());
static CL100K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::cl100k_base().ok());
static P50K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::p50k_base().ok());
static R50K_BPE: Lazy<Option<CoreBPE>> = Lazy::new(|| tiktoken_rs::r50k_base().ok());

type CacheKey = (u8, usize, u64, u64);
static COUNT_SHARDS: Lazy<[Mutex<HashMap<CacheKey, i64>>; SHARD_COUNT]> = Lazy::new(|| {
    std::array::from_fn(|_| Mutex::new(HashMap::with_capacity(MAX_SHARD_ENTRIES)))
});

thread_local! {
    static L1_COUNT_CACHE: [Cell<(CacheKey, i64)>; L1_SLOTS] =
        std::array::from_fn(|_| Cell::new(((0, 0, 0, 0), 0)));
}

#[inline]
fn read_u64_le(slice: &[u8]) -> u64 {
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&slice[..8]);
    u64::from_le_bytes(arr)
}

#[inline]
pub fn fast_hash_pair(bytes: &[u8]) -> (u64, u64) {
    let len = bytes.len();
    let mut h1: u64 = 0xcbf29ce484222325 ^ (len as u64);
    let mut h2: u64 = 0x84222325cbf29ce4 ^ ((len as u64).rotate_left(32));
    let mut i = 0usize;
    while i + 32 <= len {
        let w0 = read_u64_le(&bytes[i..i + 8]);
        let w1 = read_u64_le(&bytes[i + 8..i + 16]);
        let w2 = read_u64_le(&bytes[i + 16..i + 24]);
        let w3 = read_u64_le(&bytes[i + 24..i + 32]);
        h1 = (h1 ^ w0).wrapping_mul(0x9E3779B97F4A7C15);
        h2 = (h2.wrapping_add(w1)).wrapping_mul(0xBF58476D1CE4E5B9);
        h1 = (h1.rotate_left(17) ^ w2).wrapping_mul(0x94D049BB133111EB);
        h2 = (h2.rotate_left(23) ^ w3).wrapping_mul(0x517cc1b727220a95);
        i += 32;
    }
    while i + 8 <= len {
        let w = read_u64_le(&bytes[i..i + 8]);
        h1 = (h1 ^ w).wrapping_mul(0x9E3779B97F4A7C15);
        h2 = (h2.wrapping_add(w)).wrapping_mul(0xBF58476D1CE4E5B9);
        i += 8;
    }
    while i < len {
        let b = bytes[i] as u64;
        h1 = (h1 ^ b).wrapping_mul(0x100000001b3);
        h2 = (h2.wrapping_add(b)).wrapping_mul(0x517cc1b727220a95);
        i += 1;
    }
    (h1 ^ (h1 >> 31), h2 ^ (h2 >> 29))
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
    Tiktoken {
        enc_id: EncodingId,
        approx: bool,
        scale_permille: u16,
    },
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
        Self::tiktoken_scaled(encoding, approx, 1000)
    }

    pub fn tiktoken_scaled(encoding: &str, approx: bool, scale_permille: u16) -> Option<Self> {
        let enc_id = EncodingId::from_name(encoding)?;
        enc_id.bpe()?;
        let method = if approx {
            format!("tiktoken:{encoding}~approx")
        } else {
            format!("tiktoken:{encoding}")
        };
        Some(Self {
            kind: CounterKind::Tiktoken {
                enc_id,
                approx,
                scale_permille: scale_permille.max(100),
            },
            method,
        })
    }

    pub fn count(&self, text: &str) -> i64 {
        if text.is_empty() {
            return 0;
        }
        match self.kind {
            CounterKind::Heuristic => heuristic_count(text),
            CounterKind::Tiktoken {
                enc_id,
                scale_permille,
                ..
            } => {
                let apply_scale = |n: i64| -> i64 {
                    if scale_permille == 1000 || n == 0 {
                        n
                    } else {
                        ((n * (scale_permille as i64) + 500) / 1000).max(1)
                    }
                };
                if text.len() <= MAX_CACHE_TEXT_LEN {
                    let (h1, h2) = fast_hash_pair(text.as_bytes());
                    let key: CacheKey = (enc_id as u8, text.len(), h1, h2);
                    let l1_idx = (h1 as usize) & (L1_SLOTS - 1);
                    if let Some(hit) = L1_COUNT_CACHE.with(|slots| {
                        let (ck, cv) = slots[l1_idx].get();
                        if ck == key {
                            Some(cv)
                        } else {
                            None
                        }
                    }) {
                        return apply_scale(hit);
                    }
                    let shard_idx = ((h1 >> 6) as usize) & (SHARD_COUNT - 1);
                    if let Ok(guard) = COUNT_SHARDS[shard_idx].lock() {
                        if let Some(&cached) = guard.get(&key) {
                            L1_COUNT_CACHE.with(|slots| slots[l1_idx].set((key, cached)));
                            return apply_scale(cached);
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
                    L1_COUNT_CACHE.with(|slots| slots[l1_idx].set((key, n)));
                    return apply_scale(n);
                }
                let raw_n = match enc_id.bpe() {
                    Some(bpe) => bpe.encode_ordinary(text).len() as i64,
                    None => heuristic_count(text),
                };
                apply_scale(raw_n)
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
    let mut cjk_chars: i64 = 0;
    let mut other_chars: i64 = 0;
    for ch in text.chars() {
        let u = ch as u32;
        if u < 0x80 {
            ascii_chars += 1;
        } else if (0xAC00..=0xD7AF).contains(&u)
            || (0x3000..=0x9FFF).contains(&u)
            || (0xF900..=0xFAFF).contains(&u)
        {
            cjk_chars += 1;
        } else {
            other_chars += 1;
        }
    }
    ((ascii_chars + 3) >> 2) + ((cjk_chars * 3 + 1) / 2) + ((other_chars * 2 + 2) / 3)
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

pub fn model_family_scale_permille(model: Option<&str>) -> u16 {
    let raw = model.unwrap_or("").trim().to_ascii_lowercase();
    if raw.contains("claude") || raw.starts_with("anthropic.") {
        1070
    } else if raw.contains("gemini") {
        1040
    } else if raw.contains("llama")
        || raw.contains("deepseek")
        || raw.contains("qwen")
        || raw.contains("mistral")
    {
        1020
    } else {
        1000
    }
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
        let scale = if approx {
            model_family_scale_permille(model)
        } else {
            1000
        };
        if let Some(c) = TokenCounter::tiktoken_scaled(enc, approx, scale) {
            return c;
        }
    }
    TokenCounter::heuristic()
}
