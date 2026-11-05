---
title: "Copilot 토큰 이코노믹스 (Tokenomics · Token Economics)"
description: "VS Code GitHub Copilot을 위한 실전 토큰 이코노믹스(Token Economics, 토큰 경제학): 토큰과 AI 크레딧 절약, 컨텍스트·MCP 최적화, cctx 개발 가속."
track: copilot
verified: 2026-11-03
---

Track A는 VS Code에서 GitHub Copilot을 **사용하는** 개발자를 위한 **Copilot 토큰 이코노믹스(Tokenomics · Token Economics)** 가이드입니다. 각 페이지는 *문제 → 조치 → 확인 방법 → 트레이드오프* 순서로 설명합니다.

먼저 Copilot 청구 설정에서 사용 중인 플랜을 확인하세요. 과금 방식에 따라 우선할 절약 방법이 다릅니다.

| 과금 방식 | 과금 단위 | 먼저 볼 페이지 |
|---|---|---|
| AI 크레딧(사용량 기반) | 입력, 캐시된 입력, 출력 토큰 각각에 모델 요율 적용 | [사용량 측정](02-measure-usage/), [컨텍스트](03-context-hygiene/), [도구](04-instructions-and-tools/) |
| 기존 프리미엄 요청 | 프롬프트/턴당 모델 배수 적용 | [과금](01-how-usage-is-charged/), [모델](05-models-and-planning/), [예산 가드](08-agent-modes-and-budget/) |

<figure class="at-doc-figure">
  <img src="https://code.visualstudio.com/assets/docs/agents/concepts/context-assembly.png" alt="여러 소스에서 에이전트 컨텍스트를 구성하는 과정을 보여 주는 VS Code 다이어그램" loading="lazy" />
  <figcaption>에이전트 컨텍스트는 여러 소스에서 구성됩니다. 출처: <a href="https://code.visualstudio.com/docs/agents/concepts/context">VS Code Docs — Context in Agents</a>.</figcaption>
</figure>

| 페이지 | 살펴볼 질문 |
|------|---------------------|
| [Copilot 사용량은 어떻게 과금되는가](01-how-usage-is-charged/) | 무엇에 비용이 드는가? |
| [사용량 측정](02-measure-usage/) | 어떤 채팅과 요청의 비용이 큰가? |
| [컨텍스트를 가볍게 유지하기](03-context-hygiene/) | 불필요한 대화 기록에 드는 비용을 어떻게 막을 수 있는가? |
| [지침과 도구](04-instructions-and-tools/) | 요청마다 전송되는 내용을 어떻게 줄일 수 있는가? |
| [모델, 계획, 위임](05-models-and-planning/) | 저렴한 모델이나 계획 우선 방식은 언제 효과가 있는가? |
| [프롬프트와 출력 스타일](06-prompt-and-output-style/) | 프롬프트를 어떻게 줄이고 답변 길이를 제한하는가? |
| [언어와 토큰화](07-language-and-tokenization/) | 한국어 등 다른 언어로 작성하면 비용이 더 드는가? |
| [에이전트 모드와 예산 가드](08-agent-modes-and-budget/) | Ask, Plan, Agent는 어떻게 다르며 반복 실행을 어떻게 제한하는가? |
| [MCP 점검](09-mcp-audit/) | 어떤 도구 서버를 계속 사용 설정해 둘 것인가? |
| [체크리스트](10-checklist/) | 한 페이지로 정리한 점검 루틴입니다. |
| [cctx: VS Code 토큰 절감 및 개발 가속](cctx/) | VS Code Marketplace 확장으로 탐색 토큰을 줄이고 개발 속도와 실시간 절감액을 확인하려면? |
| [리소스](../resources/) | 알아 두면 좋은 도구와 리포지토리입니다. |
| [논문](../papers/) | 이 전략의 바탕이 되는 연구를 신뢰도 순으로 정리했습니다. |

:::note
Copilot의 과금 방식, UI, 설정은 빠르게 바뀝니다. 각 페이지에는 확인한 날짜와 공식 출처 링크를 함께 적었습니다.
사용 중인 VS Code와 내용이 다르면 공식 문서를 기준으로 확인하세요.
:::
