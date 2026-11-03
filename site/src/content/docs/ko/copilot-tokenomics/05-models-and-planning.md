---
title: "모델, 계획, 위임"
description: "근거를 바탕으로 모델을 고르고, 코딩 전에 계획하며, 범위가 정해진 작업을 위임합니다."
track: copilot
verified: 2026-11-03
sidebar:
  order: 5
---

## 문제

토큰당 가장 저렴한 모델이 *결과*당 가장 저렴한 것은 아닙니다. 또한 잘못된 접근 방식으로 곧바로 코드를 작성하면
세션 전체가 낭비됩니다.

## 조치

1. **근거를 바탕으로 모델을 선택합니다.** 기대하는 결과와 평가 방법을 먼저 정의합니다. 출력은 비결정적이므로
   후보 모델마다 두 번 이상 실행합니다. 완료 여부, 품질, 안정성, 소요 시간, 크레딧을 비교하고,
   기준을 꾸준히 충족하는 가장 저렴한 모델을 고릅니다. 모델이 바뀌면 다시 평가합니다.
2. **관리 부담이 적은 기본값:** **자동 모델 선택**을 사용합니다. 반복되는 워크플로에서는
   **사용자 지정 에이전트**에 모델을 고정합니다.
3. **사고 강도는 기본값으로 둡니다.** 강도를 높이면 사고 토큰이 늘어나 비용과 지연 시간이 커집니다.
   아키텍처 계획이나 여러 단계의 디버깅처럼 정말 복잡한 작업에서만 높입니다.
4. **계획한 다음 구현합니다.** 성능이 높은 모델로 Plan 에이전트를 실행하고 계획을 검토한 뒤,
   범위가 명확한 작업으로 나눕니다. 이 작업은 더 저렴한 모델, 집중된 지침, 최소한의 도구를 갖춘
   **사용자 지정 에이전트를 서브에이전트로** 사용해 위임합니다. 기본 에이전트가 결과를 검토합니다.

## 확인 방법

반복되는 작업 유형에서 (a) 성능이 높은 모델 하나로 처음부터 끝까지 수행한 경우와
(b) 성능이 높은 모델로 계획하고 더 저렴한 모델로 작업자를 구성한 경우를 비교합니다.
크레딧, 재시도 횟수, 결과가 인수 검사를 통과하는지를 기록합니다.

## 트레이드오프

- 위임 자체도 토큰을 사용합니다. 격리와 저렴한 모델의 이점이 오케스트레이션 오버헤드를 상쇄하지 못하는
  작은 작업은 메인 세션에서 처리합니다.
- 재시도가 필요하거나 재작업을 만드는 저렴한 모델은 전체 비용이 더 클 수 있습니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 논문의 결과는 각자의 워크로드에서 검증해야 하는 가설입니다. 전체 목록은 [논문](../../papers/)을 참조합니다.

- [FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance](https://arxiv.org/abs/2305.05176) — TMLR 2023, 인용 1,247회. LLM을 선택하고 연쇄 호출해 더 낮은 비용으로 강력한 모델에 가까운 품질을 얻습니다.
- [Hybrid LLM: Cost-Efficient and Quality-Aware Query Routing](https://arxiv.org/abs/2404.14618) — ICLR 2024, 인용 519회. 라우터가 질의의 난이도를 예측해 쉬운 질의를 더 작은 모델로 보냅니다.
- [AutoMix: Automatically Mixing Language Models](https://arxiv.org/abs/2310.12963) — NeurIPS 2023, 인용 169회. 작은 모델이 자신의 답을 스스로 검증하게 하고, 필요할 때만 더 큰 모델로 넘깁니다.
- [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) — ICLR 2025, 인용 56회. 정답을 얻는 데 드는 기대 비용을 정의하고, 이를 기준으로 모델을 비교합니다.
- [RouteLLM: Learning to Route LLMs with Preference Data](https://arxiv.org/abs/2406.18665) — 프리프린트 2024, 인용 922회. 선호도 데이터로 학습해 질의를 강력한 모델과 더 저렴한 모델 사이에서 라우팅합니다.

## 출처

- [Optimize AI credit usage — Choose models, Plan and delegate, Thinking effort](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [AI model comparison (GitHub Docs)](https://docs.github.com/en/copilot/reference/ai-models/model-comparison)
