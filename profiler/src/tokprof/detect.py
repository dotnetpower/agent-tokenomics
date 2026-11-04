"""Smart detection of LLM API calls: which HTTP(S) requests are model calls, and of which API family.

A fixed list of URL paths misses most real deployments: private DNS names, on-prem vLLM / Ollama / TGI /
llama.cpp servers, gateways that re-route paths, Azure private endpoints, Gemini and Bedrock style URLs.
Instead of trusting one signal, this module scores independent evidence and records why it decided:

1. **Body shape** (strongest, independent of host and path): a ``messages`` list of role/content objects, Gemini
   ``contents``/``parts``, a Responses-style ``input`` with its sibling keys, a ``prompt``/``inputs`` body.
2. **Path hint**: ``/chat/completions``, ``/responses``, ``/messages``, ``/generate``, ``/api/chat``,
   ``:generateContent``, ... tolerant of prefixes, trailing slashes, Azure ``/deployments/<name>/``.
3. **Host / port hint** (weak, never enough alone): known provider domains, loopback, RFC 1918 and
   private DNS names (``.internal``, ``.svc.cluster.local``, ``privatelink``), single-label service names,
   LLM-ish name tokens (``vllm``, ``ollama``) and conventional ports (8000, 11434, ...).
4. **Response corroboration**: usage counters, ``choices``, ``candidates``, ``content`` blocks, SSE/NDJSON chunks.
   Ambiguous bodies (a bare ``prompt`` or ``input``) become *candidates* that are accepted only when the
   response looks like a model answer; once an endpoint is confirmed, later ambiguous calls to it are accepted.
5. **User overrides**: ``--llm-host`` / ``--llm-path`` force detection, ``--exclude-host`` suppresses it.

Standard library only; the other language implementations (Node hook, Go helper, mitmproxy addon) apply only the
cheap :func:`plausible` pre-filter and leave the decision to this module, so the logic exists in one place.
"""

from __future__ import annotations

import fnmatch
import ipaddress
import json
import os
import re
import threading
import time
from dataclasses import dataclass, field
from functools import lru_cache
from typing import Any, Iterator, Optional
from urllib.parse import unquote

from .parse.request import ANTHROPIC, COMPLETIONS, GEMINI, OPENAI_CHAT, OPENAI_RESPONSES

ACCEPT = 50
CANDIDATE = 25
_CGNAT_NET = ipaddress.ip_network("100.64.0.0/10")
_HOST_SPLIT_RE = re.compile(r"[.\-_0-9]+")

# ---- pre-filter (mirrored in the Node hook, the Go helper and the mitmproxy addon; see tests/vectors) --------------

EXCLUDED_PATH = re.compile(
    r"/(embeddings?|moderations?|rerank|tokenize|detokenize|count_tokens|"
    r"audio/(speech|transcriptions|translations)|images?/(generations|edits|variations)|"
    r"files|batches|fine_tuning|uploads|vector_stores|assistants|threads|realtime)(/|$)"
    r"|:(embedContent|batchEmbedContents|countTokens)$",
    re.I,
)
_PLAUSIBLE_KEYS = re.compile(r'"(?:messages|contents|chat_history)"\s*:\s*\[|"(?:prompt|input|inputs|preamble|message)"\s*:')


@lru_cache(maxsize=1024)
def normalize_path(path: str) -> str:
    p = (path or "").split("?", 1)[0].split("#", 1)[0]
    return p.rstrip("/") or "/"


def plausible(method: str, path: str, body_text: str) -> bool:
    """Cheap test that decides whether a request body is worth sending to the detector at all."""
    if (method or "").upper() != "POST" or not body_text:
        return False
    head = body_text[:512].lstrip() if len(body_text) > 512 else body_text.lstrip()
    if len(head) < 8 or head[0] != "{":
        return False
    if EXCLUDED_PATH.search(normalize_path(path)):
        return False
    if len(body_text) <= 81_920:
        return _PLAUSIBLE_KEYS.search(body_text) is not None
    return _PLAUSIBLE_KEYS.search(body_text[:65_536]) is not None or _PLAUSIBLE_KEYS.search(body_text[-16_384:]) is not None


# ---- host classification ------------------------------------------------------------------------------------------

_PROVIDERS = (
    ("api.openai.com", "openai"),
    ("openai.azure.com", "azure_openai"),
    ("cognitiveservices.azure.com", "azure_openai"),
    ("services.ai.azure.com", "azure_ai_foundry"),
    ("models.inference.ai.azure.com", "azure_ai"),
    ("api.anthropic.com", "anthropic"),
    ("generativelanguage.googleapis.com", "google_gemini"),
    ("aiplatform.googleapis.com", "google_vertex"),
    ("api.mistral.ai", "mistral"),
    ("api.groq.com", "groq"),
    ("openrouter.ai", "openrouter"),
    ("api.together.xyz", "together"),
    ("api.together.ai", "together"),
    ("api.fireworks.ai", "fireworks"),
    ("api.deepseek.com", "deepseek"),
    ("api.x.ai", "xai"),
    ("api.cohere.com", "cohere"),
    ("api.cohere.ai", "cohere"),
    ("api-inference.huggingface.co", "huggingface"),
    ("router.huggingface.co", "huggingface"),
    ("integrate.api.nvidia.com", "nvidia_nim"),
    ("api.perplexity.ai", "perplexity"),
    ("api.cerebras.ai", "cerebras"),
    ("api.sambanova.ai", "sambanova"),
    ("api.githubcopilot.com", "github_copilot"),
    ("models.github.ai", "github_models"),
)
_BEDROCK = re.compile(r"^bedrock-runtime(-fips)?\.[a-z0-9-]+\.amazonaws\.com(\.cn)?$")
_PRIVATE_SUFFIXES = (
    ".internal", ".local", ".lan", ".corp", ".intranet", ".home.arpa", ".svc", ".cluster.local",
    ".private", ".priv", ".localdomain", ".home", ".intra",
)
_NAME_TOKENS = frozenset({
    "vllm", "ollama", "llm", "llms", "llama", "llamacpp", "openai", "inference", "tgi", "litellm", "lmstudio", "triton",
    "sglang", "localai", "gpt", "genai", "aoai", "qwen", "gemma", "mistral", "deepseek", "phi", "model", "models", "ai",
})
_PORT_HINTS = frozenset({8000, 8001, 8080, 11434, 4000, 1234, 30000, 5000, 9000, 3000})


@dataclass
class HostInfo:
    cls: str = ""  # provider:<name> | loopback | private-ip | private-dns | local-name | public
    points: int = 0
    reasons: list = field(default_factory=list)


@lru_cache(maxsize=1024)
def _classify_host_cached(host: str, port: int = 0) -> tuple[str, int, tuple[str, ...]]:
    h = (host or "").strip().strip("[]").lower().rstrip(".")
    if "%" in h:
        h = h.split("%", 1)[0]
    if not h:
        return "unknown", 0, ()
    for suffix, name in _PROVIDERS:
        if h == suffix or h.endswith("." + suffix):
            reasons = [f"known provider domain ({name})"]
            if "privatelink" in h:
                reasons.append("private endpoint")
            return f"provider:{name}", 15, tuple(reasons)
    if _BEDROCK.match(h):
        return "provider:aws_bedrock", 15, ("known provider domain (aws_bedrock)",)
    try:
        ip = ipaddress.ip_address(h)
    except ValueError:
        ip = None
    if ip is not None:
        if ip.is_loopback:
            cls = "loopback"
        elif ip.is_private or ip.is_link_local or (ip.version == 4 and ip in _CGNAT_NET):
            cls = "private-ip"
        else:
            cls = "public"
    elif h == "localhost" or h.endswith(".localhost"):
        cls = "loopback"
    elif h.endswith(_PRIVATE_SUFFIXES) or ".privatelink." in h or h.startswith("privatelink.") or h == "host.docker.internal":
        cls = "private-dns"
    elif "." not in h:
        cls = "local-name"  # a bare service name (docker compose, Kubernetes, /etc/hosts)
    else:
        cls = "public"
    points = 0
    reasons: list[str] = []
    if cls in ("loopback", "private-ip", "private-dns", "local-name"):
        points += 5
        reasons.append(f"non-public host ({cls})")
    if ip is None and set(_HOST_SPLIT_RE.split(h)) & _NAME_TOKENS:
        points += 5
        reasons.append("host name mentions an LLM server")
    if port in _PORT_HINTS:
        points += 5
        reasons.append(f"conventional LLM server port ({port})")
    return cls, points, tuple(reasons)


def classify_host(host: str, port: int = 0) -> HostInfo:
    cls, points, reasons = _classify_host_cached(host or "", int(port or 0))
    return HostInfo(cls=cls, points=points, reasons=list(reasons))


# ---- path hints ---------------------------------------------------------------------------------------------------

_PATH_HINTS = (
    (re.compile(r"/chat/completions$", re.I), OPENAI_CHAT),
    (re.compile(r"/responses$", re.I), OPENAI_RESPONSES),
    (re.compile(r"/messages$", re.I), ANTHROPIC),
    (re.compile(r"/(?:api|v\d+)/chat$|/chat$", re.I), OPENAI_CHAT),  # Ollama /api/chat, Cohere /v1/chat & /v2/chat
    (re.compile(r"/(?:v\d+/)?completions?$|/generate(?:_stream)?$|/api/generate$", re.I), COMPLETIONS),
    (re.compile(r":(?:stream)?generateContent$", re.I), GEMINI),
    (re.compile(r"/(?:converse(?:-stream)?|invoke(?:-with-response-stream)?|invocations)$", re.I), "generic"),
)
_MODEL_FROM_PATH = (
    re.compile(r"/deployments/([^/]+)/", re.I),  # Azure OpenAI
    (re.compile(r"/models/([^/:?]+)(?:[:/])", re.I)),  # Gemini, Vertex
    re.compile(r"/model/([^/]+)/(?:converse|invoke)", re.I),  # Bedrock
)
_STREAM_PATH = re.compile(r":streamGenerateContent$|/converse-stream$|-with-response-stream$|/generate_stream$", re.I)


@lru_cache(maxsize=1024)
def path_hint(path: str) -> Optional[str]:
    p = normalize_path(path)
    for rx, api in _PATH_HINTS:
        if rx.search(p):
            return api
    return None


@lru_cache(maxsize=1024)
def model_from_path(path: str) -> Optional[str]:
    p = normalize_path(path)
    target = p + "/"
    for rx in _MODEL_FROM_PATH:
        m = rx.search(target)
        if m:
            return unquote(m.group(1))
    return None


# ---- body shape ---------------------------------------------------------------------------------------------------

_ROLES = frozenset({"system", "user", "assistant", "developer", "tool", "function", "model", "human", "ai", "chatbot"})
_RESPONSES_KEYS = ("instructions", "previous_response_id", "max_output_tokens", "reasoning", "store", "include", "truncation", "parallel_tool_calls")
_SAMPLING_KEYS = frozenset({
    "max_tokens", "temperature", "top_p", "top_k", "stop", "n_predict", "max_new_tokens", "stream", "n", "logprobs",
    "frequency_penalty", "presence_penalty", "repetition_penalty", "min_p", "seed", "sampling_params", "options",
    "raw", "system", "template", "parameters", "preamble", "connectors",
})
_ANTHROPIC_BLOCKS = frozenset({"tool_use", "tool_result", "thinking", "redacted_thinking", "document"})


@dataclass
class Shape:
    api: Optional[str] = None
    strong: bool = False
    points: int = 0
    reasons: list = field(default_factory=list)


def classify_body(obj: Any) -> Shape:
    sh = Shape()
    if not isinstance(obj, dict):
        return sh
    msgs = obj.get("messages")
    if isinstance(msgs, list) and msgs and any(isinstance(m, dict) and str(m.get("role") or "").lower() in _ROLES for m in msgs):
        if any(isinstance(m, dict) and ("content" in m or "tool_calls" in m or "parts" in m or "message" in m) for m in msgs):
            sh.strong, sh.points, sh.api = True, 60, OPENAI_CHAT
            sh.reasons.append("body has a messages list of role/content objects")
            if "anthropic_version" in obj or ("system" in obj and "max_tokens" in obj) or _has_anthropic_blocks(msgs):
                sh.api = ANTHROPIC
                sh.reasons.append("Anthropic-style (anthropic_version, top-level system or content blocks)")
            return sh
    if isinstance(obj.get("message"), str) and obj["message"]:
        ch = obj.get("chat_history")
        if isinstance(ch, list) or isinstance(obj.get("preamble"), str):
            sh.strong, sh.points, sh.api = True, 60, OPENAI_CHAT
            sh.reasons.append("body has Cohere chat_history/preamble and message")
            return sh
        if isinstance(obj.get("model"), str) and obj["model"]:
            sh.api, sh.points = OPENAI_CHAT, 25
            sh.reasons.append("body has model and message (Cohere chat shape: needs corroboration)")
            return sh
    contents = obj.get("contents")
    if isinstance(contents, list) and contents and any(isinstance(c, dict) and isinstance(c.get("parts"), list) for c in contents):
        sh.strong, sh.points, sh.api = True, 60, GEMINI
        sh.reasons.append("body has Gemini contents/parts")
        return sh
    inp = obj.get("input")
    if inp is not None and "messages" not in obj and "prompt" not in obj:
        responses_keys = [k for k in _RESPONSES_KEYS if k in obj]
        items = inp if isinstance(inp, list) else []
        item_shape = any(isinstance(i, dict) and ("role" in i or i.get("type") in ("message", "function_call", "function_call_output", "input_text")) for i in items)
        if item_shape or responses_keys:
            sh.strong, sh.points, sh.api = True, 60, OPENAI_RESPONSES
            sh.reasons.append("body has a Responses-style input" + (f" with {responses_keys[0]}" if responses_keys else ""))
            return sh
        if isinstance(inp, (str, list)) and (isinstance(inp, str) or all(isinstance(i, (str, int)) for i in inp)):
            sh.api, sh.points = OPENAI_RESPONSES, 25
            sh.reasons.append("body has a bare input (Responses API or embeddings: needs corroboration)")
            return sh
    for key in ("prompt", "inputs"):
        val = obj.get(key)
        if isinstance(val, (str, list)) and val != "" and val != []:
            sh.api, sh.points = COMPLETIONS, 25
            sh.reasons.append(f"body has a {key} (text completion shape: needs corroboration)")
            if _SAMPLING_KEYS & set(obj):
                sh.points += 10
                sh.reasons.append("with sampling parameters")
            return sh
    return sh


def _has_anthropic_blocks(msgs: list) -> bool:
    for m in msgs:
        c = m.get("content") if isinstance(m, dict) else None
        if isinstance(c, list) and any(isinstance(b, dict) and (b.get("type") in _ANTHROPIC_BLOCKS or (b.get("type") == "image" and "source" in b)) for b in c):
            return True
    return False


# ---- response corroboration ---------------------------------------------------------------------------------------

_USAGE_KEYS = ("prompt_tokens", "completion_tokens", "input_tokens", "output_tokens", "total_tokens", "inputTokens", "outputTokens", "totalTokens")
_OBJECT_TYPES = frozenset({"chat.completion", "chat.completion.chunk", "text_completion", "response"})
_TYPE_VALUES = frozenset({"message", "message_start", "content_block_delta", "content_block_start", "message_delta", "message-start", "content-delta", "message-end"})


def _response_objects(text: str, sse: bool) -> Iterator[dict]:
    from .parse.response import extract_eventstream_objects

    raw = (text or "").strip()
    if not raw:
        return
    n = 0
    if sse or raw.startswith(("data:", "event:")):
        lines = raw.splitlines()
        # Check the first 100 and last 100 lines first (where start/delta/usage events live)
        scan_lines = lines if len(lines) <= 240 else (lines[:120] + lines[-120:])
        for line in scan_lines:
            if line.startswith("data:"):
                payload = line[5:].strip()
                if payload and payload != "[DONE]":
                    try:
                        o = json.loads(payload)
                    except ValueError:
                        continue
                    if isinstance(o, dict):
                        yield o
                        n += 1
                        if n >= 200:
                            return
        return
    try:
        o = json.loads(raw)
    except ValueError:
        o = None
    if isinstance(o, dict):
        yield o
        return
    if isinstance(o, list):
        for x in o[:200]:
            if isinstance(x, dict):
                yield x
        return
    yielded = False
    for line in raw.splitlines()[:200]:  # NDJSON
        line = line.strip()
        if not line:
            continue
        try:
            x = json.loads(line)
        except ValueError:
            yielded = False
            break
        if isinstance(x, dict):
            yielded = True
            yield x
    if not yielded:
        for x in extract_eventstream_objects(raw)[:200]:
            yield x


def response_evidence(text: str, sse: bool = False) -> Optional[str]:
    """Return what in the response proves it came from a model, or None."""
    for o in _response_objects(text, sse):
        u = o.get("usage")
        if isinstance(u, dict) and (any(k in u for k in _USAGE_KEYS) or isinstance(u.get("tokens") or u.get("billed_units"), dict)):
            return "response has token usage"
        meta = o.get("meta") or o.get("metadata")
        if isinstance(meta, dict) and isinstance(meta.get("usage") or meta.get("tokens") or meta.get("billed_units"), dict):
            return "response has token usage"
        if isinstance(o.get("amazon-bedrock-invocationMetrics"), dict):
            return "response has Bedrock invocation metrics"
        if isinstance(o.get("usageMetadata"), dict):
            return "response has usageMetadata"
        ch = o.get("choices")
        if isinstance(ch, list) and any(isinstance(c, dict) and ("message" in c or "delta" in c or "text" in c) for c in ch):
            return "response has choices"
        if isinstance(o.get("candidates"), list) and o["candidates"]:
            return "response has candidates"
        if o.get("object") in _OBJECT_TYPES or o.get("type") in _TYPE_VALUES:
            return "response has a model-response type"
        if isinstance(o.get("contentBlockDelta") or o.get("contentBlockStart"), dict):
            return "response has Bedrock Converse stream blocks"
        c = o.get("content")
        if isinstance(c, list) and any(isinstance(b, dict) and b.get("type") in ("text", "tool_use", "thinking") for b in c):
            return "response has content blocks"
        if isinstance(o.get("output"), dict) and isinstance((o["output"].get("message") or {}).get("content"), list):
            return "response has Bedrock Converse output"
        if isinstance(o.get("output"), list) and any(isinstance(i, dict) and i.get("type") in ("message", "function_call", "reasoning") for i in o["output"]):
            return "response has Responses output items"
        if "prompt_eval_count" in o or "eval_count" in o or "tokens_evaluated" in o or "generated_text" in o:
            return "response has self-hosted server counters"
        if o.get("done") is not None and ("response" in o or isinstance(o.get("message"), dict)):
            return "response looks like an Ollama answer"
    return None


# ---- configuration and state --------------------------------------------------------------------------------------


def _split_list(value: Optional[str]) -> list:
    return [x.strip() for x in (value or "").split(",") if x.strip()]


@dataclass
class DetectConfig:
    hosts: list = field(default_factory=list)  # force detection: host, host:port, *.suffix, or CIDR
    paths: list = field(default_factory=list)  # force detection: regular expressions matched against the path
    exclude_hosts: list = field(default_factory=list)

    @classmethod
    def from_env(cls, env: Optional[dict] = None) -> "DetectConfig":
        e = os.environ if env is None else env
        return cls(_split_list(e.get("TOKPROF_LLM_HOSTS")), _split_list(e.get("TOKPROF_LLM_PATHS")), _split_list(e.get("TOKPROF_EXCLUDE_HOSTS")))


@lru_cache(maxsize=256)
def _parse_cidr(cidr: str):
    try:
        return ipaddress.ip_network(cidr, strict=False)
    except ValueError:
        return None


def host_matches(patterns: list, host: str, port: int) -> bool:
    if not patterns:
        return False
    h = (host or "").lower().strip("[]")
    if "%" in h:
        h = h.split("%", 1)[0]
    for pat in patterns:
        p = pat.lower()
        if "/" in p:
            net = _parse_cidr(p)
            if net is not None:
                try:
                    if ipaddress.ip_address(h) in net:
                        return True
                except ValueError:
                    pass
            continue
        want_port = None
        if p.count(":") == 1 and p.rsplit(":", 1)[1].isdigit():
            p, want_port = p.rsplit(":", 1)[0], int(p.rsplit(":", 1)[1])
        if want_port is not None and want_port != port:
            continue
        if fnmatch.fnmatchcase(h, p):
            return True
    return False


@dataclass
class Verdict:
    llm: bool = False
    candidate: bool = False  # plausible but ambiguous: decide when the response arrives
    api: Optional[str] = None
    score: int = 0
    evidence: list = field(default_factory=list)
    host_class: str = ""
    model_hint: Optional[str] = None
    stream_hint: bool = False
    forced: bool = False
    reject: Optional[str] = None


@dataclass
class EndpointStat:
    host: str
    port: int
    host_class: str
    calls: int = 0
    apis: dict = field(default_factory=dict)
    paths: dict = field(default_factory=dict)
    how: set = field(default_factory=set)
    confirmed: bool = False


class Detector:
    """Stateful: remembers which endpoints proved to be LLM servers and what looked like one but was not."""

    def __init__(self, config: Optional[DetectConfig] = None):
        self.cfg = config or DetectConfig.from_env()
        self._path_rx = []
        for p in self.cfg.paths:
            try:
                self._path_rx.append(re.compile(p, re.I))
            except re.error:
                continue
        self._lock = threading.Lock()
        self._known: dict = {}  # (host, port, path) -> api of a confirmed LLM endpoint; per path, because one host can serve several APIs
        self.endpoints: dict = {}
        self.missed: dict = {}  # (host, port, path) -> count of unconfirmed candidates
        self.rejected: dict = {}  # reason -> count

    # -- request stage --
    def inspect(self, *, method: str, host: str, port: int, path: str, body: Any) -> Verdict:
        v = Verdict()
        hostinfo = classify_host(host, port)
        v.host_class = hostinfo.cls
        if (method or "").upper() != "POST":
            return self._reject(v, "not a POST request")
        npath = normalize_path(path)
        if host and host_matches(self.cfg.exclude_hosts, host, port):
            return self._reject(v, "host excluded by --exclude-host")
        m = EXCLUDED_PATH.search(npath)
        if m:
            return self._reject(v, f"non-generative endpoint ({m.group(0).strip('/:')})")
        shape = classify_body(body)
        hint = path_hint(npath)
        v.model_hint = model_from_path(npath)
        v.stream_hint = bool(_STREAM_PATH.search(npath))
        forced = bool(host and host_matches(self.cfg.hosts, host, port)) or any(rx.search(npath) for rx in self._path_rx)

        api = shape.api
        if shape.strong and shape.api in (OPENAI_CHAT, ANTHROPIC) and hint in (OPENAI_CHAT, ANTHROPIC):
            api = hint  # /messages vs /chat/completions settles an ambiguous chat body
        v.api = api
        if api is None:
            return self._reject(v, "forced by --llm-host/--llm-path but the body has no recognizable LLM shape" if forced else "body has no LLM request shape")

        score = shape.points
        v.evidence += shape.reasons
        if hint and hint != "generic":
            if hint == api:
                score += 25
                v.evidence.append(f"path looks like the {hint} API")
        elif hint == "generic":
            score += 10
            v.evidence.append("path looks like a model invocation endpoint")
        score += hostinfo.points
        v.evidence += hostinfo.reasons
        with self._lock:
            known = (host, port, npath) in self._known
        if known and not shape.strong:
            score += 30
            v.evidence.append("this endpoint already returned a model answer in this session")
        if forced:
            score += 100
            v.forced = True
            v.evidence.append("forced by --llm-host/--llm-path")
        v.score = score
        if score >= ACCEPT:
            v.llm = True
        elif score >= CANDIDATE:
            v.candidate = True
        else:
            return self._reject(v, "evidence too weak")
        return v

    def _reject(self, v: Verdict, reason: str) -> Verdict:
        v.reject = reason
        with self._lock:
            self.rejected[reason] = self.rejected.get(reason, 0) + 1
        return v

    # -- response stage --
    def confirm(self, v: Verdict, host: str, port: int, path: str, text: str, sse: bool) -> Optional[str]:
        """Decide a candidate from its response. Returns the evidence, or None (and counts it as missed)."""
        ev = response_evidence(text, sse)
        with self._lock:
            if ev is None:
                key = (host, port, normalize_path(path))
                if len(self.missed) < 4096 or key in self.missed:
                    self.missed[key] = self.missed.get(key, 0) + 1
                return None
        v.evidence.append(ev)
        v.score += 40
        v.llm, v.candidate = True, False
        return ev

    def accept(self, v: Verdict, host: str, port: int, path: str) -> None:
        """Record an accepted call; a confirmed (not merely request-shaped) call makes the endpoint known."""
        with self._lock:
            st = self.endpoints.get((host, port))
            if st is None:
                if len(self.endpoints) >= 1024:
                    self.endpoints.pop(next(iter(self.endpoints)), None)
                st = self.endpoints[(host, port)] = EndpointStat(host, port, v.host_class)
            st.calls += 1
            st.apis[v.api] = st.apis.get(v.api, 0) + 1
            np_ = normalize_path(path)
            if len(st.paths) < 8 or np_ in st.paths:
                st.paths[np_] = st.paths.get(np_, 0) + 1
            st.how.add(v.evidence[0] if v.evidence else "")
            if v.forced or any(e.startswith("response has") for e in v.evidence) or v.score >= 60:
                st.confirmed = True
                if len(self._known) >= 4096 and (host, port, np_) not in self._known:
                    self._known.pop(next(iter(self._known)), None)
                self._known[(host, port, np_)] = v.api

    # -- reporting --
    def summary(self) -> list:
        with self._lock:
            return sorted(
                ({"host": s.host, "port": s.port, "class": s.host_class, "calls": s.calls, "apis": dict(s.apis),
                  "paths": dict(s.paths), "evidence": sorted(x for x in s.how if x), "confirmed": s.confirmed}
                 for s in self.endpoints.values()),
                key=lambda d: -d["calls"],
            )

    def render_summary(self) -> str:
        rows = self.summary()
        lines = []
        if rows:
            lines.append("LLM endpoints detected:")
            for r in rows:
                where = f"{r['host']}:{r['port']}" if r["port"] else r["host"] or "(unknown host)"
                apis = ", ".join(f"{a}x{n}" for a, n in r["apis"].items())
                lines.append(f"  {where}  [{r['class']}]  {r['calls']} call(s)  {apis}")
                lines.append(f"      paths: {', '.join(r['paths'])}")
                if r["evidence"]:
                    lines.append(f"      why:   {r['evidence'][0]}")
        with self._lock:
            missed = dict(self.missed)
        if missed:
            total = sum(missed.values())
            top = sorted(missed.items(), key=lambda kv: -kv[1])[:3]
            lines.append(f"{total} JSON POST request(s) looked LLM-like but their responses did not, so they were not recorded:")
            for (h, p, path), n in top:
                lines.append(f"  {h}:{p}{path}  x{n}")
            lines.append("  If one of these is a model endpoint, force it with --llm-host HOST[:PORT] or --llm-path REGEX.")
        return "\n".join(lines)


def explain(url: str, body: Any = None, *, config: Optional[DetectConfig] = None, response: Optional[str] = None, sse: bool = False) -> str:
    """Human-readable verdict for one request, for `tokprof explain`."""
    from urllib.parse import urlsplit

    u = urlsplit(url)
    port = u.port or (443 if u.scheme == "https" else 80)
    det = Detector(config)
    v = det.inspect(method="POST", host=u.hostname or "", port=port, path=u.path, body=body)
    if v.candidate and response is not None:
        det.confirm(v, u.hostname or "", port, u.path, response, sse)
    status = "LLM call" if v.llm else ("candidate (decided by the response)" if v.candidate else "not recorded")
    lines = [f"verdict: {status}", f"score:   {v.score} (accept >= {ACCEPT}, candidate >= {CANDIDATE})"]
    if v.api:
        lines.append(f"api:     {v.api}")
    lines.append(f"host:    {u.hostname}:{port}  [{v.host_class}]")
    if v.model_hint:
        lines.append(f"model from URL: {v.model_hint}")
    for e in v.evidence:
        lines.append(f"  + {e}")
    if v.reject:
        lines.append(f"  - {v.reject}")
    if not v.llm and not v.reject and v.candidate:
        lines.append("  ? ambiguous body: it is recorded only if the response looks like a model answer")
    if not v.llm:
        lines.append("Force it with --llm-host HOST[:PORT] or --llm-path REGEX if this is a model endpoint.")
    return "\n".join(lines)
