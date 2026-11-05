---
title: "cctx: VS Code Copilot 토큰 절감 및 개발 가속"
description: VS Code Marketplace에서 Code Context(cctx)를 설치해 코드 탐색 토큰을 줄이고, 에이전트 개발 속도를 가속하며, 실시간 AIC/USD 절감량을 확인합니다.
track: copilot
verified: 2026-11-06
sidebar:
  order: 11
---

[Code Context (`cctx`)](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)는 공유 로컬 Rust 트라이그램 인덱스를 기반으로 동작하는 **VS Code 전용 네이티브 Language Model Tools 확장**(`dotnetpower.code-context-native`)입니다. VS Code Marketplace에서 바로 설치할 수 있으며, 여러 번 반복되던 `grep` + 전체 파일 `view` 루프를 **단일 블록 클램핑 조회 1회**로 대체해 **Copilot 토큰·AI 크레딧 비용을 아끼는 동시에 에이전트 개발 속도를 크게 가속**합니다. 또한 절감된 토큰과 비용(AIC/USD)을 VS Code 상태 표시줄과 터미널 대시보드에서 실시간으로 보여 줍니다.

<a id="problem"></a>
## 문제

GitHub Copilot 에이전트 모드가 기본 검색·파일 읽기 도구만으로 저장소를 탐색할 때는 **개발 속도를 늦추고 AI 크레딧을 소모하는 두 가지 비용**이 함께 발생합니다.

1. **다단계 탐색으로 인한 지연(개발 속도 저하):** 심볼 하나를 찾고 정의와 호출부를 확인하기 위해 `grep` → `view` → 다시 `grep`을 여러 턴 왕복하면, 추가되는 매 LLM 턴마다 네트워크 왕복과 프리필(Prefill) 대기 시간이 누적됩니다.
2. **대화 이력 비대화(토큰 비용 증가):** 함수 경계 없이 읽어 들인 넓은 범위의 코드(`100~200줄` 이상 또는 문서 전체)는 대화 이력(History)에 남아 **세션이 끝날 때까지 매 턴 다시 전송**됩니다. 여기에 서술형(Prose) 지침 파일까지 길면 매 턴 고정 입력 비용이 반복 청구됩니다.

---

<a id="action-install-in-vs-code-marketplace"></a>
## 조치: VS Code에서 설치 및 사용하기 (Marketplace)

`cctx`는 **Linux, 네이티브 Windows(`cctx.exe`), WSL, SSH** 환경의 VS Code(`1.140+`)에서 별도의 MCP JSON 설정이나 실험적(proposed) API 플래그 없이 네이티브 확장으로 동작합니다.

<a id="1-install-the-vs-code-extension-from-marketplace"></a>
### 1. VS Code Marketplace에서 확장 설치

VS Code 확장(Extensions) 탭에서 **[Code Context by dotnetpower](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)**를 검색해 설치하거나 다음 명령을 실행합니다.

```bash
code --install-extension dotnetpower.code-context-native
```

<a id="2-install-the-local-rust-backend-cctx"></a>
### 2. 로컬 Rust 백엔드(`cctx`) 설치

VS Code 확장은 로컬에서 동작하는 초고속 Rust 데몬(`cctx`)과 통신합니다.

```bash
# Linux / WSL / macOS / SSH:
git clone https://github.com/dotnetpower/code-context.git
cd code-context && bash scripts/install.sh && ~/.local/bin/cctx ping

# 네이티브 Windows (PowerShell, WSL 불필요):
cargo build --locked --release
copy target\release\cctx.exe $HOME\.local\bin\cctx.exe
```

<a id="3-open-your-project-and-run-getting-started"></a>
### 3. 프로젝트 열기 및 Getting Started 실행

VS Code 명령 팔레트(`Ctrl+Shift+P` / `⌘Shift+P`)에서 **Code Context: Getting Started**를 실행합니다. 네이티브 Windows에서는 첫 사용 시 `cctx.exe daemon`이 백그라운드에서 자동으로 시작됩니다. 확장이 설치되면 BPE 토큰 경계에 맞춰 압축된 수식형 에이전트 지침(`applyTo: "**"`)이 자동으로 연결되어, 사용자가 매번 지시하지 않아도 Copilot이 아래 네이티브 도구를 우선 사용합니다.

| VS Code 네이티브 도구 | Copilot 활용 목적 | 더 빠르고 저렴한 이유 |
|---|---|---|
| `#cctxNativeContext` | **심볼/패턴 검색 + 주변 코드 1회 조회** | 별도의 `grep` + `view` 왕복을 1회 호출로 통합하고, `<8 KiB` 인라인 예산 안에서 함수·블록 경계에 맞춰 필요한 코드만 반환합니다. |
| `#cctxNativeGrep` | **트라이그램 인덱스 기반 문자열/정규식 검색** | 노이즈가 많은 단순 일치 목록 대신 심볼 정의부(`max_matches_per_file`)를 우선 반환합니다. |
| `#cctxNativeFiles` | **초고속 파일명·경로 글롭 탐색** | 인메모리 인덱스에서 평균 `0.307 ms`(웜 캐시 중앙값) 만에 경로를 찾습니다. |
| `#cctxNativeRead` | **해시 검증 라인 범위 및 문서 앵커 읽기** | 필요한 줄 범위만 정확히 읽거나 `.md`, `.docx`, `.pptx`, `.pdf`의 `#toc` / `#slide-N-M` / `#heading` 앵커만 선택 조회합니다. |

---

<a id="why-it-both-saves-cost-and-accelerates-development"></a>
## 비용 절감과 개발 속도 가속이 동시에 일어나는 이유

`aks-store-demo`와 Copilot A/B 코딩 실험을 대상으로 수행된 [8세션·40라운드·8,960쌍 벤치마크 리포트](https://github.com/dotnetpower/code-context/blob/main/docs/BENCHMARKS.md)(`dotnetpower/code-context` 보고 수치)에 따르면, `cctx`는 답변 정확도(100%)를 그대로 유지하면서 **개발 완료 속도**와 **토큰 효율**을 동시에 끌어올립니다.

<a id="1-development-speed-acceleration"></a>
### 1. 개발 속도 가속 (Developer Velocity)

- **웜 캐시 기준 코드 탐색 속도 15.20배 가속** (혼합 탐색 스위트 평균 `4.450 ms → 0.293 ms`, 95% 신뢰구간 `14.53–15.84×`, 콜드 싱크 중앙값 `29.115 ms`):
  - 문자열 검색(Literal search): **20.35배 빠름** (`5.885 ms → 0.289 ms`)
  - 검색 + 주변 컨텍스트(Search + context): **18.70배 빠름** (`5.935 ms → 0.317 ms`)
  - 파일명 탐색(Filename discovery): **16.03배 빠름** (`4.923 ms → 0.307 ms`)
- **도구 호출 횟수 61.9% 감소** (`7.00회 → 2.67회`) 및 **LLM 왕복 턴 수 26.7% 감소** (`5.00턴 → 3.67턴`): 불필요한 탐색 왕복이 줄어들어 에이전트가 코드 수정과 검증 단계에 훨씬 빨리 도달합니다.
- **전체 코딩 작업 완료 시간(Wall time) 24.7% 단축** (`20.31초 → 15.30초`, 정답률 100% 동일 유지).
- **논블로킹 콜드 캐시(Nonblocking Root Cache):** 인덱스가 아직 준비되지 않은 저장소에서도 에이전트가 멈춰 기다리지 않도록 즉시 `CACHE_MISS`를 반환해 기본 도구로 작업을 이어가며 백그라운드에서 인덱싱을 완료합니다.

<a id="2-token-and-ai-credit-savings"></a>
### 2. 토큰 및 AI 크레딧 비용 절감

| 벤치마크 항목 | 기본 도구 (`rg` + `view`) | VS Code `cctx` 적용 시 | 보고된 절감 및 가속 효과 |
|---|---:|---:|---|
| **Rust 6개 심볼 연속 조회 토큰** | 15,028 tok (100줄) / 25,133 tok (200줄) | **4,031 tok** (20회 → 6회 호출) | **토큰 -73.2% ~ -84.0%** (**10.3배** 빠름) |
| **Python 6개 심볼 연속 조회 토큰** | 13,997 tok (100줄) / 24,609 tok (200줄) | **2,141 tok** (15회 → 6회 호출) | **토큰 -84.7% ~ -91.3%** (**13.16배** 빠름) |
| **문서 파일 (`.pptx` / `.docx` / `.md`)** | 문서 전체 읽기 | 가상 `#toc` + `#slide-N-M` 앵커 조회 | **토큰 -86.3% ~ -98.9%** |
| **자동 로드 에이전트 지침** | 서술형 2,048 tok | **BPE 정렬 수식 지침 756 tok** | **지침 고정 토큰 -63.1%** |
| **Copilot A/B 세션 과금 비용** | 페이로드 6,360 tok / 4.76B nanoAIU | **페이로드 3,177 tok / 3.55B nanoAIU** | **페이로드 -50.0% · AIU 과금 -25.4%** |

---

<a id="how-to-verify-live-usage--cost-savings-in-vs-code"></a>
<a id="how-to-verify-live-usage-cost-savings-in-vs-code"></a>
## 확인 방법: VS Code에서 실시간 Usage 및 절감 비용 보기

`cctx`가 실제로 얼마나 호출되었고 내 Copilot 비용을 얼마나 아껴 주었는지 VS Code 안에서 두 가지 화면으로 즉시 확인할 수 있습니다.

<a id="1-vs-code-status-bar-badge--hover-popover-instant-in-editor"></a>
### 1. VS Code 하단 상태 표시줄(Status Bar) 배지 및 마크다운 팝오버

VS Code 하단 상태 표시줄에 네이티브 도구 성공 횟수(`S`), 활성 모델, 추정 절감 토큰 및 절감 금액(`+$USD`)이 실시간으로 표시됩니다.

```text
$(database) CCTX N S:382 C:4 P:0 E:0 · Claude Sonnet 4.6 · ~142.8K tok (+$0.48)
```

상태 표시줄 항목 위에 마우스를 올리면 모델별 토큰 소비량, 캐시 적중률, 네이티브 도구별 절감 내역을 정리한 마크다운 툴팁이 열립니다(합성 예시):

| 💰 Cost Saved (@$0.001) | 💎 API Value (@$0.01) | 🔥 Tokens Saved | 🎯 Warm Hit Rate | ⚡ Avg Latency |
| :---: | :---: | :---: | :---: | :---: |
| **+$0.48** (`~480.0 AIC`) | **+$4.80** | **~142.8K tok** | **99.0%** (`382/386`) | **0.31 ms** |

| Operation | Calls (OK/Miss) | Delivered | Saved Tok | Avg ms |
| :--- | ---: | ---: | ---: | ---: |
| `cctxNativeContext` | 142 (140/2) | ~68.4K | ~94.2K | 0.32 ms |
| `cctxNativeRead` | 128 (127/1) | ~52.1K | ~31.5K | 0.26 ms |
| `cctxNativeGrep` | 74 (73/1) | ~16.8K | ~12.4K | 0.29 ms |
| `cctxNativeFiles` | 42 (42/0) | ~5.5K | ~4.7K | 0.30 ms |

<a id="2-multi-session-terminal-dashboard-cctx-usage---hours-24"></a>
### 2. 멀티 세션 통합 대시보드 (`cctx usage --hours 24`)

VS Code 통합 터미널에서 `cctx usage --hours 24`(또는 최근 7일은 `--hours 168`)를 실행하면, 최근 Copilot 세션 전체의 모델별 지출, 프롬프트 캐시 적중률, 소스 탐색 채택률, 그리고 누적 절감 금액(AIC 및 API 정가 환산)을 한눈에 볼 수 있습니다(합성 예시):

```text
$ cctx usage --hours 24
╔═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╗
║ ⚡ CODE CONTEXT (cctx) — MODEL, TOKEN & COST SAVINGS DASHBOARD (Window: 24.0h)                                            ║
╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣
║ 💰 COST SAVED (@$0.001/AIC) :  +$24.80    USD   (-23.5% of baseline bill · ~24800.0 AIC saved)                            ║
║ 💎 API LIST VALUE (@$0.01)  :  +$248.00   USD   (Frontier API list price: $2/$0.10/$10 per 1M)                            ║
║ 🔥 TOKENS SAVED             :  ~740.20M   tok   (Payload ~7.42M + 35x Cache ~259.7M + ~2040 Turns ~450.1M + BPE ~22.98M)  ║
║ 🧬 BPE FORMULA COMPRESSION  :  ██████████ 100.0%  (4/4 surfaces: ext·mcp·subagent·global · ~2100 tok/turn · +$12.30)      ║
║ 🎯 WARM CACHE HIT RATE      :  ██████████  97.2%  (4680 warm hits / 4815 CCTX lookups)                                    ║
║ 🚀 ELIGIBLE SOURCE ADOPTION :  ███████░░░  74.8%  (4815/6437 · excl. 920 /tmp & 680 pre-edit)                             ║
╚═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝

  1. LLM MODEL USAGE, CACHE HIT & COST SAVINGS
┌──────────────────────────┬────────────┬─────────────────┬───────────────┬───────────────┬───────────┬────────────────────────┬──────────────────────────────────────────┐
│ Model (Effort)           │ Sess/Turns │  Input (Cache%) │   Out+Rsn Tok │  CCTX (Warm%) │ Src Adopt │    LLM Spend (@$0.001) │        COST SAVED (@$0.001 / API @$0.01) │
├──────────────────────────┼────────────┼─────────────────┼───────────────┼───────────────┼───────────┼────────────────────────┼──────────────────────────────────────────┤
│ Claude Sonnet 4.6 (high) │    18/4120 │   1.84B (96.2%) │   4.12M+1.90M │  3410 (97.5%) │     76.4% │   $58.40 (58400.0 AIC) │   +$18.20 / API +$182.00 (~18200.0 AIC)  │
│ GPT-5.4 (medium)         │    24/1980 │ 640.00M (97.4%) │   2.10M+1.15M │  1405 (96.5%) │     71.2% │   $22.10 (22100.0 AIC) │     +$6.60 / API +$66.00 (~6600.0 AIC)   │
│ TOTAL (All Models)       │    42/6100 │   2.48B (96.5%) │   6.22M+3.05M │  4815 (97.2%) │     74.8% │   $80.50 (80500.0 AIC) │   +$24.80 / API +$248.00 (~24800.0 AIC)  │
└──────────────────────────┴────────────┴─────────────────┴───────────────┴───────────────┴───────────┴────────────────────────┴──────────────────────────────────────────┘
```

VS Code 기본 제공 [Agent Debug Logs](../02-measure-usage/)와 함께 사용하면 동일한 작업의 `cctx` 적용 전후 턴 수, 소요 시간, 소비 크레딧을 직접 비교할 수 있습니다.

---

<a id="trade-offs-and-cautions"></a>
## 트레이드오프와 주의 사항

- **아주 작은 단일 파일 구간 읽기:** `cctx`는 로컬 데몬과 IPC로 통신하므로, 이미 파일과 줄 번호를 알고 있는 아주 짧은 단일 구간 읽기는 직접 파일 시스템을 읽는 것보다 미세 지연 시간 자체는 조금 더 걸릴 수 있습니다(`0.251 ms` vs `0.083 ms`). 다만 `#cctxNativeRead`는 SHA-256 해시 검증과 `<8 KiB` 인라인 예산 보호를 함께 제공합니다.
- **첫 조회 시 콜드 스타트 폴백:** 새 워크스페이스에서 처음 조회할 때 나오는 `CACHE_MISS`는 검색 결과가 0건이라는 뜻이 **아닙니다**. 에이전트가 기다리지 않도록 해당 턴에만 기본 호스트 도구로 즉시 폴백하고, 백그라운드에서 인덱싱(중형 저장소 중앙값 `~29 ms`)을 마친 뒤 다음 턴부터 캐시를 사용합니다.
- **보안 및 제외 경로 경계:** `.github`, `.vscode` 같은 숨김 설정 폴더, `.gitignore` 대상 경로, `/tmp`, 시크릿 파일은 보안 정책에 따라 트라이그램 인덱스에서 제외되며 VS Code의 호스트 정책 도구를 통해 처리됩니다.

<a id="sources"></a>
## 출처

- [Code Context by dotnetpower — VS Code Marketplace (`dotnetpower.code-context-native`)](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)
- [dotnetpower/code-context GitHub 저장소](https://github.com/dotnetpower/code-context)
- [Code Context 벤치마크 및 토큰 효율 리포트](https://github.com/dotnetpower/code-context/blob/main/docs/BENCHMARKS.md)
- [VS Code 설치 및 문제 해결 가이드](https://github.com/dotnetpower/code-context/blob/main/vscode-extension/docs/SETUP.md)
