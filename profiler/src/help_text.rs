pub const HELP_EN: &str = "\
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  tokprof — Zero-Code LLM Agent Token Profiler                                            │
│  Break down input tokens per Session, per Turn, and per Call without modifying code      │
╰──────────────────────────────────────────────────────────────────────────────────────────╯

OVERVIEW
  tokprof captures LLM API requests from running services, Docker containers, or CLI commands
  without changing a single line of agent code. Every model call is decomposed into:
    • System Prompt (split by markdown headings / XML tags)
    • Conversation History (prior user & assistant turns, including JSON-embedded context)
    • Current User Prompt (including structured JSON utterance payloads)
    • Tools & Response Schema (tool definitions, tool calls, tool results, JSON schemas)
    • Assistant Output & Cached / Reasoning token counts + full prompt/response texts

LANGUAGE SELECTION
  • English (default):  tokprof --help              or  tokprof --lang en
  • Korean (한국어):     tokprof --lang ko           or  TOKPROF_LANG=ko tokprof

QUICK START & EXAMPLES
  1) Profile already-running host services & Docker containers (recommended on Linux):
       $ tokprof start                     # attach eBPF to all processes & Docker containers
       $ tokprof start --comm fdai         # or filter by process, folder (cwd), or container name
       ... trigger your agent / API / UI / tests ...
       $ tokprof status                    # check captured call count in real time
       $ tokprof stop                      # stop, print visual report, and save .tokprof/latest.{html,csv}

  2) Profile a single command in one step (any OS, any language):
       $ tokprof -- python my_agent.py
       $ tokprof -- pytest tests/
       $ tokprof profile -- node agent.js

  3) View & export reports in multiple formats (pretty, text, html, csv, md, json):
       $ tokprof report                              # pretty terminal report of the latest run
       $ tokprof report --full-prompts               # print full prompt & response texts without truncation
       $ tokprof report --format csv                 # print CSV to stdout (session / turn / call rows + prompts)
       $ tokprof report --format csv --out run.csv   # save CSV to a specific file
       $ tokprof report --csv --html --open          # write .tokprof/latest.csv & latest.html and open browser
       $ tokprof report --watch                      # live-refreshing terminal dashboard (1s interval)
       $ tokprof report --prices prices.json         # include estimated cost from your price table

  4) Private DNS, on-prem vLLM / Ollama / TGI / LiteLLM, and private CAs:
       $ tokprof explain http://vllm.internal:8000/v1/chat/completions --body req.json
       $ tokprof start --llm-host 'gpu*.corp' --llm-path '/internal/ask$' --upstream-ca corp-ca.pem

  5) Compare before vs. after an optimization (95% bootstrap confidence interval):
       $ tokprof compare before.jsonl after.jsonl

COMMANDS
  start      Start a background profiling session (Linux: auto-attaches eBPF to host & Docker)
  status     Show active background session status and recorded LLM call count
  stop       Stop the active session, print the visual report, and write .tokprof/latest.{html,csv}
  profile    One-step profile: run a command (or attach to --pid/--comm) and emit report + HTML/CSV
  report     Render a recorded JSONL run (defaults to the most recent file in .tokprof/)
               --format {pretty,text,html,csv,md,json}   Output format (default: pretty)
               --csv [FILE]                              Also write CSV file (default: <run>.csv & latest.csv)
               --html [FILE]                             Also write HTML dashboard (default: <run>.html & latest.html)
               --full-prompts                            Show untruncated prompt & response texts in terminal
               --watch                                   Live-refreshing terminal view
  run        Run a command with LLM base URLs & in-process hooks routed through the local gateway
  shell      Open an interactive subshell where all child processes are profiled
  ebpf       Linux: capture LLM calls (HTTPS & plain HTTP/h2c) of running processes via eBPF
  mitm       Windows/macOS/Linux: capture LLM calls via mitmproxy (--local or --mode)
  pull       Pull LLM calls from Azure Application Insights / Log Analytics (--workspace or --app-insights)
  import     Import Application Insights, OTel GenAI/OTLP, Bedrock, Langfuse/LangSmith, or container logs
  compare    Compare two JSONL runs (input, output, calls, latency, paired CI & --max-regression gate)
  export     Export records in the legacy examples/token_report.py JSONL schema
  explain    Show how tokprof classifies a request URL + JSON body and preview segments
  clean      Remove older .jsonl/.jsonl.gz/.html/.csv artifacts in .tokprof/ (--keep N, default: 5)
  doctor     Check environment capabilities (--json supported)
  selftest   Verify the full capture & decomposition pipeline against a local mock LLM

GLOBAL & DETECTION OPTIONS
  -h, --help                 Show this detailed help message
  -l, --lang {en,ko}         Help and report UI language: 'en' (English) or 'ko' (Korean / 한국어)
  --version                  Show tokprof version
  --session-name NAME        Isolate background start/status/stop state for parallel experiments
  --upstream-timeout DUR     Socket timeout for slow reasoning models (default: 600s; e.g. 15m, 1800s)
  --prices {auto,FILE}       Estimate USD cost using built-in model rates ('auto') or a custom JSON file
  --redact-pii               Also redact email addresses, phone numbers, and national ID patterns
  --redact-pattern REGEX     Additional custom regex to redact from stored prompt/response texts
  --llm-host HOST[:PORT]     Force matching hosts (glob or CIDR) to be treated as LLM endpoints
  --llm-path REGEX           Force matching URL paths to be treated as LLM endpoints
  --exclude-host HOST[:PORT] Never treat matching hosts as LLM endpoints
  --upstream-ca FILE         Trust a custom CA bundle for upstream TLS connections
  --no-content               Omit prompt/response texts from JSONL (store only token counts & hashes)
  --tokenizer SPEC           Tokenizer for local counting (o200k_base, cl100k_base, p50k_base, r50k_base)

Run `tokprof <command> --help` (or `tokprof <command> --help --lang ko`) for command-specific flags.";

pub const HELP_KO: &str = "\
╭──────────────────────────────────────────────────────────────────────────────────────────╮
│  tokprof — 코드 수정 없는 LLM 에이전트 토큰 프로파일러                                   │
│  에이전트 재시작·코드 변경 없이 세션당 · 턴당 · 호출당 토큰과 프롬프트 원문을 상세 분석  │
╰──────────────────────────────────────────────────────────────────────────────────────────╯

개요 (OVERVIEW)
  tokprof는 이미 실행 중인 백그라운드 서비스, Docker 컨테이너, 또는 CLI 명령이 수행하는 LLM 호출을
  코드 수정 없이 감지하여 다음 항목으로 정밀 분해합니다:
    • 시스템 프롬프트 (System Prompt — 마크다운 제목 / XML 태그 구간별 자동 분할)
    • 대화 이력 (Conversation History — 이전 사용자·어시스턴트 턴 및 JSON 내장 context 분리)
    • 현재 사용자 프롬프트 (Current User Prompt — 구조화된 JSON utterance 페이로드 자동 추출)
    • 도구 및 응답 스키마 (Tools & Schema — 도구 정의, 도구 호출, 도구 결과, JSON 응답 스키마)
    • 어시스턴트 출력 및 캐시 적중(Cached) / 추론(Reasoning) 토큰 + 프롬프트·응답 전체 원문

언어 선택 (LANGUAGE SELECTION)
  • 한국어 도움말:  tokprof --lang ko           또는  TOKPROF_LANG=ko tokprof
  • 영어 도움말:    tokprof --lang en           또는  tokprof --help

빠른 시작 및 사용 예시 (QUICK START & EXAMPLES)
  1) 이미 실행 중인 서비스 및 Docker 컨테이너 프로파일링 (Linux 권장 워크플로):
       $ tokprof start                     # 호스트의 모든 프로세스 및 Docker 컨테이너 자동 감지 시작
       $ tokprof start --comm fdai         # 특정 프로세스명·작업 폴더(cwd)·컨테이너 이름만 필터링
       ... 에이전트 질문, API 호출, 콘솔 작업 또는 테스트 수행 ...
       $ tokprof status                    # 현재까지 수집된 LLM 호출 수 실시간 확인
       $ tokprof stop                      # 수집 종료 + 시각적 리포트 출력 + .tokprof/latest.{html,csv} 저장

  2) 단일 명령어를 한 번에 실행하고 프로파일링 (모든 OS, 모든 언어):
       $ tokprof -- python my_agent.py
       $ tokprof -- pytest tests/
       $ tokprof profile -- node agent.js

  3) 다양한 포맷으로 결과 조회 및 내보내기 (pretty, text, html, csv, md, json):
       $ tokprof report                              # 가장 최근 실행 결과를 시각적 터미널 표로 출력
       $ tokprof report --full-prompts               # 프롬프트와 응답 원문 전체를 생략 없이 터미널에 출력
       $ tokprof report --format csv                 # 세션/턴/호출별 토큰 및 프롬프트 원문을 CSV로 표준출력
       $ tokprof report --format csv --out run.csv   # CSV 파일로 직접 저장 (엑셀 / 판다스 분석용)
       $ tokprof report --csv --html --open          # .tokprof/latest.csv 및 latest.html 생성 후 브라우저 열기
       $ tokprof report --watch                      # 호출 발생 시 1초 주기로 갱신되는 실시간 터미널 대시보드
       $ tokprof report --prices prices.json         # 단가표(1M 토큰당 가격) 기반 예상 비용 계산 추가

  4) 사설 DNS, 온프레미스 vLLM / Ollama / TGI / LiteLLM 및 사설 인증서(CA) 환경:
       $ tokprof explain http://vllm.internal:8000/v1/chat/completions --body req.json
       $ tokprof start --llm-host 'gpu*.corp' --llm-path '/internal/ask$' --upstream-ca corp-ca.pem

  5) 최적화 전·후 효과 검증 (95% 부트스트랩 신뢰구간 비교):
       $ tokprof compare before.jsonl after.jsonl

명령어 목록 (COMMANDS)
  start      백그라운드 프로파일링 세션 시작 (Linux: 실행 중인 프로세스 및 Docker 컨테이너 eBPF 자동 연결)
  status     실행 중인 세션 상태 및 현재까지 기록된 LLM 호출 수 확인
  stop       세션 종료 후 즉시 시각적 터미널 리포트 출력 및 .tokprof/latest.{html,csv} 파일 생성
  profile    원스텝 실행: 명령 실행(또는 --pid/--comm 연결) 후 리포트 출력 및 HTML/CSV 대시보드 저장
  report     기록된 JSONL 파일 요약 (파일 경로 생략 시 .tokprof/ 내 최신 파일 자동 선택)
               --format {pretty,text,html,csv,md,json}   출력 포맷 선택 (기본값: pretty)
               --csv [FILE]                              CSV 파일 추가 생성 (기본: <run>.csv 및 .tokprof/latest.csv)
               --html [FILE]                             단일 파일 HTML 대시보드 생성 (기본: <run>.html 및 latest.html)
               --full-prompts                            터미널에서 프롬프트·응답 원문을 길이 제한 없이 전체 출력
               --watch                                   실시간 터미널 대시보드 모드
  run        로컬 게이트웨이 및 프로세스 내부 훅(Python/Node)을 통해 명령 실행 및 캡처
  shell      하위 프로세스가 모두 프로파일링되는 대화형 셸 열기
  ebpf       Linux: 재시작 없이 실행 중인 프로세스의 TLS/평문 HTTP 호출을 eBPF로 직접 캡처
  mitm       Windows/macOS/Linux: mitmproxy를 통한 프로세스별 로컬 캡처 (--local 또는 --mode)
  pull       Azure Application Insights / Log Analytics에서 배포된 에이전트 로그를 직접 조회·분석
  import     Application Insights, OTel/OTLP, Bedrock, Langfuse/LangSmith, 컨테이너 로그를 JSONL로 변환
  compare    개선 전·후 두 JSONL 실행의 토큰·출력·지연시간·에러율 검증 및 CI 게이트(--max-regression)
  export     examples/token_report.py 호환 JSONL 스키마로 내보내기
  explain    특정 URL과 요청 본문의 LLM 판정 점수·이유 및 세그먼트 미리보기 출력
  clean      .tokprof/ 내 오래된 아티팩트 정리 (--keep N, 기본 최근 5개 유지)
  doctor     현재 머신의 커널 eBPF/BTF, 토크나이저, Node, mitmproxy 지원 상태 점검 (--json 지원)
  selftest   내장 모의(mock) LLM 서버로 캡처·분해 파이프라인 전체 자동 검증

공통 및 감지 옵션 (GLOBAL & DETECTION OPTIONS)
  -h, --help                 상세 도움말 및 사용 예시 출력
  -l, --lang {en,ko}         도움말 및 리포트 UI 언어 선택: 'en'(영어) 또는 'ko'(한국어)
  --version                  버전 출력
  --session-name NAME        병렬 실험을 위한 백그라운드 세션 상태 격리
  --upstream-timeout DUR     장시간 추론 모델을 위한 업스트림 타임아웃 (기본값: 600s; 예: 15m)
  --prices {auto,FILE}       내장 모델 단가표('auto') 또는 사용자 JSON 단가표로 예상 비용 산출
  --redact-pii               이메일·전화번호 등 개인식별정보(PII) 추가 마스킹
  --redact-pattern REGEX     사용자 지정 정규식 패턴 추가 마스킹
  --llm-host HOST[:PORT]     지정한 호스트(와일드카드 또는 CIDR)를 LLM 엔드포인트로 강제 인식
  --llm-path REGEX           정규식과 일치하는 경로를 LLM 엔드포인트로 강제 인식
  --exclude-host HOST[:PORT] 지정한 호스트를 캡처 대상에서 제외
  --upstream-ca FILE         사설 CA 인증서 번들 파일 신뢰 추가
  --no-content               JSONL에 프롬프트·응답 원문 저장 생략 (토큰 수와 해시만 저장)
  --tokenizer SPEC           로컬 토큰 계산용 토크나이저 지정 (o200k_base, cl100k_base, p50k_base, r50k_base)

개별 명령어의 상세 옵션은 `tokprof <명령어> --help --lang ko`로 확인할 수 있습니다.";

pub fn resolve_lang(explicit: Option<&str>) -> &'static str {
    let raw = explicit
        .map(|s| s.to_string())
        .or_else(|| std::env::var("TOKPROF_LANG").ok())
        .unwrap_or_else(|| "en".to_string());
    if raw.trim().to_ascii_lowercase().starts_with("ko") {
        "ko"
    } else {
        "en"
    }
}

pub fn subcommand_help_ko(subcmd: &str) -> Option<&'static str> {
    match subcmd {
        "start" => Some("백그라운드 프로파일링 세션을 시작합니다 (Linux에서는 실행 중인 모든 프로세스와 Docker 컨테이너에 eBPF를 자동 연결합니다).\n  예시:\n    tokprof start\n    tokprof start --comm fdai\n    tokprof start --pid 1234,5678"),
        "stop" => Some("실행 중인 백그라운드 세션을 종료하고, 시각적 리포트 출력과 .tokprof/latest.html 및 .tokprof/latest.csv 저장을 수행합니다.\n  예시:\n    tokprof stop\n    tokprof stop --csv my_report.csv --html my_report.html"),
        "status" => Some("현재 활성화된 백그라운드 프로파일링 세션과 기록된 LLM 호출 수를 표시합니다.\n  예시:\n    tokprof status"),
        "profile" => Some("명령어를 실행하거나 실행 중인 프로세스(--pid/--comm)에 연결하여 프로파일링한 뒤 리포트와 HTML/CSV를 생성합니다.\n  예시:\n    tokprof profile -- python my_agent.py\n    tokprof profile --comm python,node --duration 30s"),
        "report" => Some("기록된 JSONL 파일을 분석해 세션당·턴당·호출별 토큰 분해와 프롬프트 원문을 출력합니다.\n  예시:\n    tokprof report\n    tokprof report --format csv\n    tokprof report --format csv --out result.csv\n    tokprof report --full-prompts\n    tokprof report --html --csv --open"),
        "run" => Some("LLM 기본 URL 환경변수 리다이렉트 및 Python/Node 프로세스 내부 훅을 적용하여 명령을 실행합니다.\n  예시:\n    tokprof run --report --csv -- python agent.py"),
        "ebpf" => Some("Linux: 이미 실행 중인 프로세스 및 컨테이너의 TLS/평문 HTTP 호출을 eBPF로 캡처합니다.\n  예시:\n    tokprof ebpf --comm fdai --report\n    tokprof ebpf --all --duration 1m --report"),
        "mitm" => Some("mitmproxy를 사용해 선택된 프로세스의 LLM 호출을 캡처합니다 (Windows/macOS/Linux).\n  예시:\n    tokprof mitm --trust-help\n    tokprof mitm --local python,node --report"),
        "pull" => Some("Azure Application Insights 또는 Log Analytics 워크스페이스에서 배포된 에이전트의 LLM 호출 로그를 가져와 세그먼트 분해 및 리포트를 생성합니다.\n  예시:\n    tokprof pull --workspace <WORKSPACE_ID> --since 24h\n    tokprof pull --app-insights <APP_ID> --since 2h --conv <OPERATION_ID>\n    tokprof pull --print-kql --since 6h"),
        "import" => Some("Application Insights, OpenTelemetry GenAI, APIM, 또는 컨테이너 로그([TOKPROF_CHUNK]) 파일을 tokprof JSONL로 변환하고 분석합니다.\n  예시:\n    tokprof import appinsights_export.json --report\n    tokprof import container_stdout.log --out .tokprof/run.jsonl --report"),
        "compare" => Some("최적화 전(before)과 후(after) 두 실행 기록을 95% 부트스트랩 신뢰구간(대응표본 자동 감지)으로 비교합니다.\n  예시:\n    tokprof compare before.jsonl after.jsonl --max-regression 5"),
        "explain" => Some("특정 URL과 요청 본문 JSON이 어떻게 분류되는지 점수와 세그먼트 분해 미리보기를 출력합니다.\n  예시:\n    tokprof explain http://vllm.internal:8000/v1/chat/completions --body req.json"),
        "clean" => Some(".tokprof/ 폴더의 오래된 기록(.jsonl, .jsonl.gz, .html, .csv)을 정리합니다 (활성 세션 보호).\n  예시:\n    tokprof clean --keep 5"),
        "doctor" => Some("현재 환경의 토크나이저, Linux eBPF 커널 지원, mitmproxy 설치 여부를 점검합니다 (--json 지원)."),
        "selftest" => Some("로컬 모의(mock) LLM 서버를 띄워 게이트웨이·OTLP·분해 파이프라인 전체를 자체 검증합니다."),
        _ => None,
    }
}

pub fn subcommand_help_en(subcmd: &str) -> Option<&'static str> {
    match subcmd {
        "start" => Some("Start a background profiling session (auto-attaches eBPF on Linux; supports --duration 30m|unlimited and --session-name NAME).\n  Examples:\n    tokprof start\n    tokprof start --comm fdai --duration 30m"),
        "stop" => Some("Stop the active background session, print the terminal report, and write .tokprof/latest.{html,csv}.\n  Examples:\n    tokprof stop\n    tokprof stop --prices auto"),
        "status" => Some("Show active background session health and recorded LLM call count (--json supported).\n  Examples:\n    tokprof status\n    tokprof status --json"),
        "profile" => Some("Run a command or attach to --pid/--comm in one step and emit terminal + HTML/CSV reports.\n  Examples:\n    tokprof profile -- python my_agent.py\n    tokprof profile --comm python,node --duration 60s"),
        "report" => Some("Render a recorded .jsonl or .jsonl.gz run across pretty, text, html, csv, md, or json formats.\n  Examples:\n    tokprof report --prices auto\n    tokprof report --watch\n    tokprof report --format md --lang ko"),
        "run" => Some("Run a command with LLM base URLs, OTLP HTTP endpoint, and Python/Node hooks routed through the local gateway.\n  Examples:\n    tokprof run --report --prices auto -- python agent.py"),
        "ebpf" => Some("Linux: capture HTTPS (OpenSSL, Go crypto/tls) and plain HTTP/h2c LLM calls of running processes.\n  Examples:\n    tokprof ebpf --comm my_agent --report"),
        "mitm" => Some("Capture LLM calls via mitmproxy on Windows, macOS, or Linux.\n  Examples:\n    tokprof mitm --trust-help\n    tokprof mitm --local python,node --report"),
        "pull" => Some("Pull LLM calls from Azure Application Insights / Log Analytics (supports az CLI, Service Principal env, and Managed Identity).\n  Examples:\n    tokprof pull --workspace <WORKSPACE_ID> --since 24h --report"),
        "import" => Some("Import Application Insights, OTel/OTLP, AWS Bedrock Invocation Logs, Langfuse/LangSmith, or container logs.\n  Examples:\n    tokprof import traces.json --out .tokprof/run.jsonl.gz --report"),
        "compare" => Some("Compare before vs. after runs with 95% bootstrap CI (auto-paired by session) and optional CI gate.\n  Examples:\n    tokprof compare before.jsonl after.jsonl --max-regression 5"),
        "explain" => Some("Explain why a URL + JSON body is classified as an LLM call and preview its segment token counts.\n  Examples:\n    tokprof explain https://vllm.internal:8000/v1/chat/completions --body req.json"),
        "clean" => Some("Delete older .jsonl/.jsonl.gz/.html/.csv artifacts in .tokprof/ while keeping the most recent N files (default: 5).\n  Examples:\n    tokprof clean --keep 5"),
        "doctor" => Some("Check platform capabilities (tokenizers, Linux eBPF/BTF/CAP_BPF, mitmproxy, Node, Azure auth; --json supported)."),
        "selftest" => Some("Run end-to-end verification against an in-process mock LLM and OTLP trace span."),
        _ => None,
    }
}

pub fn render_detailed_help(lang: Option<&str>, subcmd: Option<&str>) -> String {
    let loc = resolve_lang(lang);
    let mut text = if let (Some(sc), "ko") = (subcmd, loc) {
        if let Some(desc) = subcommand_help_ko(sc) {
            format!("[한국어 설명 — tokprof {sc}]\n  {desc}\n")
        } else {
            HELP_KO.to_string()
        }
    } else if let Some(sc) = subcmd {
        if let Some(desc) = subcommand_help_en(sc) {
            format!("[tokprof {sc}]\n  {desc}\n")
        } else {
            HELP_EN.to_string()
        }
    } else if loc == "ko" {
        HELP_KO.to_string()
    } else {
        HELP_EN.to_string()
    };
    if !crate::report::utf8_ok() {
        for (a, b) in [
            ("╭", "+"),
            ("╮", "+"),
            ("╰", "+"),
            ("╯", "+"),
            ("─", "-"),
            ("│", "|"),
            ("—", "-"),
            ("·", "/"),
            ("•", "*"),
        ] {
            text = text.replace(a, b);
        }
    }
    text
}
