---
title: "사용량 측정"
description: "VS Code 기본 제공 뷰로 비용이 큰 요청을 찾습니다."
track: copilot
verified: 2026-11-03
sidebar:
  order: 2
---

## 문제

"Copilot이 비싸다"는 느낌만으로는 원인을 알 수 없습니다. *어떤* 세션, 요청, 구성 요소에 비용이 가장 많이 드는지 확인해야 합니다.

## 조치

1. **요청별** — 채팅 응답 위에 마우스를 올리면 해당 턴에서 사용한 크레딧을 볼 수 있습니다.
2. **세션별** — 채팅 입력란의 **컨텍스트 윈도우 컨트롤**에 마우스를 올리거나 컨트롤을 선택합니다. 팝오버에
   세션 전체 비용(크레딧)과 누적 컨텍스트 윈도우 토큰 사용량이 표시됩니다.
3. **월별** — VS Code 상태 표시줄에서 **Copilot 상태 대시보드**를 열면 월 기본 제공량 중 사용한 비율을 볼 수 있습니다.
   GitHub의 사용량 세부 정보로 연결되는 링크도 있습니다.
4. **세션 내부 확인** — **Agent Debug Logs**를 엽니다(채팅 뷰의 `...` 메뉴 →
   *Show Agent Debug Logs*). **Summary** 뷰에서는 전체 토큰 사용량, 도구 호출, 소요 시간을 확인하고,
   **Logs** 뷰에서는 개별 요청, 오류, 도구 호출을 확인합니다.
5. **프롬프트 캐싱** — Summary에서 캐시 재사용률은 낮은데 소요 시간이나 입력 토큰이 예상보다 클 때
   **Cache Explorer**(Agent Debug Logs → Summary → *Cache Explorer*)를 사용합니다. 연속된 요청을 비교하면
   프롬프트 시그니처(시스템 지침, 도구 정의, 메시지)에서 *처음 달라진 지점*을 확인할 수 있습니다.
6. **맞춤 팁** — 채팅 세션에서 `/chronicle:cost-tips`를 실행하면 최근 활동을 바탕으로 한 권장 사항을 받을 수 있습니다.
7. **모델 비용** — 모델 선택기에 마우스를 올리면 토큰 유형별 비용과 Low / Medium / High 등급이 표시됩니다.

<figure class="at-doc-figure">
  <div class="at-doc-figure-grid">
    <img src="https://code.visualstudio.com/assets/docs/agents/chat-debug-view/agent-logs-summary-v3.png" alt="VS Code Agent Logs Summary 뷰에서 전체 토큰 사용량과 도구 호출 요약을 보여주는 화면" loading="lazy" />
    <img src="https://code.visualstudio.com/assets/docs/agents/cache-explorer/cache-explorer.png" alt="VS Code Cache Explorer에서 연속된 두 요청의 캐시된 접두부와 첫 불일치 지점을 비교하는 화면" loading="lazy" />
  </div>
  <figcaption>
    VS Code Agent Debug Logs의 Summary 뷰(왼쪽)와 연속된 요청의 프롬프트 캐시 불일치 지점을 비교하는 Cache Explorer(오른쪽). 출처: <a href="https://code.visualstudio.com/docs/copilot/chat/chat-debug-view">VS Code Docs — Agent Debug View</a> 및 <a href="https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer">Cache Explorer</a>.
  </figcaption>
</figure>

## 확인 방법

대표적인 작업 하나를 골라 같은 모델로 두 번 실행합니다. 요청별 크레딧, 세션 합계, 캐시 적중률(%)을 기록합니다.
이 값을 이후 변경 사항을 비교할 기준선으로 삼습니다.

## 트레이드오프와 주의 사항

- Cache Explorer의 비교 결과는 보고된 캐시 지표를 설명할 뿐, 제공자가 요청을 실제로 어떻게 처리하고 과금했는지를
  **증명하지는 않습니다**. 일부 모델에서는 캐시 정보가 없을 수도 있습니다.
- 캐시 재사용률을 높이려고 꼭 필요한 지침이나 도구를 제거하지 않습니다. 품질을 우선하세요.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 논문의 결과는 각자의 워크로드에서 검증해야 하는 가설입니다. 전체 목록은 [논문](../../papers/)을 참조합니다.

- [SGLang: Efficient Execution of Structured Language Model Programs](https://arxiv.org/abs/2312.07104) — NeurIPS 2023, 인용 1,783회. RadixAttention으로 접두부를 공유하는 호출 간에 KV 캐시를 재사용합니다. 접두부 캐싱의 기반이 되는 연구입니다.
- [Prompt Cache: Modular Attention Reuse for Low-Latency Inference](https://arxiv.org/abs/2311.04934) — MLSys 2023, 인용 417회. 반복되는 프롬프트 모듈의 사전 계산된 어텐션 상태를 재사용해 지연 시간을 줄입니다.
- [Don't Break the Cache: An Evaluation of Prompt Caching for Long-Horizon Agentic Tasks](https://arxiv.org/abs/2601.06007) — 프리프린트 2026, 인용 25회. 장기 에이전트 작업에서 프롬프트 캐싱의 효과를 평가합니다.

## 출처

- [Optimize AI credit usage in VS Code — Monitor and Inspect sections](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [Diagnose prompt caching with the Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [Debug chat interactions](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
