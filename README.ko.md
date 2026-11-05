# agent-tokenomics — LLM 및 AI 에이전트 토큰 이코노믹스(Tokenomics · Token Economics)

[![Deploy docs site to GitHub Pages](https://github.com/dotnetpower/agent-tokenomics/actions/workflows/pages.yml/badge.svg)](https://github.com/dotnetpower/agent-tokenomics/actions/workflows/pages.yml)
[![VS Code Marketplace - cctx](https://img.shields.io/badge/VS%20Code%20Marketplace-Code%20Context%20(cctx)-3b5bdb)](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)
[![PyPI - tokprof](https://img.shields.io/pypi/v/tokprof?label=PyPI%20tokprof&color=245e63)](https://pypi.org/project/tokprof/)
[![License: MIT](https://img.shields.io/github/license/dotnetpower/agent-tokenomics)](./LICENSE)
[![Docs (KO)](https://img.shields.io/badge/docs-%ED%95%9C%EA%B5%AD%EC%96%B4-3b5bdb)](https://dotnetpower.github.io/agent-tokenomics/ko/)
[![Docs (EN)](https://img.shields.io/badge/docs-English-3b5bdb)](https://dotnetpower.github.io/agent-tokenomics/)
[![llms.txt](https://img.shields.io/badge/GEO-llms.txt-20c997)](https://dotnetpower.github.io/agent-tokenomics/llms.txt)

**AI 도구의 응답 품질은 유지하면서 불필요한 LLM 토큰 소비를 줄이고 개발 속도를 높이세요.**

`agent-tokenomics`는 [microsoft/TokenEconomizer](https://github.com/microsoft/TokenEconomizer)에서 영감을 받아, LLM과 AI 에이전트의 **토큰 이코노믹스(Tokenomics · Token Economics, 토큰 경제학)**—즉 토큰 사용량을 측정·진단·절감하는 실용적인 방법을 두 개의 트랙과 도구로 정리한 한영([English](README.md) / 한국어) 기술 문서 저장소입니다.

- **트랙 A – [Copilot 토큰 절약](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/):** VS Code에서 GitHub Copilot을 사용할 때 토큰과 AI 크레딧 사용량을 줄이는 실천 가이드와, [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)에서 설치해 탐색 토큰 절감·개발 속도 가속(탐색 `15.20배`, 작업 완료 시간 `-24.7%` 보고)·실시간 AIC/USD 절감 대시보드(`cctx usage` 및 상태 표시줄)를 제공하는 **[`Code Context (cctx)`](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/cctx/)**를 다룹니다.
- **트랙 B – [에이전트 토큰 절약](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/):** LLM 에이전트를 개발·운영하는 엔지니어를 위한 6단계 문제 해결 절차(`개념 → 진단 → 분리 → 증명 → 개선 → 결과 → 골든 데이터셋 평가`)입니다.
- **[`tokprof`](https://pypi.org/project/tokprof/) (`pip install tokprof`):** 에이전트 코드를 수정하지 않고 세션별·턴별·호출별 입력 토큰을 시스템 프롬프트, 대화 이력, 사용자 프롬프트, 도구 정의·결과로 분해하고 대화형 HTML 리포트를 생성하는 독립 실행형 Rust 토큰 프로파일러입니다.

> **한국어 문서 사이트:** <https://dotnetpower.github.io/agent-tokenomics/ko/> · **영문 문서 사이트:** <https://dotnetpower.github.io/agent-tokenomics/> · **LLM 색인:** [`llms.txt`](https://dotnetpower.github.io/agent-tokenomics/llms.txt) / [`llms-full.txt`](https://dotnetpower.github.io/agent-tokenomics/llms-full.txt)

---

## 역할별 빠른 시작

| 대상 | 시작할 문서 |
|---|---|
| **VS Code에서 GitHub Copilot을 사용하는 개발자** | [트랙 A: Copilot 토큰 이코노믹스](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/) → [한 페이지 체크리스트](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/10-checklist/) → [`cctx` VS Code 확장 및 가속](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/cctx/) |
| **LLM 에이전트를 개발하거나 운영하는 엔지니어** | [트랙 B: 에이전트 토큰 이코노믹스](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/) → [0. 개념](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/00-concepts/) → [1. 현재 상태 진단](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/01-diagnose/) |
| **코드 수정 없이 에이전트 토큰 사용량을 측정하려는 경우** | [`tokprof` 설치 및 사용법](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/tokprof/) · [대화형 HTML 리포트 샘플](https://dotnetpower.github.io/agent-tokenomics/demo/tokprof-sample.html) |
| **토큰 최적화 전후의 답변 품질을 검증하려는 경우** | [6. 골든 데이터셋과 루브릭 평가](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/06-golden-dataset-and-rubric/) |
| **AI 에이전트 / 생성형 검색 크롤러 (GEO)** | [`llms.txt`](llms.txt) (구조화 색인) · [`llms-full.txt`](https://dotnetpower.github.io/agent-tokenomics/llms-full.txt) (전체 문서 단일 파일 코퍼스) |

---

## 빠른 실행: `tokprof` (코드 수정 없는 LLM 토큰 프로파일러)

[`tokprof`](https://pypi.org/project/tokprof/)는 Linux(`x86_64`, `aarch64`), macOS(`arm64`, `x86_64`), Windows(`x64`)용 프리빌트 네이티브 바이너리를 PyPI로 배포합니다. Rust나 Go 툴체인을 따로 설치할 필요가 없습니다.

```bash
# 1. PyPI에서 설치 (또는 uvx tokprof로 설치 없이 바로 실행)
pip install tokprof
# 또는: uv tool install tokprof

# 2. 백그라운드 측정 세션 시작 (지원되는 Linux에서는 실행 중인 프로세스 및 Docker에 eBPF 자동 연결)
tokprof start

# 3. 에이전트 작업 실행 후 수집 상태 확인 및 리포트 생성
tokprof status
tokprof stop                           # 터미널 요약표 출력 + .tokprof/latest.html 리포트 생성

# 단일 명령을 한 번에 프로파일링하거나 한국어 도움말 보기:
tokprof -- python my_agent.py
tokprof --lang ko
```

자세한 설명은 [profiler/README.md](profiler/README.md) 및 [`tokprof` 설치 및 사용법 가이드](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/tokprof/)를 참조하세요.

---

## 전체 문서 안내

### 트랙 A — VS Code GitHub Copilot (`ko/copilot-tokenomics/`)

모든 문서는 *문제 → 조치 → 확인 방법 → 트레이드오프* 순서로 구성되어 있습니다.

| # | 문서 | 핵심 질문 |
|---|---|---|
| 01 | [Copilot 사용량은 어떻게 과금되는가](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/01-how-usage-is-charged/) | AI 크레딧(토큰 기반 과금)과 프리미엄 요청 배수는 어떻게 다른가요? |
| 02 | [사용량 측정](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/02-measure-usage/) | VS Code 기본 뷰와 디버그 로그로 비용이 큰 요청을 어떻게 찾나요? |
| 03 | [컨텍스트를 가볍게 유지하기](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/03-context-hygiene/) | 새 채팅, `/fork`, `/compact`, `#file` 첨부로 대화 기록 누적을 어떻게 막나요? |
| 04 | [지침과 도구](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/04-instructions-and-tools/) | 매 요청마다 전송되는 사용자 지정 지침과 도구 목록을 어떻게 줄이나요? |
| 05 | [모델, 계획, 위임](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/05-models-and-planning/) | 작업 결과당 비용을 기준으로 모델을 고르고 계획·서브에이전트 위임을 어떻게 활용하나요? |
| 06 | [프롬프트와 출력 스타일](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/06-prompt-and-output-style/) | BPE 정렬 수식 프롬프트와 출력 제한으로 입출력 토큰을 어떻게 줄이나요? |
| 07 | [언어와 토큰화](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/07-language-and-tokenization/) | 같은 문장도 언어에 따라 토큰 수가 왜 달라지며 어떻게 측정하나요? |
| 08 | [에이전트 모드와 예산 가드](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/08-agent-modes-and-budget/) | Ask, Plan, Agent 모드의 차이와 루프 제한 설정 방법은 무엇인가요? |
| 09 | [MCP 감사](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/09-mcp-audit/) | 작업 영역별로 어떤 MCP 서버만 활성화해야 하나요? |
| 10 | [체크리스트](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/10-checklist/) | 세션 시작 전·중·후에 바로 점검할 수 있는 한 페이지 루틴은 무엇인가요? |
| VSC | [`cctx`: VS Code 토큰 절감 및 개발 가속](https://dotnetpower.github.io/agent-tokenomics/ko/copilot-tokenomics/cctx/) | VS Code Marketplace에서 `cctx`를 설치해 탐색 토큰을 줄이고 개발 속도(탐색 `15.20배`, 작업 완료 `-24.7%`)와 실시간 AIC/USD 절감량을 확인하는 방법은? |

### 트랙 B — LLM 에이전트 토큰 문제 해결 (`ko/agent-tokenomics/`)

| 단계 | 문서 | 산출물 |
|---|---|---|
| 00 | [개념: 에이전트는 토큰을 어떻게 쓰는가](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/00-concepts/) | stateless 재전송, 에이전트 루프, 프롬프트 캐싱, 컨텍스트 열화, 도구 비용의 기본 개념 |
| 01 | [현재 상태 진단](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/01-diagnose/) | 입력·출력·캐시 토큰, 첫 호출 고정 오버헤드, 턴별 증가 곡선의 기준선 측정 |
| 02 | [문제 분리](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/02-isolate/) | 변수를 한 번에 하나씩 바꾸어 미사용 도구, 실패 루프, 과대한 도구 출력 등 원인 특정 |
| 03 | [문제 증명](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/03-prove/) | 고정된 작업 집합에서 반복 실행으로 낭비 규모와 신뢰구간 정량화 |
| 04 | [개선](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/04-improve/) | 실험당 하나의 수정 적용(마이크로 스킬, 도구 가지치기, 루프 가드, 프롬프트 캐싱) |
| 05 | [결과](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/05-result/) | 기준선 대비 비용·성공률·지연 시간 재측정 및 유지/되돌리기 결정 |
| 06 | [골든 데이터셋과 루브릭 평가](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/06-golden-dataset-and-rubric/) | 단일 턴·멀티턴 품질 검증을 위한 엑셀(CSV) 골든 데이터셋과 키워드·의도 하이브리드 채점 |
| 도구 | [`tokprof`: 설치 및 사용법](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/tokprof/) | eBPF, 로컬 게이트웨이, 프로세스 내부 훅을 통한 무수정 토큰 프로파일링 |
| 사례 | [사례 연구: TokenEconomizer](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/case-tokeneconomizer/) | SWE-Bench 평균 비용을 18.4~30.9% 줄인 `microsoft/TokenEconomizer` 설계 분석 |

### 참고 자료 및 연구

- **[리소스](https://dotnetpower.github.io/agent-tokenomics/ko/resources/):** 토큰 절약에 도움이 되는 오픈소스 저장소, 도구, 공식 문서 목록입니다.
- **[논문 75선](https://dotnetpower.github.io/agent-tokenomics/ko/papers/):** 프롬프트 압축, 캐싱, 라우팅, 도구 가지치기 관련 논문 75편을 학회 등급과 인용 수 순으로 정리했습니다.
- **[용어집](https://dotnetpower.github.io/agent-tokenomics/ko/glossary/):** 문서 전반에서 사용하는 영어 ↔ 한국어 표준 기술 용어집입니다.

---

## 저장소 구조

| 경로 | 내용 |
|------|------|
| `site/` | Astro Starlight 문서 사이트(`https://dotnetpower.github.io/agent-tokenomics/`)입니다. 영어 문서는 `site/src/content/docs/`, 한국어 문서는 `site/src/content/docs/ko/`에 있습니다. 영문 README는 [README.md](README.md)를 참조하세요. |
| `profiler/` | [`tokprof`](https://pypi.org/project/tokprof/): 코드를 수정하지 않고 세션별·턴별·호출별 입력 토큰을 시스템 프롬프트, 대화 이력, 사용자 프롬프트, 도구 관련 항목으로 분해하는 네이티브 Rust 토큰 프로파일러입니다(`pip install tokprof` / `uvx tokprof`). |
| `examples/` | 재현 가능한 측정 스크립트 및 샘플 데이터: `token_report.py` + `sample-requests.jsonl`; `count_tokens.py`; `formula_prompt_en.py`, `formula_prompt_ko.py`, `formula_prompt.py` + `test_formula_prompt.py`; `prompt_budget.py` + `sample-pipeline.json`; `golden_dataset_tool.py` + `sample-golden-dataset.csv`/`.xlsx`; `papers.json` + `papers-ko.json` + `fetch_paper_meta.py` + `build_papers.py`. |
| `llms.txt` | 생성형 검색 엔진(GEO)과 코딩 에이전트가 프로젝트 구조를 한 번에 파악할 수 있도록 정리한 [llmstxt.org](https://llmstxt.org/) 표준 색인 파일입니다(`site/public/llms-full.txt`와 연동). |
| `.github/copilot-instructions.md` | 이 저장소의 문서 작성 및 한영 대칭 규칙입니다. |

---

## 자주 묻는 질문 (FAQ)

### LLM과 AI 에이전트에서 토큰 이코노믹스(Tokenomics · Token Economics)란 무엇인가요?
AI와 대규모 언어 모델(LLM)에서 **토큰 이코노믹스(Tokenomics · Token Economics, 토큰 경제학)**는 모델 호출과 멀티턴 에이전트 루프 전반에서 입력·캐시·출력·추론 토큰의 과금 구조를 분석하고, 답변 품질 저하 없이 컨텍스트 예산과 비용 대비 성능을 최적화하는 엔지니어링 분야입니다.

### `agent-tokenomics`의 제0원칙(Rule Zero)은 무엇인가요?
토큰 수만 줄어든 것은 성공이 아닙니다. 토큰 절감 조치를 적용할 때는 반드시 고정된 작업 집합의 성공률이나 [골든 데이터셋 루브릭 평가](https://dotnetpower.github.io/agent-tokenomics/ko/agent-tokenomics/06-golden-dataset-and-rubric/)를 함께 측정해 답변 정확도와 멀티턴 문맥 유지력이 떨어지지 않았는지 확인해야 합니다.

### `tokprof`는 에이전트 코드를 수정하지 않고 어떻게 LLM 호출을 수집하나요?
Linux에서는 `tokprof start`가 eBPF TLS uprobe를 통해 이미 실행 중인 프로세스와 Docker 컨테이너의 HTTPS 트래픽을 자동 수집합니다. Linux, macOS, Windows 공통으로는 로컬 게이트웨이(`OPENAI_BASE_URL`, `ANTHROPIC_BASE_URL`, `AZURE_OPENAI_ENDPOINT`)와 Python(`httpx`, `requests`, `urllib3`, `aiohttp`)·Node.js(`fetch`, `http`/`https`) 프로세스 내부 훅을 지원합니다.

---

## 개발 및 검증

```bash
npm --prefix site ci
npm --prefix site run dev          # http://localhost:4321
npm --prefix site run check:pairs -- --strict
npm --prefix site run build        # dist/ 빌드 및 llms.txt · llms-full.txt 생성
npm --prefix site run check:seo    # SEO 메타태그, JSON-LD, robots.txt, sitemap, llms.txt 검증
python3 examples/token_report.py examples/sample-requests.jsonl
```

사이트 로고는 **T**에 화폐 기호를 연상시키는 두 개의 사선을 더해 토큰과 비용을 표현합니다. 헤더 로고와 favicon은 [`site/public/favicon.svg`](site/public/favicon.svg)를 함께 사용하므로, 이 SVG를 수정하면 두 곳에 모두 반영됩니다.

## 라이선스

[LICENSE](LICENSE)를 참조하세요.
