---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: 에이전트 모드와 예산 가드
description: Ask, Plan, Agent 모드의 차이, 에이전트 루프 제한, 추론 강도, 클라우드 에이전트 설정을 설명합니다.
track: copilot
verified: 2026-11-19
sidebar:
  order: 8
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

모드 이름은 고정 모델 호출 횟수나 달러 예산이 아닙니다. 읽기 전용 조사와 계획도 검색·도구 호출·여러 모델 요청을 포함할 수 있습니다.

## 조치

### 작업에 맞는 모드를 고릅니다

| 작업 흐름 | 목적 | 피해야 할 가정 |
|---|---|---|
| Ask·읽기 전용 조사 | 코드 수정 없이 이해 | 정확히 한 번 호출·조회 없음·무료 |
| Plan | 구현 전 조사하고 접근 방식 검토 | 한 번의 계획 호출이나 무료 작업 |
| Agent 구현 | 한정된 수정과 검증 실행 | 한도에 닿을 때까지 범위를 넓혀도 된다는 허가 |

접근 방식이 불확실하면 [계획 작업 흐름](https://code.visualstudio.com/docs/agents/run/planning)을 사용합니다. 작고 명확한 수정은 바로 구현할 수 있습니다.

### 루프를 제한합니다

합격 검사와 재시도 가드를 명시합니다.

```text
Run focused tests. If the same command fails twice with the same error,
stop and report the blocker, attempts, and unresolved checks.
Do not silently skip failed verification.
```

이는 **프롬프트 지시**이지 강제 지출 한도가 아닙니다. `chat.agent.maxRequests`의 제공 여부·의미·기본값은 설치 버전과 하네스에 맞는 [현재 설정 참조](https://code.visualstudio.com/docs/copilot/reference/copilot-settings)에서 확인합니다. 이 문서는 보편적인 기본값을 단정하지 않습니다. Copilot 하네스·Agent Host를 Local과 동일하게 취급하지 않습니다.

### 한도의 적용 범위

| 제어 | 제한 대상 | 한계 |
|---|---|---|
| 재시도 지시 | 의도한 에이전트 행동 | 로그로 준수를 확인해야 합니다. |
| 지원되는 요청 제한 | 문서화된 범위의 요청 수 | 달러·토큰 한도가 아니며 자식·다른 세션 적용 범위를 확인합니다. |
| GitHub 사용자 예산·지출 한도 | 계정·조직 소비량 또는 지출 | 사용자 예산은 사용을 중단하지만 지출 한도는 강제 옵션이 필요합니다. |

[조직 예산 규칙](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)이나 [개인 추가 사용 규칙](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)을 확인합니다. 크레딧 소비 기능의 중단이 모든 코드 완성 중단을 뜻하지는 않습니다.

### Copilot cloud agent (이슈 → PR)

파일, 재현 절차, 범위, 합격 검사를 제공합니다. [Copilot setup steps](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/customize-the-agent-environment)로 지원되는 의존성을 준비합니다. 설정·검토 작업과 별도 인프라 요금도 구분하며 모든 자동화를 무료로 취급하지 않습니다.

## 확인 방법

대표적인 제한된 수정 하나로 요청, 재시도, 자식 작업, 크레딧, 테스트, 중단 이유를 기록합니다. 작업 완료·지시 준수·요청 제한·예산 중단·오류를 구분합니다.

## 절충점

제한이 너무 작으면 유효한 작업도 중간에 남습니다. 계획·강한 모델이 초기에 더 비싸도 재시도를 줄일 수 있으므로 모드 이름 대신 성공한 결과를 비교합니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 각 연구 결과는 가설이므로 자신의 워크로드에서 검증해야 합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [Training Language Models to Reason Efficiently](https://arxiv.org/abs/2502.04463) - NeurIPS 2025, 인용 285회. 정확도와 추론 길이 사이의 균형을 조절하도록 추론 모델을 학습시킵니다.
- [Token-Budget-Aware LLM Reasoning](https://arxiv.org/abs/2412.18547) - ACL 2024, 인용 259회. 문제마다 토큰 예산을 추정해 chain-of-thought 추론을 짧게 만듭니다.
- [Do NOT Think That Much for 2+3=? On the Overthinking of o1-Like LLMs](https://arxiv.org/abs/2412.21187) - 프리프린트 2024, 인용 605회. o1 계열 모델이 단순한 질문에도 지나치게 많은 토큰을 쓴다는 점을 보이고, 이를 줄이는 방법을 제안합니다.
- [Agentless: Demystifying LLM-based Software Engineering Agents](https://arxiv.org/abs/2407.01489) - 프리프린트 2024, 인용 566회. 위치 파악, 수정, 검증으로 이루어진 고정 파이프라인이 낮은 비용으로 SWE-bench Lite에서 개방형 에이전트 루프와 경쟁할 수 있음을 보여 줍니다.
- [L1: Controlling How Long A Reasoning Model Thinks With Reinforcement Learning](https://arxiv.org/abs/2503.04697) - 프리프린트 2025, 인용 416회. 사용자가 지정한 길이 예산을 따르도록 추론 모델을 학습시킵니다.
- [Efficient Agents: Building Effective Agents While Reducing Cost](https://arxiv.org/abs/2508.02694) - 프리프린트 2025, 인용 40회. 에이전트 시스템을 설계할 때 효율과 효과 사이의 트레이드오프를 체계적으로 연구합니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: planning](https://code.visualstudio.com/docs/agents/run/planning)
- [code.visualstudio.com: settings](https://code.visualstudio.com/docs/copilot/reference/copilot-settings)
- [docs.github.com: org](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
- [docs.github.com: setup](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/cloud-agent/customize-the-agent-environment)
