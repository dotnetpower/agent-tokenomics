---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "Copilot 사용량은 어떻게 과금되는가"
description: "VS Code GitHub Copilot 토큰 이코노믹스 과금 가이드: AI 크레딧 기반 사용량 과금(입력·캐시·출력 토큰 요율)과 프리미엄 요청 배수를 설명합니다."
track: copilot
verified: 2026-11-19
sidebar:
  order: 1
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

총토큰 수, GitHub AI Credits, 기존 프리미엄 요청은 서로 다른 계기판입니다. 절약 방법을 고르기 전에 본인에게 적용되는 과금 방식을 확인해야 합니다.

## 조치

| 과금 경로 | 측정 대상 | 구분할 경계 |
|---|---|---|
| 개인 사용량 기반 요금제 | 모델별 입력·캐시 읽기·해당되는 캐시 쓰기·출력 요금과 계정 크레딧 | 기본 크레딧과 가변 추가 제공량을 고정 토큰 한도로 해석하지 않습니다. |
| Business / Enterprise | 소비량·공유 제공량·사용자 예산·추가 사용 정책 | 사용자 예산은 사용을 중단하지만 지출 한도는 사용 중단 옵션을 켜야 강제됩니다. |
| 기존 연간 Pro/Pro+ | 사용자 프롬프트와 모델·기능별 배수 | 자율 도구 호출마다 프리미엄 요청이 추가되는 것은 아닙니다. |
| SDK BYOK | 제공자 사용량과 청구 | GitHub가 과금하는 모델 사용량과 별도입니다. |

[GitHub의 공식 환산은 1 AI 크레딧 = $0.01 USD입니다](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing). 제공자의 할인율을 가정하지 말고 [GitHub 모델·컨텍스트 구간별 요금표](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)를 사용합니다. [기존 요청 과금의 대상·기능 규칙](https://docs.github.com/en/copilot/reference/copilot-billing/request-based-billing-legacy/copilot-requests)과 [SDK BYOK](https://docs.github.com/en/copilot/how-tos/copilot-sdk/auth/byok)도 따로 확인합니다.

### 캐시 입력과 과금

대화의 논리적 입력이 반복되어도 네트워크 바이트까지 매번 동일하게 재전송되는 것은 아닙니다. 재사용 가능한 접두부는 캐시 읽기 단가로 처리될 수 있습니다. 이미 캐시를 포함한 입력 합계에 캐시 읽기를 다시 더하거나, 추론을 포함한 청구 출력에 추론을 더하지 않습니다.

캐시는 모델·하네스·요청 구성·보존 시간에 따라 다릅니다. 모델·강도 변경이 영향을 줄 수 있지만 **모든 추론 강도 변경이 전체 캐시 미적중을 보장하지는 않습니다.** [보고된 재사용과 요청 차이](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)를 확인합니다. 요청 차이는 제공자 과금의 증거가 아니며 과거 배포 보고서를 영구 기본값으로 해석해서도 안 됩니다.

## 확인 방법

요금제, 클라이언트·버전, 하네스, 인증, 모델, 작업 성공 여부, 계정 수치를 기록합니다. 요청·세션 크레딧을 계정 계기판과 대조하고 비공개 항목은 0 대신 **확인 불가**로 표시합니다. [제품별 상세 비교](../coding-agent-quotas-and-caching/)도 참고합니다.

## 절충점

입력을 줄여 재시도가 늘면 절약이 아닙니다. 실패·위임 작업을 포함한 성공 작업당 비용을 비교합니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
- [docs.github.com: org](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)
- [docs.github.com: rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)
- [docs.github.com: legacy](https://docs.github.com/en/copilot/reference/copilot-billing/request-based-billing-legacy/copilot-requests)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
