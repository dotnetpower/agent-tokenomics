---
title: Agent Tokenomics
description: LLM 에이전트의 토큰 비용을 줄이기 위한 문제 해결 절차입니다.
track: agent
verified: 2026-11-03
---

Track B는 LLM 에이전트를 **직접 만들거나 운영하는** 엔지니어를 위한 가이드입니다. 토큰 비용이 높은 상황을
운영 장애처럼 다룹니다. 추측에 의존하지 말고 절차에 따라 원인을 확인하세요.

처음이라면 에이전트가 토큰을 쓰는 방식을 설명하는 [0. 개념](00-concepts/)부터 읽은 다음 절차를 따라가세요.

```text
Diagnose → Isolate → Prove → Improve → Result
   └────────── repeat with the next biggest cause ──────────┘
```

| 단계 | 질문 | 산출물 |
|------|------|--------|
| [0. 개념](00-concepts/) | 에이전트는 토큰을 어떻게 쓰는가? | 공통 개념 |
| [1. 진단](01-diagnose/) | 지금 토큰이 어디에 쓰이는가? | 측정된 기준선 |
| [2. 원인 분리](02-isolate/) | 어떤 원인 때문에 비용이 드는가? | 우선순위를 매긴 가설 하나 |
| [3. 입증](03-prove/) | 그 원인이 실제로 존재하며 규모는 얼마인가? | 재현하고 수치화한 낭비 |
| [4. 개선](04-improve/) | 원인에 맞는 어떤 변경으로 낭비를 줄일 수 있는가? | 변경 하나, 실험 하나 |
| [5. 결과](05-result/) | 품질 저하 없이 비용이 줄었는가? | 유지 / 되돌리기 결정 |

이 외에 [tokprof: 설치 및 사용법](tokprof/), [TokenEconomizer 사례 연구](case-tokeneconomizer/), 개선 방법과 관련된 [논문 순위 목록](../papers/),
실행 가능한 [측정 스크립트](https://github.com/dotnetpower/agent-tokenomics/tree/main/examples)도 제공합니다.

<figure class="at-doc-figure">
  <img src="https://code.visualstudio.com/assets/docs/agents/concepts/agent-loop.png" alt="에이전트가 도구와 그 결과를 반복해서 활용해 작업을 진행하는 과정" loading="lazy" />
  <figcaption>에이전트 루프에서는 지침, 도구, 대화 기록, 도구 결과가 다음 모델 호출에 영향을 줍니다. 0–5단계에서 이 비용을 진단하고 최적화합니다. 출처: <a href="https://code.visualstudio.com/docs/agents/concepts/agents">VS Code Docs — Agents Overview</a>, <a href="https://github.com/microsoft/TokenEconomizer">microsoft/TokenEconomizer</a>.</figcaption>
</figure>

## 준비물

- **요청 로그**: 모델 호출마다 한 건씩 기록한 로그입니다. 입력 / 캐시된 입력 / 출력 토큰, 전송한 도구 정의,
  도구 호출과 오류, 해당 호출이 속한 사용자 프롬프트(턴)와 세션을 포함합니다. 대부분의 에이전트 프레임워크가
  이 로그를 내보낼 수 있습니다. 그렇지 않다면 모델 클라이언트를 감싸서 직접 기록하세요.
- 다시 실행할 수 있는 **작업 세트**(대표 작업 5–20개)와 자동 성공 판정 기준
- 한 번에 변수 하나만 바꾸는 원칙

<a id="제0-원칙"></a>
## 기본 원칙

토큰을 줄이는 것은 **작업 성공률이 떨어지지 않을 때만** 성과입니다. 항상 비용과 품질 지표를 함께 보고하세요.
