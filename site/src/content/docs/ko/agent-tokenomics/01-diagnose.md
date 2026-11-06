---
title: "1. 현재 상태 진단"
description: "LLM 에이전트 토큰 이코노믹스 1단계(진단): 요청별 입력·캐시·출력·추론 토큰, 첫 호출 고정 오버헤드, 턴별 컨텍스트 증가 곡선을 측정합니다."
track: agent
stage: diagnose
verified: 2026-11-03
sidebar:
  order: 1
---

**목표:** 신뢰할 수 있는 기준선을 확보합니다. 아직 최적화하지 마세요.

## 증상

- 청구액이나 토큰 수가 예상보다 많거나, 사용량 증가 속도보다 빠르게 늘어납니다.
- 세션이 길어질수록 느려집니다.
- 비슷한 작업인데 비용이 크게 다릅니다.

<a id="collect-the-request-log"></a>
## 요청 로그 수집

모델 호출마다 한 건씩 기록해야 합니다(필드는 [인덱스](../) 참조). 로그를 얻는 방법은 네 가지입니다.

1. **공급자 사용량 필드** — API 응답마다 반환되는 `usage` 객체(입력, 캐시된 입력, 출력, 추론 토큰)를 기록합니다.
2. **트레이싱 SDK / OpenTelemetry** — 에이전트를 계측해 모델 호출과 도구 호출을 각각 스팬으로 남깁니다.
3. **게이트웨이 / 프록시** — 모델 트래픽을 모든 요청을 기록하는 게이트웨이로 라우팅합니다.
4. **코드 수정 없는 프로파일러** — [`tokprof`](#코드-수정-없이-프로파일링하기-tokprof)는 기존 에이전트를 로컬 게이트웨이 아래에서 실행하고, 호출마다 입력 토큰을 시스템 프롬프트, 이력, 사용자 프롬프트, 도구 항목으로 분해합니다.

아래 플랫폼은 직접 만들거나 운영하는 에이전트에서 2번과 3번 방식을 지원합니다. 하나만 선택하고, 같은 트래픽에 여러 개를 겹쳐 쓰지 마세요.

| 플랫폼 | 방식 | 적합한 용도 | 링크 |
|--------|------|-------------|------|
| **Arize Phoenix** | 트레이싱(OpenTelemetry / OpenInference), 자체 호스팅 가능(라이선스 확인 필요) | 모델, 검색, 도구 호출의 단계별 트레이스를 보고, 평가와 프롬프트 실험을 한곳에서 수행 | [문서](https://arize.com/docs/phoenix), [저장소](https://github.com/Arize-ai/phoenix) |
| **Langfuse** | 트레이싱 SDK 및 통합, 오픈 소스, 자체 호스팅 또는 클라우드 | 생성 단위의 사용량과 유형별(입력, 출력, 캐시 등) 비용, 비용 대시보드, 알림, Metrics API | [토큰 및 비용 추적](https://langfuse.com/docs/observability/features/token-and-cost-tracking), [저장소](https://github.com/langfuse/langfuse) |
| **LangSmith** | 트레이싱(LangChain의 호스팅 플랫폼) | 트레이스별 토큰 및 비용 내역(캐시 읽기를 포함한 입력, 추론을 포함한 출력), 프로젝트 통계와 대시보드 | [비용 추적](https://docs.langchain.com/langsmith/cost-tracking) |
| **Helicone** | 게이트웨이 / 프록시(OpenAI 호환) | 최소한의 코드 변경으로 모든 요청 기록, 공급자 간 대체 경로 | [빠른 시작](https://docs.helicone.ai/getting-started/quick-start), [저장소](https://github.com/Helicone/helicone) |
| **Portkey** | 게이트웨이(통합 API) 및 관측 기능 | 여러 모델을 하나의 인터페이스로 사용하면서 모니터링, 제어, 가드레일 적용 | [문서](https://portkey.ai/docs/introduction/what-is-portkey), [저장소](https://github.com/Portkey-AI/gateway) |

선택 기준은 다음과 같습니다.

- **트레이싱**은 실행의 *구조*(어떤 단계, 어떤 도구, 중첩 관계)를 보여 주므로 루프와 과도하게 큰 도구 출력을
  분리하는 데 가장 적합합니다. 코드나 프레임워크에 계측을 추가해야 합니다.
- **게이트웨이**는 추가하기 가장 쉽고(기본 URL만 변경) 모든 호출을 볼 수 있습니다. 하지만 모델 트래픽만 볼 수 있고
  호출 사이에 일어난 일은 볼 수 없으며, 프롬프트가 지나가는 경로 위에 위치합니다.
- 각 제품의 최신 라이선스, 호스팅 옵션, 가격, 데이터 보존 정책을 확인하고, 필요한 **캐시된 토큰과 추론 토큰**
  필드를 기록하는지도 확인하세요. 기능은 바뀔 수 있으므로 도입하기 전에 검증하세요.
- 프롬프트와 도구 결과에는 비공개 코드나 데이터가 포함되는 경우가 많습니다. 프로덕션 트래픽을 보내기 전에
  **마스킹 / 샘플링**을 설정하고 데이터 처리 약관을 검토하세요.
- 모델 가격표를 내장한 도구는 비용을 대신 계산해 주지만 가격이 최신이 아닐 수 있습니다. 금액은 추정치로 보고
  **토큰 수**를 기본 지표로 유지하세요.

이 중 어느 플랫폼에서든 [`token_report.py`](https://github.com/dotnetpower/agent-tokenomics/tree/main/examples)를
사용하려면, 플랫폼의 데이터를 내보내거나 쿼리한 다음 JSONL 필드(`session`, `turn`, `input_tokens`,
`cached_input_tokens`, `output_tokens`, `tool_defs`, `tool_calls`)에 매핑하세요.

**JSONL 스키마 두 가지:** `.tokprof/*.jsonl`에는 세그먼트별 토큰 추정치, 솔트가 적용된 해시, 비밀값을 마스킹한 프롬프트·응답 텍스트를 담은 `tokprof`의 상세 `v: 1` 레코드가 저장됩니다. `--no-content`를 지정하면 텍스트는 저장하지 않습니다. 반면 `examples/sample-requests.jsonl`은 `examples/token_report.py`가 읽는 간결한 요약 스키마를 사용합니다. 해당 스크립트에 기록을 전달하기 전에 변환하세요.

```bash
tokprof export run.jsonl --out legacy.jsonl
```

마스킹만으로 공유 안전이 보장되지는 않으므로 로그를 먼저 검토하세요.

VS Code에서 GitHub Copilot을 사용하나요? 위 플랫폼은 Copilot을 계측하지 않습니다.
[Agent Debug Logs와 Cache Explorer](../../copilot-tokenomics/02-measure-usage/)를 사용하세요.

<a id="profile-without-changing-code-tokprof"></a>
## 코드 수정 없이 프로파일링하기: tokprof

[`tokprof`](../tokprof/)([PyPI](https://pypi.org/project/tokprof/), 독립 실행형 네이티브 Rust 단일 바이너리, 에이전트 프로젝트에 의존하지 않음)는 이 도구로 시작하거나 연결한 에이전트에 대해 "각 요청에서 어느 부분이 토큰을 쓰는가"에 답합니다.

```bash
pip install tokprof                             # 또는: uv tool install tokprof / uvx tokprof / pipx install tokprof
tokprof --lang ko                               # 상세 사용 가이드 및 예시 출력 (--lang ko | --lang en)
tokprof start                                   # 백그라운드 프로파일링 시작 (Linux: 실행 중인 프로세스 및 Docker 자동 수집)
tokprof stop                                    # 세션 종료 + 시각적 리포트 출력 + .tokprof/latest.html 생성
tokprof -- python my_agent.py                   # 단일 명령 실행: 프로파일링 + 시각적 터미널 리포트 + .tokprof/latest.html 생성
tokprof report --format pretty --html --csv     # 가장 최근 실행을 요약하고 단일 파일 HTML 대시보드 및 CSV 파일을 생성합니다
tokprof report --format csv                     # 세션별·턴별·호출별 분해 및 프롬프트 원문을 CSV로 표준 출력합니다
tokprof report --watch                          # 에이전트 실행 중 실시간으로 터미널 리포트를 갱신합니다
tokprof compare before.jsonl after.jsonl        # 부트스트랩 구간, 3단계와 5단계용
```

모든 캡처 방식과 CLI 명령어, 사내·온프레미스 엔드포인트 설정은 [tokprof: 설치 및 사용법 가이드](../tokprof/)를 참고하세요.

- **수집 방식**: `tokprof start`는 백그라운드 세션을 시작하며, Linux에서는 실행 중인 호스트 프로세스와 Docker 컨테이너에 eBPF 프로브를 자동으로 연결합니다(`tokprof start --comm <이름>`으로 프로세스·작업 디렉터리·컨테이너 이름별 필터링 가능). 작업 후 `tokprof stop`을 실행하면 세션을 멈추고 시각적 리포트와 `.tokprof/latest.html`을 바로 생성합니다. `tokprof run`은 `127.0.0.1`에 게이트웨이를 띄우고, **해당 명령에서만** `OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, `AZURE_OPENAI_ENDPOINT`와 패턴으로 찾은 기본 URL 변수(`VLLM_BASE_URL`, `OLLAMA_HOST` 등)를 그곳으로 향하게 하며, 프로세스 내부 훅(`sitecustomize`를 통한 Python `httpx`, `requests`, `urllib3`, `aiohttp`, `urllib.request`와 `--require`를 통한 Node `fetch`, `http.request`, `https.request`)도 함께 로드합니다.
- **LLM 호출로 인식하는 기준**: 고정된 URL 목록이 아닙니다. `tokprof`는 요청 본문의 형태(`messages` 목록, Gemini `contents`, Responses 방식의 `input`, `prompt`), 경로, 호스트(사설 DNS, RFC 1918, `.svc.cluster.local`, `privatelink`, `vllm`·`ollama` 같은 이름, 관례적인 포트), 그리고 모호한 본문에 대해서는 응답(사용량 카운터, `choices`, Ollama·llama.cpp·TGI 카운터)을 함께 점수화합니다. 그래서 사설 DNS 엔드포인트, 온프레미스 vLLM, Ollama, TGI, llama.cpp, LiteLLM 방식 게이트웨이, Azure 프라이빗 엔드포인트, Gemini를 모두 인식합니다. `tokprof explain <url> --body request.json`은 요청이 기록되었거나 기록되지 않은 이유를 보여 주고, `--llm-host`, `--llm-path`, `--exclude-host`로 판정을 직접 지정할 수 있습니다. 사설 CA는 `SSL_CERT_FILE` 또는 `--upstream-ca`로 신뢰합니다.
- **확인할 수 있는 정보**: 세션별·턴별·호출별로 구분된 시스템 프롬프트(구간별), 대화 이력(History), 현재 사용자 프롬프트(`utterance`, `context`/`history`, `capabilities`가 담긴 구조화 JSON 페이로드 자동 분해 포함), 도구 정의·호출·결과 및 응답 스키마별 입력 토큰과 실제 프롬프트·응답 원문 내용, 고정 오버헤드, 단계별 토큰 지도, 요청당 호출 수, 사용되지 않는 도구, 반복되는 정적 블록, 실패 루프(동일 도구 호출 반복 및 에러 재시도), 프롬프트 캐시 깨짐(동적 값 삽입으로 시스템 프롬프트 해시가 매번 바뀌는 현상), 대화 이력 급증입니다. 터미널(`pretty`, `text`, `--full-prompts`), CSV(`--format csv` / `--csv`), Markdown, JSON 및 외부 의존성 없는 단일 파일 HTML 대시보드(`--html`)로 출력합니다. 인자 없이 `tokprof`를 실행하면 상세 설명과 예시를 보여 줍니다(`--lang ko` 또는 `--lang en`).
- **저장하는 정보**: 토큰 수, 솔트 해시, 길이, 구간·도구 이름 및 시크릿이 마스킹된 프롬프트·응답 원문입니다(원문을 제외하려면 `--no-content` 사용). HTTP 헤더나 API 키는 저장하지 않습니다.
- **정확도**: 제공자는 합계만 알려 주므로 구간별 수치는 로컬 추정치이고 나머지는 *미분류*로 표시됩니다. OpenAI 계열 모델은 `tiktoken`을 설치하세요. 분해 결과를 믿기 전에 잔차를 확인하세요.
- **이미 실행 중인 프로세스(Linux)**: `tokprof ebpf --pid <PID>`는 프로세스의 TLS 라이브러리(OpenSSL 및 Go `crypto/tls`)와 시스템 콜 트레이스포인트에 eBPF 프로브를 연결해, 재시작이나 변경 없이 LLM 호출을 기록하며, 온프레미스 서버로 가는 평문 `http://` 및 `h2c` 호출도 포함합니다. root 또는 `CAP_BPF` 권한이 필요하며, 관찰을 시작한 뒤 열린 HTTP/1.1과 HTTP/2 연결을 따라갑니다. 자세한 내용은 [profiler README](https://github.com/dotnetpower/agent-tokenomics/tree/main/profiler#linux-ebpf-tap-running-processes-no-restart-no-code-change)를 참고하세요.
- **현재 한계**: eBPF 관찰을 제외하면 `tokprof`로 시작한 프로세스만 수집합니다. 기본 URL 환경 변수를 따르지 않으면서 `httpx`, `requests`, `urllib3`, `aiohttp`, `urllib.request`, Node `fetch`, Node `http`/`https` 중 어느 것도 쓰지 않는 클라이언트는 `tokprof run`으로 수집되지 않습니다. Windows와 macOS에서는 `tokprof mitm --local <프로세스>`가 mitmproxy의 로컬 캡처를 사용합니다(관리자 권한과 mitmproxy CA 신뢰가 필요합니다).

## 점검 항목

로그에 예제 스크립트를 실행합니다.

```bash
python examples/token_report.py requests.jsonl
```

기준선을 위해 다음 항목을 기록합니다.

| 지표 | 확인 방법 | 중요한 이유 |
|------|-----------|-------------|
| 요청당 입력 / 캐시된 입력 / 출력 토큰 | 제공자 사용량 필드 | 보통 입력이 대부분이며 캐시된 입력은 더 저렴함 |
| **캐시 적중률(%)** | 캐시된 입력 ÷ 입력 | 접두부가 고정인데 적중률이 낮으면 접두부 변동을 의심 |
| **고정 오버헤드** | 세션에서 *첫 번째* 요청의 입력 토큰 | 시스템 프롬프트 + 지침 + 도구 정의로, 요청마다 비용 발생 |
| **사용자 요청당 LLM 호출 수(팬아웃)** | 요청 ID나 트레이스 ID가 같은 모델 호출 수 | 질문 하나가 여러 호출을 일으킬 수 있으며, 호출마다 자체 프롬프트를 포함 |
| **단계별 토큰 지도** | 단계마다 모델, 호출 수, 호출당 고정 토큰, 동적 토큰 | 어떤 단계와 구성 요소가 비용을 좌우하는지 확인(아래 참조) |
| **커버리지** | 같은 기간 동안 기록된 토큰 합계와 제공자가 청구하거나 모니터링한 토큰 비교 | 차이가 있으면 계측되지 않은 호출(하위 서비스, 임베딩, 백그라운드 작업)이 있음 |
| **도구 정의 토큰** | 전송한 도구 스키마의 합계 | 사용 여부와 관계없이 매 요청마다 지불 |
| **컨텍스트 증가 곡선** | 요청 순번별 입력 토큰 | 가파른 증가는 기록이나 도구 출력이 누적되고 있다는 뜻 |
| **도구별 도구 출력 토큰** | 각 도구 결과가 추가하는 토큰 | 과도하게 큰 결과(대용량 파일 읽기, 장황한 로그)를 찾음 |
| **기록 대비 도구 출력 비중** | 요청마다 증가분을 *대화 텍스트*와 *도구 결과*로 분리 | 기록 요약이 효과가 있는지 판단. 도구 결과가 더 큰 비중인 경우가 많음 |
| **턴당 재시도 / 오류** | 도구 호출 오류 시그니처 | 반복되는 오류는 낭비되는 루프 |
| 성공 작업당 비용 | 비용 ÷ 통과한 작업 수 | 궁극적으로 낮추려는 수치 |

## 단계별 토큰 지도

사용자 요청 하나에 모델 호출을 여러 번 실행하는 에이전트라면 요청 유형마다 표를 하나씩 만드세요. 아래는 합성 파이프라인([`examples/sample-pipeline.json`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-pipeline.json))을 기준으로 작성한 단계별 토큰 지도 예시입니다(실제 로그를 수집하면 `호출당 동적 토큰` 열에 검색 구절·대화 기록·도구 결과 평균치를 함께 채웁니다).

| 단계 | 모델 | 호출 수 | 호출당 고정 토큰 | 호출당 동적 토큰 | 고정 합계 | 비중 | 비고 |
|------|------|--------:|-----------------:|-----------------:|----------:|-----:|------|
| `guardrail` | `small` | 1 | 800 | *(로그에서 측정)* | 800 | 1.0% | `< 1,024 tok`: 캐시 미적용 가능성 |
| `rewrite` | `small` | 1 | 2,500 | *(로그에서 측정)* | 2,500 | 3.2% | |
| `router` | `large` | 1 | 12,000 | *(로그에서 측정)* | 12,000 | 15.2% | 대형 모델 라우팅 오버헤드 |
| `classify` | `small` | 1 | 2,000 | *(로그에서 측정)* | 2,000 | 2.5% | |
| **`answer`** | **`large`** | **2** | **28,000** | *(로그에서 측정)* | **56,000** | **71.1%** | `reference_doc`(20k) + `rules_and_tools`(8k) × 2회 |
| `follow_up` | `small` | 1 | 3,500 | *(로그에서 측정)* | 3,500 | 4.4% | |
| `final_reply` | `large` | 1 | 2,000 | *(로그에서 측정)* | 2,000 | 2.5% | |
| **합계 (요청 1건당)** | — | **8** | — | — | **78,800** | **100.0%** | 동적 토큰을 제외한 하한값 |

**구성 요소**(시스템 프롬프트, 도구 스키마, 출력 스키마, 참조 문서)를 `tokens × calls` 기준으로 순위를 매기세요(`answer/reference_doc`: `20,000 × 2 = 40,000 tok, 50.8%` → `answer/rules_and_tools`: `8,000 × 2 = 16,000 tok, 20.3%` → `router/fixed`: `12,000 × 1 = 12,000 tok, 15.2%`).
상위 한두 개가 대부분의 낭비를 설명하는 경우가 많습니다.

### 프로덕션에 접근할 수 없다면? 정적으로 추정하기

아직 프로덕션 사용량을 읽을 수 없다면, 토크나이저로 코드에서 고정 프롬프트 부분의 토큰 수를 세세요.
[`examples/prompt_budget.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/prompt_budget.py)는
단계를 설명하는 작은 JSON(프롬프트 파일 또는 토큰 수, 요청당 호출 수)을 입력받아 토큰 지도를 출력합니다.

```bash
python3 examples/prompt_budget.py examples/sample-pipeline.json
```

결과는 **하한값**으로 취급하세요. 요청마다 새로 들어오는 모든 내용(사용자 데이터, 검색된 구절, 대화 기록, 도구 결과)이
빠져 있으므로 실제 사용량은 더 많습니다. 후보의 순위를 매기는 데 사용하고, 가능한 한 빨리 실제 측정값으로 확인하세요.
보고서에는 수치가 추정치라는 점을 분명히 밝히세요. 또한 고정 프롬프트가 제공자의 최소 캐시 가능 길이보다 짧은 단계도
표시하세요.

## 해석

| 관찰 | 가능성이 높은 방향(2단계에서 검증) |
|------|-------------------------------------|
| 요청 #1의 오버헤드가 큼 | 큰 지침 또는 도구 카탈로그 |
| 요청 사이의 증가가 가파름 | 큰 도구 출력 또는 정리하지 않은 대화 기록(수정 방법을 고르기 전에 기록 대비 도구 출력 비중을 확인) |
| 캐시 적중률이 낮음 | 요청 사이에 접두부가 바뀜 |
| 한 턴에서 같은 오류 시그니처가 3회 이상 | 실패 루프 |
| 출력 토큰이 많음 | 장황한 답변 또는 과도한 추론 강도 |
| 참조 문서 하나가 한 단계에서 큰 비중을 차지하거나 그 단계가 두 번 이상 호출됨 | 긴 고정 프롬프트 × 호출 수 |
| 사용자 요청당 LLM 호출이 많고, 그중 여러 개가 작은 분류기나 검사임 | 불필요한 파이프라인 단계(팬아웃) |
| 기록된 토큰이 청구된 토큰보다 훨씬 적음 | 계측되지 않은 호출: 결론을 신뢰하기 전에 로그에 추가 |
| 세션별 목록(메시지, 보고서, 도구 결과)이 계속 늘어나기만 함 | 제한 없는 상태: 잘라내는 로직을 찾고, 없으면 추가 |

## 측정 결과를 안전하게 공유하기

진단에는 벤더, 플랫폼 팀, 고객이 관여하는 경우가 많습니다. 필요한 것만 공유하고 그 이상은 공유하지 마세요.

- **공유 후보인 집계 정보:** 집계된 토큰 수, 프롬프트 크기, 호출 수, 적중률, 오류 시그니처
- **공유 전 확인할 정보:** 프롬프트 텍스트, 도구 결과, 검색된 구절, 사용자 또는 고객 데이터, 내부 명칭
- 내보내기 전에 식별자를 마스킹하고 샘플링하거나 집계하세요. 로그와 트레이스 플랫폼은 전송한 내용을 그대로
  저장한다는 점을 기억하세요.
- 누가 무엇을 결정할지 미리 합의하세요. 플랫폼 또는 벤더 팀은 구조와 설정을 제안하고, 도메인 책임자는 요약 후에도
  남겨야 할 사실을 결정하며, 보안 책임자는 보존 또는 데이터 처리 변경을 승인합니다.

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>입력 구성</span><div class="at-diag-bar"><span class="at-diag-seg seg-prefix" style="width: 61.4%"></span><span class="at-diag-seg seg-tool" style="width: 23.0%"></span><span class="at-diag-seg seg-hist" style="width: 6.3%"></span><span class="at-diag-seg seg-user" style="width: 9.3%"></span></div><span>100%</span></div>
</div>
<figcaption>설명용 구성: 시스템 프롬프트 61.4%, 도구 정의 23.0%, 도구 결과 6.3%, 대화 기록과 사용자 입력 9.3%. <a href="https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-requests.jsonl">샘플 로그</a>의 7개 요청에서 도구 정의는 16,800 / 73,000 입력 토큰으로 23.0%입니다. 나머지 구간은 로그에 기록되지 않아 실제 분해 결과가 아닌 가상의 값입니다. <a href="https://github.com/dotnetpower/agent-tokenomics/tree/main/profiler">tokprof</a>의 구간별 추정과 <a href="https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/token_report.py">token_report.py</a>의 집계를 참고하세요.</figcaption>
</figure>

## 적용 예시

샘플 로그에서 `s1`은 첫 요청의 오버헤드가 9,200토큰이며, 그중 도구 정의 두 개(`notebook_edit`, `browser_open`)에 해당하는 도구는
한 번도 호출되지 않습니다. `s1`의 턴 1에는 동일한 `pytest: command not found` 오류가 세 번 있습니다.

파이프라인의 경우 합성 파일
[`sample-pipeline.json`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-pipeline.json)에
정적 추정을 실행합니다(설명용으로 만든 가상의 수치). 요청당 LLM 호출은 8회이고 고정 토큰은 약 78,800개입니다.
답변 단계(두 번 호출됨)에 첨부된 참조 문서 하나가 전체 고정 토큰의 약 절반을 차지하며, 가드레일 단계는 1,024토큰
미만이라 캐시되지 않을 가능성이 높습니다. 표 하나만으로도 어디부터 살펴봐야 하는지 알 수 있습니다.

## 다음 단계

가장 **큰** 의심 신호를 골라 [2. 원인 분리](../02-isolate/)로 이동하세요.
