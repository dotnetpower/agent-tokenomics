---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: MCP 감사
description: 사용량을 기준으로 MCP 서버를 점검하고 작업 영역별로 필요한 도구만 사용합니다.
track: copilot
verified: 2026-11-19
sidebar:
  order: 9
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

설치한 서버, 발견한 도구, 전체 스키마를 로드한 도구, 실제 실행한 호출을 구분합니다. 서버·도구 개수만으로 청구 토큰 절감을 입증할 수는 없습니다.

## 조치

| 감사 항목 | 기록 |
|---|---|
| 범위 | 클라이언트·버전·하네스·모델·저장소 설정 |
| 도구 목록 | 활성 서버·발견한 도구·확인 가능한 로드 정의 |
| 실제 작업 | 호출 수·결과 크기·실패·지연 |
| 결과 | 크레딧·재시도·테스트와 작업 성공 |

VS Code의 Copilot 하네스는 [Chat: Open Customizations → Tools](https://code.visualstudio.com/docs/agents/run/tools)와 [실제 요청](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)을 확인합니다. 선택은 하네스·호스트에 유지되어 다른 세션에도 영향을 줄 수 있으며 Local의 제어는 다릅니다. CLI는 [/mcp와 명시적 도구 선택](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)을 사용합니다. 저장소 설정은 검토 가능하게 유지하고 자격 증명은 커밋하지 않습니다.

### 필요 시 도구 로딩과 가상 그룹

지연 로딩·그룹화는 현재 하네스·모델·런타임에 따라 다릅니다. 과거 실험 설정인 `github.copilot.chat.virtualTools.threshold`를 **보편적인 현재 조정법으로 제시하지 않습니다.** 변경 전에 지원 여부를 확인합니다. “안 쓰면 무료”나 “활성화하면 매번 전체 스키마 전송”을 가정하지 말고 로드 정의, 검색 비용, 후속 호출을 비교합니다.

### 전역 대신 작업 영역에 맞춥니다

작업에 필요한 서버·도구만 활성화합니다. 설정 경로는 클라이언트마다 다르므로 VS Code의 `.vscode/mcp.json`을 다른 런타임에서도 자동으로 읽는다고 가정하지 않습니다. 도구 허용 목록과 실행 권한 정책은 서로 다른 문제를 해결합니다.

### 도구 출력을 줄입니다

새 결과를 기록에 넣기 전에 필터링하되 실패·개수는 보존합니다. Copilot CLI의 큰 출력 분리 기능을 쓰더라도 파일을 다시 통째로 읽지 말고 범위를 제한합니다. [문맥·출력 제어](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)를 참고합니다.

## 확인 방법

사용하지 않는 서버 하나를 끄거나 도구 목록 하나를 좁힌 뒤 같은 작업을 반복합니다. [기준선 표](../02-measure-usage/)로 첫 입력, 검색·로딩 호출, 전체 세션 크레딧, 성공 여부를 비교합니다. 필요한 도구를 없애 더 저렴하게 실패하는 대신 복구합니다.

## 절충점

지연 발견에는 호출이 추가될 수 있습니다. 일괄 터미널 스크립트는 반복 작업을 대체할 수 있지만 유지보수·보안 검토가 필요합니다. 원문 링크 없는 기존 발표 수치는 제외하며 도구 정의 감소 자체를 실측 종단 간 절감으로 제시하지 않습니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 각 연구 결과는 가설이므로 자신의 워크로드에서 검증해야 합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) - 프리프린트 2025, 인용 55회. 모든 도구 스키마를 주입하지 않고, 에이전트가 필요한 도구를 직접 요청하게 합니다.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) - 프리프린트 2025, 인용 41회. 모든 도구를 로드하는 대신 관련 있는 MCP 도구 설명만 검색해 가져옵니다(프롬프트 비대화 완화).
- [Less is More: Optimizing Function Calling for LLM Execution on Edge Devices](https://arxiv.org/abs/2411.15399) - 프리프린트 2024, 인용 33회. 함수 호출 시점에 선별한 소규모 도구 집합만 제시해 엣지 디바이스의 비용을 줄입니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: tools](https://code.visualstudio.com/docs/agents/run/tools)
- [docs.github.com: mcp](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)
- [code.visualstudio.com: debug](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [docs.github.com: context](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)
