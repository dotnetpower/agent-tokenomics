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

<a id="6-cli-command-reference"></a>
## 6. 주요 CLI 명령어 요약

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
| `tokprof explain <url> [--body 파일]` | 특정 URL과 요청 본문이 LLM 호출로 판별되는 점수와 이유를 설명합니다. |
| `tokprof export <in.jsonl> --out <out.jsonl>` | 기록을 `examples/token_report.py` 호환 요약 스키마로 변환합니다. |
| `tokprof doctor` | 로컬 토크나이저, 커널 eBPF 지원, 헬퍼 바이너리 상태를 점검합니다. |
| `tokprof selftest` | 로컬 모의 LLM 서버를 띄워 전체 파이프라인을 자체 테스트합니다. |

<a id="sources"></a>
## 출처

- PyPI 패키지: [PyPI의 `tokprof`](https://pypi.org/project/tokprof/) (2026-11-06 확인)
- 소스 코드 및 구조: [`dotnetpower/agent-tokenomics`의 `profiler/`](https://github.com/dotnetpower/agent-tokenomics/tree/main/profiler)
