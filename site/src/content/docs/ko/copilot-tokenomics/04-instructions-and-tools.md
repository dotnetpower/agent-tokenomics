---
title: "지침과 도구"
description: "모든 요청에 전송되는 사용자 지정 지침과 도구 목록을 줄입니다."
track: copilot
verified: 2026-11-03
sidebar:
  order: 4
---

## 문제

사용자 지정 지침과 사용 설정한 도구의 정의(MCP 서버의 도구 포함)는 **요청마다** 비용을 발생시킵니다.
사용하지 않거나 불필요하게 큰 항목은 매번 드는 고정 비용과 같습니다.

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row" aria-label="범위 없는 지침과 도구 40개: 고정 비용 82%, 작업 18%">
      <span>범위 없는 지침 + 도구 40개</span>
      <div class="at-diag-bar"><div class="at-diag-seg seg-miss" style="width:82%">고정 82%</div><div class="at-diag-seg seg-user" style="width:18%">작업 18%</div></div>
      <span>100%</span>
    </div>
    <div class="at-diag-row" aria-label="applyTo로 범위를 정하고 도구를 줄인 구성: 고정 비용 22%, 작업 18%">
      <span>applyTo 범위 지정 + 도구 가지치기</span>
      <div class="at-diag-bar"><div class="at-diag-seg seg-hit" style="width:22%">고정 22%</div><div class="at-diag-seg seg-user" style="width:18%">작업 18%</div></div>
      <span>40%</span>
    </div>
  </div>
  <figcaption>측정 결과가 아닌 가상의 예시입니다. 변경 전 입력을 기준으로 고정 비용은 82%에서 22%로 줄고 작업은 18%로 유지되어, 변경 후 전체 입력은 기존의 40%가 됩니다. 작업 내용은 유지하면서 지침과 도구의 고정 비용을 줄이는 모습을 보여 주며, 실제 절감량은 다릅니다. 색상은 구성의 차이를 나타낼 뿐 캐시 적중 여부를 뜻하지 않습니다.</figcaption>
</figure>

## 조치

**사용자 지정 지침**(`.github/copilot-instructions.md`, `*.instructions.md`, `AGENTS.md`)

- *안정적이고 가치가 높은* 사실만 남깁니다. 기본값과 다른 빌드/테스트 명령, 필수 환경 변수,
  생성된 파일의 위치, 에이전트가 추론할 수 없는 규칙이 해당합니다.
- `*.instructions.md`의 `applyTo`로 범위를 지정해, 언어나 폴더에 한정된 규칙이 해당 위치에서만 로드되게 합니다.
  지침은 세 단계로 나누어 생각합니다. **상시 적용**(항상 적용되는 소수의 규칙), **조건부 적용**(`applyTo` 경로 규칙),
  **필요 시 적용**(PR 검토처럼 가끔 쓰는 워크플로를 위한 스킬이나 메모)입니다.
- 에이전트가 코드에서 쉽게 알아낼 수 있는 내용은 삭제합니다. 긴 스타일 가이드를 붙여 넣지 않습니다.
- 지침에 사실을 한 번 적어 두면 에이전트가 세션마다 다시 찾을 필요가 없습니다.
- `AGENTS.md`와 `copilot-instructions.md`에 중복된 내용은 합칩니다. 중복된 텍스트는 두 번 과금됩니다.
- 규칙은 간결하게 씁니다(군더더기 없는 선언형 규칙). 이 파일은 모든 요청에서 다시 전송됩니다.
- 도구 선택 우선순위나 폴백 조건을 정의할 때는 **BPE 친화적 수식 표기법**(`조건 => 도구`, `상황 -> 조치`, `>` 우선순위 체인; [프롬프트와 출력 스타일](../06-prompt-and-output-style/#4-bpe-aligned-formula-notation) 참조)을 고려합니다. VS Code용 참고 사례로, [dotnetpower/code-context](https://github.com/dotnetpower/code-context)의 자동 로드 지침(`*.instructions.md`)을 106줄 줄글(9,127바이트, `o200k_base` 2,048토큰)에서 26줄 BPE 정렬 수식 명세(2,688바이트, 718토큰)로 변환했을 때 12개 엣지케이스 시나리오의 규칙 인식률 **100% (31/31)**를 유지하면서 매 턴 전송되는 지침 토큰을 **64.9%** 줄였습니다.

**자동 생성한 지침을 그대로 방치하지 마세요.** 리포지토리 컨텍스트 파일을 다룬 ETH Zurich 연구(arXiv 2602.11988)에 따르면,
컨텍스트 파일은 대체로 작업 성공률을 높이지 못하면서 추론 비용을 평균 20% 넘게 늘렸습니다. 리포지토리 개요는 도움이 되지 않았고,
**표준에서 벗어난 관행**에 대한 지침은 에이전트가 잘 따랐습니다. 이 파일을 버그 트래커처럼 다룹니다.
에이전트가 실제 "지뢰"를 밟았을 때 한 줄을 추가하고, 코드에서 알아낼 수 있는 줄은 삭제하며, 코드가 바뀌면 다시 평가합니다.

| 유지(지뢰 규칙) | 삭제(스스로 파악 가능) |
|------------------|----------------------------|
| "`pip`가 아니라 `uv`를 사용한다" | "이 프로젝트는 Python 프로젝트다" |
| "테스트는 `--no-cache`로 실행한다" | "테스트는 `tests/`에 있다" |
| "`auth/`는 리팩터링하지 않는다 — 감사 진행 중" | "PostgreSQL을 사용한다" |
| "배포하려면 VPN이 필요하다" | "코드 품질이 중요하다"(내용 없는 조언) |

**도구 목록**

- 채팅 입력란의 **Configure Tools**에서 현재 작업에 필요한 도구나 MCP 서버만 사용 설정합니다.
- 사용자 지정 에이전트에서는 `tools` 속성을 작업에 맞는 좁은 목록으로 지정합니다.
- 단일 목적의 MCP 도구 수십 개를 로드하기보다, 터미널 도구로 호출하는 집중된 CLI 하나를 우선 고려합니다.
  조직에서 이를 유지 관리하고 보안을 확보할 수 있는 경우에 해당합니다.
- 세션을 시작하기 **전에** 도구를 선택합니다. 세션 도중에 MCP 서버를 사용 설정하면 도구 정의가 바뀌어
  프롬프트 캐시 재사용이 깨질 수 있습니다(Cache Explorer에서 처음 달라진 지점으로 확인됩니다).

**결정적 반복 작업 묶기**

- 대량 작업이나 폴링 워크플로는 검토를 마친 스크립트를 터미널에서 실행하고, 모델에는 요약만 전달합니다.
  여러 번 도구를 호출해 출력이 컨텍스트에 쌓이는 일을 피합니다.

## 확인 방법

Agent Debug Logs에서 줄이기 전후의 첫 요청 입력 토큰을 비교합니다. 그런 다음 기준선 작업을 실행해 세션 크레딧을 비교합니다.
품질도 다시 확인합니다. 에이전트가 지침에서 제거한 내용을 다시 찾고 있지는 않은지 살펴봅니다.

## 트레이드오프

- 지침을 지나치게 줄이면 같은 내용을 반복해서 탐색하게 되고, 도구를 지나치게 줄이면 작업이 실패하거나 수동으로 우회해야 합니다.
- 스크립트 방식과 대화형 방식을 비교해 측정합니다. 절감 효과는 도구와 결과에 따라 달라집니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 논문의 결과는 각자의 워크로드에서 검증해야 하는 가설입니다. 전체 목록은 [논문](../../papers/)을 참조합니다.

- [Batch Prompting: Efficient Inference with Large Language Model APIs](https://arxiv.org/abs/2301.08721) — EMNLP 2023, 인용 154회. 여러 샘플을 하나의 프롬프트에 담아 지침 토큰을 공유합니다.
- [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) — 프리프린트 2025, 인용 55회. 모든 도구 스키마를 주입하는 대신 에이전트가 필요한 도구를 요청하게 합니다.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) — 프리프린트 2025, 인용 41회. 모든 도구를 로드하는 대신 관련 있는 MCP 도구 설명만 검색해 가져옵니다(프롬프트 비대화 완화).
- [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents?](https://arxiv.org/abs/2602.11988) — 프리프린트 2026, 인용 36회. 리포지토리 컨텍스트 파일은 대체로 작업 성공률을 높이지 못했고 추론 비용을 평균 20% 넘게 늘렸습니다.

## 출처

- [Optimize AI credit usage — Keep the tool catalog focused, Batch repetitive operations, Ground the agent](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [Diagnose prompt caching with the Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents? (arXiv:2602.11988)](https://arxiv.org/abs/2602.11988)
- 도구 목록을 더 깊이 다루는 절차는 [MCP 점검](../09-mcp-audit/)을 참조합니다.
