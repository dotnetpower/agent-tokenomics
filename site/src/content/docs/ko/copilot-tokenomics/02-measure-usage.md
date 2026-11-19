---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "사용량 측정"
description: "VS Code Agent Debug Logs, Cache Explorer, 컨텍스트 윈도우 컨트롤, cctx usage 대시보드로 Copilot 토큰과 AI 크레딧 소비를 측정합니다."
track: copilot
verified: 2026-11-19
sidebar:
  order: 2
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

**문맥 점유율**, **누적 사용량**, **계정 제공량**을 구분해야 합니다. 서로 다른 화면의 백분율을 같은 값으로 해석하면 안 됩니다.

## 조치

| 환경 | 확인 경로 | 범위와 한계 |
|---|---|---|
| VS Code | 응답 위에 마우스 올리기·문맥 제어·Copilot 상태 대시보드 | 요청·세션·월간 크레딧 표시는 빌드와 과금 방식에 따라 다릅니다. |
| VS Code 진단 | Developer: Open Agent Debug Logs → Summary → Cache Explorer | 재현 **전에** 캡처를 켭니다. 패널은 Preview이며 Agent Host와 Local의 로깅 설정이 다릅니다. |
| CLI | /context·/usage | 문맥 구성과 세션·모델별 사용량을 구분합니다. 토큰 과금 계정은 AI Credits도 표시합니다. |
| SDK | assistant.usage·session.usage_info | 호출별 사용량과 현재 문맥 점유율입니다. 서브에이전트 호출도 포함하므로 중복 합산하지 않습니다. |

하네스에 맞는 [공식 캡처 설정](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)을 따릅니다. [Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)는 요청 차이를 설명할 뿐 최종 과금을 증명하지 않습니다. [SDK 사용량 이벤트](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing)는 실시간이며 재개 시 재생되지 않고 누적 지표 RPC는 실험적입니다. [cctx](../cctx/) 추정치는 선택적인 조회 관측값이지 두 번째 청구 장부가 아닙니다.

## 확인 방법

아래 미측정 기준선 표를 복사하고 한 번에 한 변수만 바꿔 같은 작업을 반복합니다.

| 항목 | 기준선 | 변경 후 |
|---|---|---|
| 작업·합격 검사·리비전 | 미기록 | 미기록 |
| 클라이언트·버전·하네스·모델·강도 | 미기록 | 미기록 |
| 인증·과금 방식 | 미기록 | 미기록 |
| 입력·캐시 읽기·쓰기·추론 포함 출력 | 미측정 | 미측정 |
| 실제 크레딧 또는 제공자 비용·시간 | 미측정 | 미측정 |
| 재시도·자식 호출·통과 검사 | 미측정 | 미측정 |
| 보고된 캐시 상태·지표 분모 | 미기록 | 미기록 |

각 수치가 호출·턴·세션·계정 중 어느 범위인지 명시합니다. 대표 작업을 반복하며 새 채팅이 콜드 캐시를 보장한다고 가정하지 않습니다. 품질이 유지될 때만 변경을 채택합니다.

## 절충점과 주의

누락된 필드는 0이 아니라 **확인 불가**입니다. 계정 수치에는 다른 작업이 섞이거나 반영이 늦을 수 있습니다. 내보낸 로그의 프롬프트·소스·경로·자격 증명을 검토한 뒤 공유합니다. 비용 팁은 가설일 뿐 측정의 대체재가 아닙니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 논문의 결과는 각자의 워크로드에서 검증해야 하는 가설입니다. 전체 목록은 [논문](../../papers/)을 참조합니다.

- [SGLang: Efficient Execution of Structured Language Model Programs](https://arxiv.org/abs/2312.07104) - NeurIPS 2023, 인용 1,783회. RadixAttention으로 접두부를 공유하는 호출 간에 KV 캐시를 재사용합니다. 접두부 캐싱의 기반이 되는 연구입니다.
- [Prompt Cache: Modular Attention Reuse for Low-Latency Inference](https://arxiv.org/abs/2311.04934) - MLSys 2023, 인용 417회. 반복되는 프롬프트 모듈의 사전 계산된 어텐션 상태를 재사용해 지연 시간을 줄입니다.
- [Don't Break the Cache: An Evaluation of Prompt Caching for Long-Horizon Agentic Tasks](https://arxiv.org/abs/2601.06007) - 프리프린트 2026, 인용 25회. 장기 에이전트 작업에서 프롬프트 캐싱의 효과를 평가합니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: debug](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [docs.github.com: context](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)
- [docs.github.com: cli](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)
- [docs.github.com: sdk](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing)
