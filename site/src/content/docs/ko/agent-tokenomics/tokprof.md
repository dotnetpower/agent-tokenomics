---
title: "tokprof: 설치 및 사용법"
description: PyPI에서 tokprof를 설치하고 에이전트 코드 수정 없이 세션별, 턴별, 호출별 LLM 토큰 사용량을 분석합니다.
track: agent
verified: 2026-11-06
sidebar:
  order: 6
---

[`tokprof`](https://pypi.org/project/tokprof/)는 에이전트 코드를 수정하거나 에이전트 프로젝트에 패키지를 추가하지 않고 **모든 LLM 호출의 입력 토큰**을 시스템 프롬프트, 대화 이력, 현재 사용자 프롬프트, 도구 관련 항목(정의, 호출, 결과, 응답 스키마)으로 분해하는 독립 실행형 네이티브 Rust 단일 바이너리 프로파일러입니다.

<a id="1-install"></a>
## 1. 설치

`tokprof`는 Linux(`x86_64`, `aarch64`), macOS(`arm64`, `x86_64`), Windows(`x64`)용 프리빌트 네이티브 Wheel로 [PyPI](https://pypi.org/project/tokprof/)에 배포되어 있습니다. 사용자 PC에 Rust나 Go 컴파일러가 설치되어 있지 않아도 바로 설치됩니다.

```bash
# 독립된 전역 CLI 도구로 설치 (권장)
uv tool install tokprof
# 또는
pipx install tokprof

# 일반 Python 환경에서 pip로 설치
pip install tokprof

# 설치 없이 즉시 실행
uvx tokprof --lang ko
```

<a id="prebuilt-platform-packages"></a>
### 플랫폼별 배포 패키지

| 플랫폼 | 패키지 형식 | 포함 바이너리 |
|--------|-------------|---------------|
| Linux `x86_64` 및 `aarch64` | `manylinux2014` Wheel | `tokprof` + 정적 빌드된 `tokprof-ebpf` 헬퍼 |
| macOS Apple Silicon(`arm64`) 및 Intel(`x86_64`) | `macosx` Wheel | `tokprof` |
| Windows `x64`(`amd64`) | `win_amd64` Wheel | `tokprof.exe` |
| 소스 배포판(`sdist`) / Cargo | `tokprof-*.tar.gz` | Rust 소스에서 직접 빌드 (`cargo install --path ./profiler`) |

설치 후 버전과 현재 머신에서 사용할 수 있는 캡처 기능을 확인합니다.

```bash
tokprof --version
tokprof doctor      # 토크나이저, Linux eBPF 커널 BTF/uprobe 지원, mitmproxy, Node 설치 여부 점검
tokprof selftest    # 로컬 모의(mock) LLM으로 수집·분해 파이프라인 전체 자체 검증
```

한국어 또는 영어로 상세 도움말과 예시를 확인할 수 있습니다.

```bash
tokprof --lang ko         # 한국어 상세 도움말 및 사용 예시 출력
tokprof --help            # 영어 도움말 출력 (기본값)
```

---

<a id="2-quick-start-workflows"></a>
## 2. 빠른 시작 워크플로

<a id="workflow-a--profile-already-running-services--docker-containers-start--status--stop"></a>
### 워크플로 A — 이미 실행 중인 서비스 및 Docker 컨테이너 측정 (`start` / `status` / `stop`)

Linux에서 `tokprof start`를 실행하면 백그라운드 세션을 시작하고, root 권한이나 비밀번호 없는 `sudo`를 사용할 수 있을 때 실행 중인 호스트 프로세스와 Docker 컨테이너에 eBPF 프로브를 자동으로 연결합니다.

```bash
# 1. 백그라운드 측정 시작 (특정 프로세스·폴더·컨테이너만 보려면: tokprof start --comm my_agent)
tokprof start

# 2. 측정할 에이전트 작업, API 요청 또는 테스트를 실행합니다
pytest tests/test_agent.py

# 3. 진행 중 지금까지 기록된 LLM 호출 수를 확인합니다
tokprof status

# 4. 세션을 종료하고 터미널 요약 리포트 출력 및 .tokprof/latest.html 생성을 수행합니다
tokprof stop
```

> **팁(비 Linux 환경 또는 eBPF 권한이 없는 경우):** eBPF를 사용할 수 없는 환경에서는 `tokprof start`가 `eval "$(tokprof env)"` 안내를 출력합니다. 에이전트를 실행할 터미널에서 해당 명령을 실행하면 하위 프로세스의 요청이 로컬 게이트웨이로 전달됩니다.

<a id="workflow-b--profile-a-single-command-in-one-step-tokprof---cmd"></a>
### 워크플로 B — 단일 명령어 원스텝 측정 (`tokprof -- <명령>`)

Linux, macOS, Windows 어디서나 `--` 뒤에 실행할 명령을 적으면 로컬 게이트웨이와 프로세스 내부 훅을 통해 실행한 뒤, 종료 시 터미널 리포트와 `.tokprof/latest.html`을 바로 생성합니다.

```bash
tokprof -- python my_agent.py
tokprof -- pytest tests/
tokprof -- node agent.js
```

<a id="workflow-c--interactive-subshell-tokprof-shell"></a>
### 워크플로 C — 대화형 서브셸 (`tokprof shell`)

하위에서 실행하는 모든 명령을 하나의 세션으로 묶어 기록하는 서브셸을 엽니다.

```bash
tokprof shell
# 서브셸 안에서 여러 에이전트 명령을 실행한 뒤 종료합니다:
exit
```

---

<a id="3-capture-modes"></a>
## 3. 캡처 방식 비교

| 캡처 방식 | 명령어 | 지원 OS | 프로세스 재시작 필요 여부 | 동작 원리 |
|-----------|--------|---------|---------------------------|-----------|
| **로컬 게이트웨이 + 프로세스 내부 훅** | `tokprof -- <cmd>`<br>`tokprof run -- <cmd>`<br>`tokprof shell` | Linux, macOS, Windows | 필요 (`tokprof`가 하위 프로세스 시작) | `OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, `AZURE_OPENAI_ENDPOINT` 및 패턴으로 찾은 `*_BASE_URL` / `*_ENDPOINT` 변수를 `127.0.0.1` 게이트웨이로 라우팅하고, 엔드포인트를 하드코딩한 클라이언트를 위해 Python(`httpx`, `requests`, `urllib3`, `aiohttp`, `urllib.request`) 및 Node(`fetch`, `http`, `https`) 훅을 함께 로드합니다. |
| **Linux eBPF 탭** | `tokprof start`<br>`tokprof ebpf --comm <이름>`<br>`tokprof ebpf --pid <PID>` | Linux (`x86_64`, `ARM64`) | **불필요** (실행 중인 프로세스에 즉시 연결) | 호스트 프로세스와 Docker 컨테이너의 OpenSSL 및 Go `crypto/tls` 라이브러리에 uprobe를 연결하고, 평문 `http://` 및 `h2c` 호출은 시스템 콜 트레이스포인트로 수집합니다. root 또는 `CAP_BPF` 권한이 필요합니다. |
| **mitmproxy 로컬 캡처** | `tokprof mitm --local <프로세스>` | Windows, macOS, Linux | **불필요** (`--local` 모드 사용 시) | `mitmdump`와 `tokprof` 애드온 스크립트로 트래픽을 수집합니다. 프로세스별 CA 인증서 신뢰 설정은 `tokprof mitm --trust-help`로 확인하세요. |
| **Azure Application Insights / Log Analytics** | `tokprof pull --workspace <ID>`<br>`tokprof pull --app-insights <ID>`<br>`tokprof import <파일>` | 모든 OS 및 **Azure Cloud Shell** | **불필요** (수집된 텔레메트리 조회) | `AppDependencies`와 `AppTraces`(OpenTelemetry GenAI, OpenInference, APIM, 또는 `[TOKPROF_CHUNK]` 로그)를 조인해 프롬프트·도구 스키마를 역조립하고 7대 세그먼트 분해 또는 Usage-Only 폴백 리포트를 생성합니다. |

---

<a id="4-reports-html-dashboard-csv-and-beforeafter-comparison"></a>
## 4. 리포트 조회, HTML 대시보드, CSV 내보내기 및 전후 비교

측정이 끝나면 타임스탬프가 붙은 JSONL 로그가 저장되고 `.tokprof/latest.html`이 갱신됩니다.

```bash
# .tokprof/에 기록된 가장 최근 실행의 터미널 요약 리포트 출력
tokprof report

# 잘림 없이 전체 프롬프트와 응답 원문을 터미널에 출력
tokprof report --full-prompts

# 단일 파일 HTML 대시보드(.tokprof/latest.html) 생성 및 브라우저 열기
tokprof report --html --open

# 세션별·턴별·호출별 토큰 분해와 프롬프트 원문을 CSV로 저장
tokprof report --format csv --out run.csv
tokprof report --csv

# 에이전트 실행 중 1초 간격으로 갱신되는 실시간 터미널 대시보드
tokprof report --watch

# 사용자 정의 가격표(1M 토큰당 단가)를 적용해 예상 비용 계산
tokprof report --prices prices.json

# 최적화 전(before)과 후(after) 두 실행을 95% 부트스트랩 신뢰구간으로 비교 (3단계·5단계)
tokprof compare before.jsonl after.jsonl
```

<a id="what-the-html-and-cli-reports-show"></a>
### HTML 및 CLI 리포트에서 확인할 수 있는 정보

1. **입력 토큰 상세 분해(Input token breakdown)**: 시스템 프롬프트(마크다운 제목 및 XML 태그 단위 분할), 도구 정의, 도구 결과, 도구 호출, 대화 이력(이전 사용자·어시스턴트 턴), 현재 사용자 프롬프트(`utterance`, `context`/`history`, `capabilities` 구조화 JSON 페이로드 자동 분리 포함), 응답 스키마, 미분류 템플릿 오버헤드 비중을 보여 줍니다.
2. **세션별·턴별 흐름(Per-Session & Per-Turn lineage)**: 고정 접두부 토큰, 대화 이력 누적, 캐시 적중률, 턴당 호출 수(`calls/turn`)가 턴이 진행됨에 따라 어떻게 변하는지 시각화합니다.
3. **프롬프트 및 도구 스키마 원문 뷰어(Call & Turn Prompt Dossier)**: 각 턴과 개별 호출에서 실제로 전송된 시스템 프롬프트, 대화 이력, 사용자 프롬프트, 도구 스키마, 모델 응답 원문을 확인할 수 있습니다.
4. **자동 진단 결과(Findings & recommendations)**: 호출되지 않은 도구 정의, 동일한 도구 호출·오류가 반복되는 실패 루프, 시스템 프롬프트 내 동적 값으로 인한 프롬프트 캐시 깨짐, 급격한 대화 이력 증가를 자동으로 감지하고 해당하는 트랙 B 단계로 안내합니다.

---

<a id="5-private--on-prem-endpoints-and-privacy"></a>
## 5. 사내·온프레미스 엔드포인트 및 프라이버시

<a id="private-dns-vllm-ollama-tgi-litellm-and-custom-cas"></a>
### 사내 DNS, vLLM, Ollama, TGI, LiteLLM 및 사설 CA 인증서

`tokprof`는 고정된 도메인 목록에 의존하지 않고 요청 본문 구조, URL 경로, 호스트·포트 힌트, 응답 사용량 필드를 종합해 점수를 매깁니다.

```bash
# 특정 URL과 요청 본문 JSON이 LLM 호출로 분류되는지 점수와 판정 이유 확인
tokprof explain http://vllm.internal:8000/v1/chat/completions --body request.json

# 특정 호스트·경로를 강제로 포함하거나 사내 CA 인증서를 신뢰하도록 지정
tokprof start --llm-host 'gpu*.corp' --llm-path '/internal/ask$' --upstream-ca corp-ca.pem
tokprof run --exclude-host telemetry.corp -- python my_agent.py
```

<a id="privacy--redaction"></a>
### 프라이버시 및 마스킹

- **저장하지 않는 항목**: HTTP 헤더(`Authorization`, `x-api-key`)와 API 키는 절대 기록하지 않습니다.
- **파일 권한**: `.tokprof/` 아래의 모든 파일은 `0600`(소유자만 읽기·쓰기) 권한으로 생성됩니다.
- **프롬프트 원문 제외**: `--no-content`(`tokprof start --no-content` 또는 `tokprof run --no-content -- ...`)를 지정하면 프롬프트·응답 원문을 저장하지 않고 토큰 수, 길이, 솔트 처리된 SHA-256 해시만 기록합니다.

---

<a id="6-profiling-agents-already-deployed-on-azure-cloud-shell--application-insights"></a>
## 6. 이미 Azure에 배포된 에이전트 프로파일링 (Cloud Shell & Application Insights)

이미 Azure에 배포되어 운영 중인 에이전트의 경우, 커널 수준 eBPF(`CAP_BPF`)는 Linux VM이나 특권(privileged) AKS 노드에서만 사용할 수 있습니다. App Service, Container Apps(ACA), Azure Functions, Foundry Hosted Agent 같은 관리형 PaaS/Serverless 환경을 위해 `tokprof`는 `tokprof pull`, `tokprof import`, 그리고 무손실 청크 로그 브리지(`TOKPROF_STDOUT_LOG=1`)를 제공합니다.

<a id="feasibility-matrix-by-azure-hosting-target"></a>
### Azure 호스팅 환경별 가능 / 불가능 매트릭스

| Azure 호스팅 환경 | `tokprof ebpf` | `tokprof run` / 훅 주입 | `tokprof pull` / `import` (App Insights) | 권장 수집 방식 |
|-------------------|----------------|-------------------------|------------------------------------------|----------------|
| **Azure VM / VMSS (Linux)** | **가능 (O)** | **가능 (O)** | **가능 (O)** | SSH/Bastion 접속 후 `sudo tokprof start` (재시작 불필요) |
| **AKS (Kubernetes)** | **조건부 (△)** (`CAP_BPF` 및 `/sys/kernel/btf` 필요) | **가능 (O)** | **가능 (O)** | 특권 디버그 컨테이너/DaemonSet, 사이드카 게이트웨이, 또는 `tokprof pull` |
| **Azure App Service / Container Apps (ACA)** | **불가능 (X)** (커널 샌드박스 격리) | **가능 (O)** (시작 명령 또는 `PYTHONPATH` / `NODE_OPTIONS`) | **가능 (O)** | Application Insights에서 `tokprof pull` 실행 또는 `TOKPROF_STDOUT_LOG=1` 훅 |
| **Azure Functions** | **불가능 (X)** | **훅만 가능 (△)** (`PYTHONPATH` / `NODE_OPTIONS`) | **가능 (O)** | Application Insights에서 `tokprof pull` 실행 또는 `TOKPROF_STDOUT_LOG=1` 훅 |
| **Azure AI Foundry Hosted Agent** | **불가능 (X)** (완전 관리형 런타임) | **불가능 (X)** | **가능 (O)** | Foundry 프로젝트에 연결된 Application Insights / Log Analytics에서 `tokprof pull` 실행 |

<a id="running-tokprof-in-azure-cloud-shell"></a>
### Azure Cloud Shell에서 `tokprof` 설치 및 실행

`tokprof`는 PyPI에 정적 빌드된 `manylinux2014_x86_64` 네이티브 바이너리 Wheel로 배포되어 있어, Azure Cloud Shell에서 Rust나 Go 컴파일러 없이 수 초 내에 설치되며 `$HOME` 영구 스토리지에 유지됩니다.

```bash
# Azure Cloud Shell에 설치
pip install --user tokprof
export PATH="$HOME/.local/bin:$PATH"

# Log Analytics 또는 Application Insights에서 최근 6시간 LLM 호출 수집 및 분석
# (Cloud Shell의 기존 `az` 로그인 세션을 자동으로 사용합니다)
tokprof pull --workspace <LOG_ANALYTICS_WORKSPACE_ID> --since 6h
tokprof pull --app-insights <APP_INSIGHTS_APP_ID> --since 2h --conv <OPERATION_ID>

# Azure Portal Logs에 붙여넣어 확인할 수 있는 KQL 쿼리 출력
tokprof pull --print-kql --since 24h

# 내보낸 JSON, NDJSON, 컨테이너 로그 파일을 오프라인으로 변환 및 분석
tokprof import appinsights_export.json --report
```

`tokprof`가 Azure Cloud Shell 환경(`CLOUD_SHELL=true` 또는 `ACC_CLOUD`)을 감지하면 HTML 리포트 서버 포트를 기본 `18765` 대신 Cloud Shell **Web Preview 허용 포트(`8080`, `8000~8090`, `8888`)**로 자동 바인딩하고, 상단 **[Web Preview] → [Configure] → 포트 8080** 열기 또는 `download .tokprof/latest.html` 명령 안내를 출력합니다.

<a id="application-insights-collection-tiers--lossless-bridge"></a>
### Application Insights 수집 단계(Tier) 및 32KB 한계 우회 브리지

| 텔레메트리 수집 모드 | 7대 입력 세그먼트 분해 (`system`, `tool_def`, `history_*`, `user_prompt`) | 활성화 방법 |
|----------------------|---------------------------------------------------------------------------|-------------|
| **Tier 1 — OTel GenAI 프롬프트 원문 기록** | **가능 (7대 세그먼트 + `sysfp` 지문 완벽 지원)** | 에이전트 환경변수에 `AZURE_TRACING_GEN_AI_CONTENT_RECORDING_ENABLED=true` 또는 `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT=true`를 설정하여 `AppTraces`에 `gen_ai.*.message` 및 `gen_ai.choice` 이벤트가 기록되도록 합니다. |
| **Tier 2 — Usage-Only 폴백 (OTel 기본값)** | **불가 (총 토큰, 비용, 지연 시간, 턴별 추이만 표시)** | 기본 `gen_ai.usage.*` 속성만 있을 때도 에러 없이 `usage_only` 모드로 리포트를 생성하며, 원문 로깅 활성화 방법을 진단 배너로 안내합니다. |
| **Tier 3 — 무손실 청크 브리지 (32KB 초과 대형 프롬프트)** | **가능 (100% 무손실 복원, App Insights 32KB 잘림 우회)** | `tokprof`의 Python/Node 훅(`PYTHONPATH` / `NODE_OPTIONS`)과 함께 `TOKPROF_STDOUT_LOG=1`을 설정하면 20KB 단위로 Gzip+Base64 분할된 `[TOKPROF_CHUNK]` 라인을 컨테이너 로그로 출력하며, `tokprof pull` / `import`가 이를 자동으로 병합·복원합니다. |

<a id="production-operational-safeguards-rbac-law-ingestion-lag-and-cloud-shell-30m-timeout"></a>
### 운영 환경 보호 장치: RBAC 권한 분리, Log Analytics 수집 지연(LAW Lag), Cloud Shell 30분 타임아웃 대응

| 운영 환경 문제 상황 | `tokprof` 내장 자동 대응 메커니즘 | CLI 옵션 및 사용법 |
|---------------------|-----------------------------------|--------------------|
| **워크스페이스 권한 없음 (App Insights 리소스 권한만 보유)** | Log Analytics 워크스페이스 전체 권한 없이 Application Insights 리소스에 대한 `Monitoring Reader`만 있을 때 `/v1/subscriptions/.../query` 경로로 리소스 중심 직접 쿼리 수행 | `tokprof pull --resource-id /subscriptions/<SUB>/.../components/<APP>` |
| **`AppTraces` 테이블 권한 차단 (Table-level RBAC)** | 보안 정책으로 `AppTraces`만 `403` / `InsufficientAccessError`가 발생할 때 자동으로 `AppDependencies` 전용 쿼리로 폴백하여 `usage_only` 리포트 생성 | `tokprof pull` 실행 시 자동 동작 |
| **구독/테넌트 불일치 및 권한 부족 진단** | `401` / `403` / `404`(`InvalidAuthenticationTokenTenant`, `InsufficientAccessError`, `WorkspaceNotFound`) 원인을 분류하고 즉시 실행 가능한 `az account set` / `az role assignment create` 명령 안내 | `tokprof pull` 실행 시 자동 동작 |
| **Log Analytics 수집 지연(2~15분) 및 비동기 도착** | `IngestionTime - TimeGenerated` 지연을 계산하고, 먼저 도착한 `AppDependencies`(`usage_only`)가 나중에 도착한 `AppTraces`와 만날 때 **제자리에서 7대 세그먼트(`tier1_full`)로 자동 승격** | `tokprof pull --workspace <ID> --wait-ingest 5m --poll-interval 15s` |
| **Azure Cloud Shell 20~30분 유휴 타임아웃** | `~/clouddrive/.tokprof`(Azure Files 영구 볼륨)에 `.tokprof/pull-checkpoint.json`과 JSONL을 매 폴링마다 원자적 저장하고, 대기 상한을 25분으로 제한하며 세션 재연결 시 이어받기 지원 | `tokprof pull --resume` |
| **HTTP 429 스로틀링 및 소버린 클라우드** | `Retry-After` / `x-ms-retry-after-ms` 헤더 기반 자동 재시도 및 미국 정부/중국 클라우드(`api.loganalytics.us`, `api.loganalytics.azure.cn`) 지원 | `tokprof pull --cloud {public,usgov,china}` |
| **멀티 인스턴스 중복 로깅 및 Azure 시크릿 마스킹** | `(OperationId, SpanId)` 기준 중복 Span 병합(토큰 이중 합산 방지) 및 Azure `AccountKey=`, `SharedAccessKey=`, SAS `sig=` 토큰 자동 마스킹 | 자동 동작 |

<a id="step-by-step-production-examples-rbac-law-latency-cloud-shell-timeout-portal-csv"></a>
#### 운영 상황별 실전 명령어 예시 (RBAC 우회, LAW 수집 지연 대기, Cloud Shell 재개, 포털 CSV)

```bash
# 1) 리소스 중심 RBAC: 공유 Log Analytics 워크스페이스 권한은 없고
#    개별 Application Insights 리소스에 대한 Monitoring Reader 권한만 있을 때
tokprof pull \
  --resource-id /subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg-agent/providers/Microsoft.Insights/components/my-agent-ai \
  --since 6h

# 2) Log Analytics 수집 지연(2~15분) 대기: 지연 도착하는 AppTraces 메시지 이벤트가 모두 들어올 때까지
#    15초 간격으로 폴링하며 usage_only 레코드를 제자리에서 7대 세그먼트(tier1_full)로 자동 승격
tokprof pull --workspace <WORKSPACE_ID> --since 30m --wait-ingest 5m --poll-interval 15s

# 3) Azure Cloud Shell 30분 타임아웃 복구: 세션 재접속 후 ~/clouddrive/.tokprof/pull-checkpoint.json의
#    타임스탬프 워터마크부터 이어받아 중복 없이 병합
tokprof pull --resume

# 4) Azure Portal CSV 내보내기 파일 오프라인 임포트 (조건부 액세스 등으로 CLI 토큰 발급이 제한된 환경)
tokprof import portal_query_export.csv --out .tokprof/portal.jsonl --report

# 5) 소버린 클라우드 (Azure US Government 또는 Azure China) 조회
tokprof pull --workspace <WORKSPACE_ID> --cloud usgov --since 24h
```

<a id="handling-oversized-logs-controlling-prompt-inclusion-and-verifying-output"></a>
### 대용량 로그 처리, 프롬프트 포함/제외 제어 및 검증 방법

<a id="what-happens-when-logs-or-prompts-are-very-large"></a>
#### 로그나 프롬프트가 매우 큰 경우 어떻게 처리되나요?

`tokprof`는 단일 프롬프트가 수십만 글자이거나 워크스페이스에 수십만 행의 로그가 쌓여 있어도 토큰 계산과 파이프라인이 중단되지 않도록 **4계층 보호 장치**를 적용합니다.

| 보호 계층 | 한도 / 임계치 | 한도 초과 시 동작 방식 | 토큰 수 및 해시 정확도 영향 |
|-----------|---------------|------------------------|------------------------------|
| **1. 게이트웨이 및 훅 캡처** | 게이트웨이: **64 MB** (`MAX_BODY`)<br>Python/Node 훅: **32 MB** (`_CAP`) | 최대 32~64 MB 크기의 요청 본문과 SSE 스트리밍 응답을 유실 없이 수집합니다. | **100% 정확** |
| **2. 세그먼트별 저장 텍스트 상한** | 세그먼트당 **32,768 문자** (`MAX_SEGMENT_TEXT_CHARS`) | **토큰 수(`tokens`), 글자 수(`chars`), SHA-256 해시(`hash`)를 절단 전 원본 전체에 대해 먼저 계산**한 뒤, 디스크(`.jsonl`/`.html`)에 저장하는 미리보기 텍스트(`text`)만 32,768자에서 자르고 `... [truncated, N chars total]` 표시를 남깁니다. | **100% 정확** (미리보기 절단 전에 토큰 계산 완료) |
| **3. App Insights 32KB 컬럼 한도** | `Properties` / `Message` 컬럼당 **32 KB (32,768 B)** | • 단일 패스 `parse_or_repair_json`이 문자열/객체 중간에서 잘린 JSON을 복구해 앞부분 세그먼트를 살리고 `... [truncated by App Insights]`를 표시합니다.<br>• 에이전트에 `TOKPROF_STDOUT_LOG=1`을 켜면 20KB 단위 Gzip+Base64 `[TOKPROF_CHUNK]` 브리지로 **100KB+ 대형 프롬프트도 손실 0%로 복원**합니다. | 기본 로깅 시 절단 이후 구간은 `residual`로 귀속 / 청크 브리지 사용 시 **100% 정확** |
| **4. Log Analytics 쿼리 결과 한도** | 단일 쿼리당 **500,000 행** 또는 **64 MB** (`ResponsePayloadTooLarge`) | • `Accept-Encoding: gzip, deflate, br`로 응답을 수신해 네트워크 전송 크기를 **약 97.7% 축소**합니다.<br>• `ResponsePayloadTooLarge` 발생 시 자동으로 `\| take 20000` 상한을 붙여 재시도하고, 그래도 초과하면 `AppDependencies` 전용 쿼리로 안전하게 폴백합니다.<br>• `--limit <행수>` 및 `--usage-only` 수동 제한도 지원합니다. | 안전 폴백으로 쿼리 실패 방지 |

<a id="three-modes-for-including-or-excluding-prompts"></a>
#### 프롬프트 원문 포함 / 제외 3단계 제어 모드

보안 정책이나 파일 크기 제약에 따라 **수집(`pull`/`start`/`run`) · 변환(`import`) · 리포트(`report`) 모든 단계**에서 프롬프트 포함 여부를 3단계로 선택할 수 있습니다.

| 제어 모드 | CLI 플래그 / 환경변수 | 디스크에 프롬프트 원문 저장 여부 | 7대 세그먼트 분해 및 `sysfp` 해시 | 생성 파일 크기 실측치 (200 호출 / 1,400 세그먼트 기준) |
|-----------|-----------------------|----------------------------------|-----------------------------------|--------------------------------------------------------|
| **1. 프롬프트 포함 (`full-prompts`)** | *(기본값)*<br>터미널 전체 출력: `--full-prompts` | **저장함 (O)** (시크릿 자동 마스킹) | **지원 (7대 세그먼트 + `sysfp`)** | JSONL: `1.50 MB`<br>HTML: `1.90 MB`<br>CSV: `1.17 MB` |
| **2. 세그먼트 분해만 유지 + 원문 제거 (`no-content`)** | `--no-content` 또는 `TOKPROF_NO_CONTENT=1` | **저장 안 함 (`0자`)** | **지원 (모드 1과 토큰·해시 100% 동일)** | JSONL: `384 KB` (**-74.4%**)<br>HTML: `245 KB` (**-87.1%**)<br>CSV: `25.6 KB` (**-97.8%**) |
| **3. 총량만 수집 (`usage-only`)** | `tokprof pull --usage-only`<br>`tokprof import --usage-only` | **저장 안 함 (`0자`, `AppTraces` 미조회)** | **생략** (총 토큰·비용·지연 시간·턴 추이만 집계) | JSONL: `138 KB` (**-90.8%**)<br>HTML: `186 KB` (**-90.2%**)<br>CSV: `27.8 KB` (**-97.6%**) |

```bash
# 모드 1: 프롬프트 및 응답 원문 포함 (기본값) + 터미널에서 생략 없이 전체 원문 출력
tokprof pull --workspace <WORKSPACE_ID> --since 6h
tokprof report --full-prompts

# 모드 2: 7대 세그먼트 토큰 수와 캐시 해시(sysfp)는 100% 계산하되 프롬프트/응답 텍스트만 완전히 제외 (--no-content)
tokprof pull --workspace <WORKSPACE_ID> --since 6h --no-content
tokprof import appinsights_export.json --no-content --out .tokprof/no-content.jsonl

# 이미 프롬프트가 포함되어 저장된 JSONL에서 원문만 제거하여 외부 공유용 HTML / CSV / JSONL 생성
tokprof report .tokprof/full.jsonl --no-content --html shareable.html --csv shareable.csv
tokprof import .tokprof/full.jsonl --no-content --out .tokprof/stripped.jsonl

# 모드 3: 초대형 워크스페이스에서 AppTraces 테이블을 아예 쿼리하지 않고 토큰 총량만 고속 수집 (--usage-only / --limit)
tokprof pull --workspace <WORKSPACE_ID> --since 7d --usage-only --limit 50000
```

<a id="how-to-verify-whether-prompts-were-included-omitted-or-truncated"></a>
#### 프롬프트 포함·제외 및 절단 여부를 실제로 확인(검증)하는 방법

1. **`tokprof pull` / `tokprof import` 실행 시 자동 출력되는 `content verification` 요약 확인:**
   실행 직후 터미널(`stderr`)에 현재 모드, 텍스트가 포함된 세그먼트 비율, 저장된 프롬프트·응답 글자 수, 32KB 절단 복구 건수가 즉시 출력됩니다.
   ```text
   # 기본(full-prompts) 실행 시:
   tokprof: content verification — mode=full-prompts, segments_with_text=1400/1400, stored_prompt_chars=1000580, stored_response_chars=74890, 32k_preview_capped=0, appinsights_32kb_repaired=0

   # --no-content 실행 시 (1,400개 세그먼트는 그대로 유지되면서 저장된 글자 수가 0자임을 즉시 증명):
   tokprof: content verification — mode=no-content, segments_with_text=0/1400, stored_prompt_chars=0, stored_response_chars=0, 32k_preview_capped=0, appinsights_32kb_repaired=0

   # --usage-only 실행 시:
   tokprof: content verification — mode=usage-only, segments_with_text=0/0, stored_prompt_chars=0, stored_response_chars=0, 32k_preview_capped=0, appinsights_32kb_repaired=0
   ```
2. **`jq` 원라이너로 `.jsonl` 파일 내 원문 존재 여부 및 절단 건수 직접 감사:**
   생성된 `.tokprof/*.jsonl` 파일에 프롬프트(`text`)나 응답(`response_text`) 필드가 단 1건이라도 남아 있는지 아래 명령으로 즉시 검증할 수 있습니다.
   ```bash
   jq -s '{
     calls: length,
     total_segments: ([.[].segments | length] | add),
     segments_with_text: ([.[].segments[] | select(.text != null)] | length),
     responses_with_text: ([.[] | select(.response_text != null)] | length),
     appinsights_32kb_repaired: ([.[] | select(.detect.truncated_repaired == true)] | length),
     preview_32k_capped: ([.[].segments[] | select(.text != null and (.text | contains("[truncated,")))] | length)
   }' .tokprof/no-content.jsonl
   ```

---

<a id="7-cli-command-reference"></a>
<a id="6-cli-command-reference"></a>
## 7. 주요 CLI 명령어 요약

| 명령어 | 설명 |
|--------|------|
| `tokprof start [--comm 이름] [--no-content]` | 백그라운드 측정 세션을 시작합니다(Linux에서는 eBPF 자동 연결). |
| `tokprof status` | 활성 백그라운드 세션 상태와 기록된 LLM 호출 수를 확인합니다. |
| `tokprof stop` | 세션을 종료하고 터미널 요약 리포트와 `.tokprof/latest.html`을 생성합니다. |
| `tokprof -- <명령> [인자...]` | 단일 명령을 실행해 프로파일링하고 터미널 및 HTML 리포트를 출력합니다. |
| `tokprof profile [--comm 이름 \| -- <명령>]` | 실행 중인 프로세스 또는 하위 명령을 포그라운드에서 측정합니다. |
| `tokprof report [파일] [--format 형식] [--html] [--csv]` | 기록된 JSONL 로그를 리포트로 출력합니다(`pretty`, `text`, `html`, `csv`, `md`, `json`). |
| `tokprof compare <before.jsonl> <after.jsonl>` | 두 실행 기록을 95% 부트스트랩 신뢰구간으로 비교합니다. |
| `tokprof run -- <명령> [인자...]` | 로컬 게이트웨이와 프로세스 내부 훅을 통해 명령을 실행합니다. |
| `tokprof shell` | 하위 프로세스가 모두 측정되는 대화형 서브셸을 엽니다. |
| `tokprof ebpf (--pid PID \| --comm 이름 \| --all)` | 이미 실행 중인 프로세스에 Linux eBPF 프로브를 연결합니다. |
| `tokprof mitm [--local 프로세스 \| --trust-help]` | Windows, macOS, Linux에서 `mitmproxy`로 트래픽을 캡처합니다. |
| `tokprof pull (--workspace ID \| --app-insights ID \| --resource-id ARM_ID) [옵션]` | Azure Application Insights / Log Analytics에서 배포된 에이전트 호출을 수집·분해합니다(`--since`, `--wait-ingest`, `--resume`, `--no-content`, `--usage-only`, `--limit`, `--cloud`). |
| `tokprof import <파일> [--no-content] [--usage-only] [--report]` | 내보낸 App Insights JSON/CSV, OTel GenAI, APIM, `[TOKPROF_CHUNK]` 로그를 JSONL로 변환합니다. |
| `tokprof explain <url> [--body 파일]` | 특정 URL과 요청 본문이 LLM 호출로 판별되는 점수와 이유를 설명합니다. |
| `tokprof export <in.jsonl> --out <out.jsonl>` | 기록을 `examples/token_report.py` 호환 요약 스키마로 변환합니다. |
| `tokprof doctor` | 로컬 토크나이저, 커널 eBPF 지원, 헬퍼 바이너리 상태를 점검합니다. |
| `tokprof selftest` | 로컬 모의 LLM 서버를 띄워 전체 파이프라인을 자체 테스트합니다. |

<a id="sources"></a>
## 출처

- PyPI 패키지: [PyPI의 `tokprof`](https://pypi.org/project/tokprof/) (2026-11-06 확인)
- 소스 코드 및 구조: [`dotnetpower/agent-tokenomics`의 `profiler/`](https://github.com/dotnetpower/agent-tokenomics/tree/main/profiler)
