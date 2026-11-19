---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "모델, 계획, 위임"
description: "측정 결과를 바탕으로 모델을 고르고, 코딩 전에 계획하며, 범위가 정해진 작업을 위임합니다."
track: copilot
verified: 2026-11-19
sidebar:
  order: 5
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

토큰 단가가 아니라 **성공한 결과당 비용**을 최적화합니다. 계획·위임은 어려운 작업을 도울 수 있지만 둘 다 모델 사용량을 추가합니다.

## 조치

| 선택 | 적합한 상황 | 확인 |
|---|---|---|
| Auto | 관리 부담을 줄이는 라우팅이 필요한 경우 | 선택된 모델과 실제 크레딧을 기록하며 항상 최저 비용이라고 가정하지 않습니다. |
| 명시적 모델 | 반복 가능한 흐름·필수 기능이 있는 경우 | 사용 가능한 모델·버전과 비교 가능한 강도를 고정합니다. |
| 계획 우선 | 요구 사항·접근 방식이 불확실한 경우 | 계획에도 조사·도구 호출이 있으므로 한 번의 모델 호출로 단정하지 않습니다. |
| 서브에이전트 | 격리할 가치가 있는 독립적인 큰 작업 | 부모·자식·검토·재시도를 한 번씩 집계합니다. |
| 직접 실행 | 작고 명확한 수정 | 절차를 따르기 위해 불필요한 계획·위임 턴을 추가하지 않습니다. |

[사용 가능한 모델·강도 제어](https://code.visualstudio.com/docs/agent-customization/language-models)와 [현재 계획 작업 흐름](https://code.visualstudio.com/docs/agents/run/planning)를 확인합니다. [서브에이전트 모델 선택은 하네스별로 다르며](https://code.visualstudio.com/docs/agents/run/subagents) 저렴한 작업 모델이 항상 제공되거나 적합한 것은 아닙니다.

### 실패·재시도의 역설

저렴한 모델도 여러 번 재시도하면 더 비쌀 수 있습니다. 유형별 실제 차감과 합격 결과를 비교합니다. 평균 단가에 토큰 수를 곱하는 추정만으로는 캐시 입력·출력의 단가 차이를 반영하지 못합니다.

## 확인 방법

Auto, 명시적 모델 하나, 필요한 경우 검토한 계획과 제한된 작업 모델의 세 구성을 대표 작업에 반복 적용합니다. 작업·리비전·품질 기준을 고정하고 [기준선 표](../02-measure-usage/)에 실제 모델, 강도, 전체 크레딧, 시간, 재시도, 통과 검사를 기록합니다. 실패한 실행을 임의로 제외하지 않습니다.

## 절충점

높은 추론 강도는 어려운 작업에 도움을 줄 수 있지만 토큰도 늘립니다. 기본값은 출발점이지 보편적인 최적값이 아닙니다. 설정 변경 시 캐시 상태를 기록합니다. 위임으로 시간이 줄어도 총비용은 늘 수 있으므로 둘 다 보고합니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 논문의 결과는 각자의 워크로드에서 검증해야 하는 가설입니다. 전체 목록은 [논문](../../papers/)을 참조합니다.

- [FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance](https://arxiv.org/abs/2305.05176) - TMLR 2023, 인용 1,247회. LLM을 선택하고 연쇄 호출해 더 낮은 비용으로 강력한 모델에 가까운 품질을 얻습니다.
- [Hybrid LLM: Cost-Efficient and Quality-Aware Query Routing](https://arxiv.org/abs/2404.14618) - ICLR 2024, 인용 519회. 라우터가 질의의 난이도를 예측해 쉬운 질의를 더 작은 모델로 보냅니다.
- [AutoMix: Automatically Mixing Language Models](https://arxiv.org/abs/2310.12963) - NeurIPS 2023, 인용 169회. 작은 모델이 자신의 답을 스스로 검증하게 하고, 필요할 때만 더 큰 모델로 넘깁니다.
- [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) - ICLR 2025, 인용 56회. 정답을 얻는 데 드는 기대 비용을 정의하고, 이를 기준으로 모델을 비교합니다.
- [RouteLLM: Learning to Route LLMs with Preference Data](https://arxiv.org/abs/2406.18665) - 프리프린트 2024, 인용 922회. 선호도 데이터로 학습해 질의를 강력한 모델과 더 저렴한 모델 사이에서 라우팅합니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: models](https://code.visualstudio.com/docs/agent-customization/language-models)
- [code.visualstudio.com: planning](https://code.visualstudio.com/docs/agents/run/planning)
- [code.visualstudio.com: subagents](https://code.visualstudio.com/docs/agents/run/subagents)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [docs.github.com: rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)
