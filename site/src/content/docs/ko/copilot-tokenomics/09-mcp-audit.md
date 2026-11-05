---
title: MCP 감사
description: 사용량을 기준으로 MCP 서버를 점검하고 작업 영역별로 필요한 도구만 사용합니다.
track: copilot
verified: 2026-11-03
sidebar:
  order: 9
---

## 문제

활성화된 MCP 서버마다 **도구 정의(스키마)**가 모델 요청에 포함되어 전송됩니다. 사용하지 않아도 마찬가지입니다. 에이전트 루프에서는 이 비용이 단계 수만큼 곱해집니다.

```text
tokens per task ≈ (tool schema tokens × steps) + accumulated conversation + tool responses
```

사용하지 않는 서버도 단계마다 토큰을 소비합니다. 에이전트가 거치는 단계가 많을수록 비용도 커집니다.

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row" aria-label="MCP 서버 5개 사용: 도구 정의 65%, 나머지 접두부 35%">
      <span>MCP 서버 5개</span>
      <div class="at-diag-bar"><div class="at-diag-seg seg-tool" style="width:65%">도구 65%</div><div class="at-diag-seg seg-prefix" style="width:35%">기타 35%</div></div>
      <span>100%</span>
    </div>
    <div class="at-diag-row" aria-label="작업에 필요한 MCP 서버 1개 사용: 도구 정의 15%, 나머지 접두부 35%">
      <span>필요한 서버 1개</span>
      <div class="at-diag-bar"><div class="at-diag-seg seg-hit" style="width:15%">도구 15%</div><div class="at-diag-seg seg-prefix" style="width:35%">기타 35%</div></div>
      <span>50%</span>
    </div>
  </div>
  <figcaption>측정 결과가 아닌 가상의 예시입니다. 변경 전 접두부를 기준으로 도구 정의는 65%에서 15%로 줄고 나머지는 35%로 유지되어, 변경 후 접두부는 기존의 50%가 됩니다. 필요한 서버 하나만 남기면 매 턴 전송하는 도구 정의가 줄어들지만, 실제 절감량은 다릅니다. 초록색은 도구를 줄인 구성일 뿐 캐시 적중을 뜻하지 않습니다.</figcaption>
</figure>

## 조치

### 매월 감사를 실행합니다

1. 모든 MCP 서버와 서버별 도구 수를 나열합니다(도구 선택기인 **Configure Tools**에서 확인할 수 있습니다).
2. 서버마다 지난달에 얼마나 자주 사용했는지 기록합니다(Agent Debug Logs에서 도구 호출을 볼 수 있습니다).
3. 서버별로 다음과 같이 결정합니다.

| 사용량 | 결정 |
|-------|----------|
| 거의 매일 사용 | 유지 |
| 가끔 사용 | 기본적으로 **비활성화**, 필요한 작업에서만 활성화 |
| 거의 또는 전혀 사용하지 않음, 또는 다른 도구와 중복 | 제거 |
| 필요한 도구는 두 개뿐인 대형 서버 | 더 작은 서버 또는 터미널에서 호출하는 CLI 사용 |

4. 첫 요청의 입력 크기와 기준선 작업을 다시 측정합니다([사용량 측정하기](../02-measure-usage/)).

### 전역이 아니라 작업 영역별로 범위를 정합니다

MCP 구성을 리포지토리(`.vscode/mcp.json`)에 두면 프로젝트마다 필요한 서버만 로드됩니다. 백엔드 리포지토리에는 디자인 도구 서버가, 프런트엔드 리포지토리에는 데이터베이스 스키마 도구가 필요하지 않습니다. 리포지토리별 파일은 전역 목록보다 검토하기도 쉽습니다.

### 긴 지식은 스킬로 대체합니다

반복되거나 분량이 큰 참고 자료(사내 스타일 가이드, 런북, API 메모)는 상시 로드되는 지침에 두는 대신 필요할 때 로드되는 **에이전트 스킬**로 패키징할 수 있습니다. 반복해서 전송하는 입력을 줄이고 한 곳에서 내용을 관리할 수 있습니다.

### 도구 출력을 작게 유지합니다

도구 결과(파일 읽기, grep, 셸 출력)는 컨텍스트에서 가장 큰 부분을 차지하는 경우가 많습니다. 파일 전체를 읽기보다 대상을 좁혀 검색하고, 노이즈가 많은 경로는 제외합니다([컨텍스트를 간결하게 유지하기](../03-context-hygiene/) 참조).

## 확인 방법

도구를 가지치기하기 전후로 Agent Debug Logs에서 첫 요청의 입력 토큰을 기록합니다. 그런 다음 같은 작업을 실행해 크레딧과 성공 여부를 비교합니다. 품질이 유지된 경우에만 변경을 유지합니다.

## 트레이드오프

작업에 필요한 도구를 가지치기하면 우회 방법을 찾느라 단계가 늘어납니다. 도구를 빠르게 다시 활성화할 방법을 마련해 두고, 추측이 아닌 사용량을 근거로 도구를 가지치기합니다.

## 참고용 수치

이 문서의 바탕이 된 발표 자료에는 도구를 187개에서 52개로 줄인 팀이 나오며, 에이전트 작업의 토큰 사용량이 약 72% 감소했다고 보고합니다. 이 사례는 독립적으로 검증하지 못했습니다. *방법*의 예시로만 참고하고, 결과는 직접 측정하세요. TokenEconomizer는 여러 세션에 걸쳐 사용되지 않는 도구를 가지치기하는 방식으로 같은 아이디어를 자동화합니다([사례 연구](../../agent-tokenomics/case-tokeneconomizer/) 참조).

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 각 연구 결과는 가설이므로 자신의 워크로드에서 검증해야 합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) — 프리프린트 2025, 인용 55회. 모든 도구 스키마를 주입하지 않고, 에이전트가 필요한 도구를 직접 요청하게 합니다.
- [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) — 프리프린트 2025, 인용 41회. 모든 도구를 로드하는 대신 관련 있는 MCP 도구 설명만 검색해 가져옵니다(프롬프트 비대화 완화).
- [Less is More: Optimizing Function Calling for LLM Execution on Edge Devices](https://arxiv.org/abs/2411.15399) — 프리프린트 2024, 인용 33회. 함수 호출 시점에 선별한 소규모 도구 집합만 제시해 엣지 디바이스의 비용을 줄입니다.

## 출처

- [Optimize AI credit usage — Keep the tool catalog focused](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [Use tools with agents (VS Code)](https://code.visualstudio.com/docs/agents/run/tools)
- Token Efficiency and Context Engineering — 개발자 발표 자료(2026년 6월), MCP 부분(수치는 보고된 그대로)
