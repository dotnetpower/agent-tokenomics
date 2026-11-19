---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "컨텍스트를 가볍게 유지하기"
description: "새 채팅, /fork, /compact 컨텍스트 압축, #file 첨부, 제외 설정, cctx 블록 클램핑 조회로 VS Code Copilot 토큰 낭비를 줄입니다."
track: copilot
verified: 2026-11-19
sidebar:
  order: 3
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

무관한 기록은 후속 요청의 논리적 입력을 키웁니다. 반대로 문맥이 부족하면 수정 실패와 재시도가 늘 수 있습니다. 최소 바이트가 아니라 충분하고 신뢰할 수 있는 문맥을 목표로 합니다.

## 조치

| 상황 | 조치 | 확인 |
|---|---|---|
| 무관한 작업 | 짧은 인계 사항과 함께 새 채팅 | 이전 주제는 빠지고 필요한 결정은 남았는지 봅니다. |
| 대안 탐색 | 지원되는 환경에서 포크 | 기록은 복사되지만 캐시 재사용은 **보장되지 않습니다**. |
| 연관된 장기 작업 | 지원되는 /compact | 결정·제약·파일·남은 작업이 보존됐는지 확인합니다. |
| 관련 소스를 아는 경우 | 파일·심볼을 명시적으로 첨부 | 폴더 전체가 포함됐다고 추정하지 말고 실제 문맥을 봅니다. |
| 잡음이 많은 검색 | 경로·제외 설정 제한 | 검색 제외가 접근 제어 경계는 아닙니다. |
| 큰 새 도구 결과 | 삽입 전에 필터링 | 종료 상태·경고·검사 개수·관련 실패를 보존합니다. |

[클라이언트별 문맥 제어](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)와 [VS Code 문맥 제공 안내](https://code.visualstudio.com/docs/agents/guides/optimize-usage)를 따릅니다. [cctx](../cctx/)는 선택 사항이며 범위를 제한한 기본 검색도 유효한 기준선입니다.

<a id="subagent-delegation-break-even-rule"></a>
### 서브에이전트 위임의 손익분기 기준

중간 출력 격리와 모델 비용 이점이 시작·조율 비용보다 클 때 충분한 규모의 독립 작업을 위임합니다. 도구 호출 횟수 같은 경험칙을 보편적인 손익분기점으로 만들지 않습니다. [Local 서브에이전트는 주 대화 기록을 상속하지 않으며](https://code.visualstudio.com/docs/agents/run/subagents) Copilot 하네스는 다른 런타임 도구를 사용합니다. 부모·자식 작업을 각각 한 번만 집계합니다.

## 확인 방법

같은 모델·강도·합격 검사로 작업을 반복합니다. [기준선 표](../02-measure-usage/)에 문맥 크기, 실제 크레딧, 재시도, 시간, 성공 여부를 기록합니다.

## 절충점

압축은 손실이 있고 접두부를 바꿀 수 있으므로 당장 더 저렴하다고 보장할 수 없습니다. **과거** 도구 메시지를 임의로 줄여 덮어쓰지 않습니다. [접두부 변경은 캐시와 추론 연속성에 영향을 줄 수 있습니다](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged). 새 결과를 미리 필터링합니다. 포크는 기록을 유지하지만 새 세션은 다르며, 적중률이 낮아져도 필요한 문맥은 보존합니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 논문의 결과는 각자의 워크로드에서 검증해야 하는 가설입니다. 전체 목록은 [논문](../../papers/)을 참조합니다.

- [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172) - TACL 2023, 인용 5,818회. 관련 정보가 긴 입력의 중간에 있으면 정확도가 떨어집니다.
- [Large Language Models Can Be Easily Distracted by Irrelevant Context](https://arxiv.org/abs/2302.00093) - ICML 2023, 인용 1,358회. 프롬프트에 관련 없는 컨텍스트가 있으면 모델 정확도가 크게 낮아질 수 있습니다.
- [LLMs Get Lost In Multi-Turn Conversation](https://arxiv.org/abs/2505.06120) - ICLR 2025, 인용 562회. 모델은 요구 사항이 불충분한 멀티턴 대화에서 단일 턴보다 성능이 떨어집니다.
- [Same Task, More Tokens: the Impact of Input Length on the Reasoning Performance of Large Language Models](https://arxiv.org/abs/2402.14848) - ACL 2024, 인용 270회. 입력 길이만 따로 분리해, 입력이 길어질수록 추론 성능이 떨어짐을 보여 줍니다.
- [The Complexity Trap: Simple Observation Masking Is as Efficient as LLM Summarization for Agent Context Management](https://arxiv.org/abs/2508.21433) - 프리프린트 2025, 인용 66회. 에이전트 컨텍스트 관리에서 단순한 관찰 마스킹이 LLM 요약만큼 효율적입니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [docs.github.com: context](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)
- [code.visualstudio.com: subagents](https://code.visualstudio.com/docs/agents/run/subagents)
- [developers.openai.com: prefix](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
