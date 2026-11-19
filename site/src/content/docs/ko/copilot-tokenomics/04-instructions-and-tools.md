---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "지침과 도구"
description: "매 요청마다 반복 전송되는 사용자 지정 지침(copilot-instructions.md, AGENTS.md)과 도구 스키마 목록을 줄여 고정 토큰 세금을 낮춥니다."
track: copilot
verified: 2026-11-19
sidebar:
  order: 4
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

로드한 지침과 도구 스키마는 여러 요청에 반복 포함될 수 있습니다. 지침이 많다고 항상 좋은 것은 아니지만, 코드로 알 수 없는 제약을 지우면 재작업 비용이 늘어납니다.

## 조치

1. 확인된 빌드·테스트 명령과 저장소 특수 제약을 간결하게 남깁니다. 중복 규칙과 쉽게 찾을 수 있는 프로젝트 개요를 줄입니다.
2. 상주 지침·경로별 규칙·필요 시 로드하는 스킬을 구분합니다. 발견·적용 조건은 하네스마다 다르므로 [지침 참조](https://code.visualstudio.com/docs/agents/reference/custom-instructions-reference)를 확인합니다.
3. 범위마다 기준 규칙을 하나로 유지합니다. 발견한 파일이 모두 요청에 들어갔다거나 포함된 지침을 모델이 모두 지켰다고 가정하지 않습니다.
4. [클라이언트 도구 제어](https://code.visualstudio.com/docs/agents/run/tools)로 필요한 도구를 선택합니다. MCP 서버가 활성화됐다고 전체 스키마가 처음부터 전송된다는 뜻은 아닙니다.
5. 결정적인 일괄 작업은 검토한 스크립트로 실행하고 실패를 포함한 제한된 요약을 전달합니다. 광범위한 터미널 우회로 필요한 권한 확인을 대체하지 않습니다.

**Copilot 하네스**는 **Chat: Open Customizations → Tools**를 사용합니다. 클라이언트 도구 선택은 하네스·호스트에 유지되고 다른 세션에도 영향을 줄 수 있으며 요청 하나에만 적용되는 재설정이 아닙니다. Local의 제어는 다르므로 공유 선택을 바꾸기 전에 범위를 확인합니다.

| 환경 | 확인 경로 | 한계 |
|---|---|---|
| VS Code | 사용자 지정 진단·Chat Debug 요청 내용 | Local과 Copilot의 활성화·도구 동작이 다를 수 있습니다. |
| CLI | copilot instruction list·/context·/mcp | 발견 목록이 모든 요청의 포함을 증명하지는 않습니다. |
| SDK | 세션에 설정한 지침·도구·사용량 이벤트 | 고정한 SDK·런타임 스키마에서 확인합니다. |

[지침 발견·포함·준수 진단](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)을 따릅니다. 도구 목록 측정은 [MCP 감사](../09-mcp-audit/)에서 다룹니다.

## 확인 방법

중복 하나를 제거하거나 규칙 하나의 범위를 좁힌 뒤 같은 작업을 반복합니다. 첫 입력, 전체 세션 크레딧, 누락 제약, 재시도, 테스트를 비교하고 성공을 유지할 때만 채택합니다.

[수식 표기 예시](../06-prompt-and-output-style/#4-bpe-aligned-formula-notation)는 텍스트 감소를 측정할 수 있지만 짧은 규칙이나 남아 있는 키워드가 모델 이해를 증명하지는 않습니다. 재현 가능한 모델 평가 기록이 없는 기존 인식 정확도 주장은 이 문서의 근거로 사용하지 않습니다.

## 절충점

도구를 과도하게 줄이면 검색이나 수작업이 늘 수 있습니다. 필수 제약은 지우지 말고 선택적 지침을 필요할 때 로드합니다. 세션 중 변경은 요청 구성을 바꿀 수 있으므로 미적중을 단정하지 말고 보고된 재사용을 확인합니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 논문의 결과는 각자의 워크로드에서 검증해야 하는 가설입니다. 전체 목록은 [논문](../../papers/)을 참조합니다.

- [Batch Prompting: Efficient Inference with Large Language Model APIs](https://arxiv.org/abs/2301.08721) - EMNLP 2023, 인용 154회. 여러 샘플을 하나의 프롬프트에 담아 지침 토큰을 공유합니다.
- [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) - 프리프린트 2025, 인용 55회. 모든 도구 스키마를 주입하는 대신 에이전트가 필요한 도구를 요청하게 합니다.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) - 프리프린트 2025, 인용 41회. 모든 도구를 로드하는 대신 관련 있는 MCP 도구 설명만 검색해 가져옵니다(프롬프트 비대화 완화).
- [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents?](https://arxiv.org/abs/2602.11988) - 프리프린트 2026, 인용 36회. 리포지토리 컨텍스트 파일은 대체로 작업 성공률을 높이지 못했고 추론 비용을 평균 20% 넘게 늘렸습니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: instructions](https://code.visualstudio.com/docs/agents/reference/custom-instructions-reference)
- [code.visualstudio.com: debug](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
- [code.visualstudio.com: tools](https://code.visualstudio.com/docs/agents/run/tools)
- [docs.github.com: context](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)
- [docs.github.com: mcp](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)
