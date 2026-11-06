---
title: "3. 문제 증명"
description: "LLM 에이전트 토큰 이코노믹스 3단계(증명): 고정된 작업 집합에서 반복 실행을 통해 토큰 낭비 규모와 신뢰구간을 정량적으로 입증합니다."
track: agent
stage: prove
verified: 2026-11-03
sidebar:
  order: 3
---

**목표:** 반복 가능한 측정으로 가설을 검증합니다.

## 점검 항목

1. **설정 고정:** 모델, 버전, temperature/effort, 프롬프트, 도구 집합, 작업 목록, 환경을 고정합니다.
2. **기준선 실행:** 모든 작업을 **최소 3회** 실행합니다(출력은 비결정적입니다). 작업별로 다음을 기록합니다.
   유형별 총 토큰, 비용, 성공 여부(통과/실패), 소요 시간.
3. **대조 실행:** 2단계의 분리 변경 하나만 적용하고(예: 사용하지 않는 도구를 비활성화),
   같은 작업 집합과 같은 반복 횟수로 다시 실행합니다.
4. **비교:** 해당 원인으로 추정한 낭비는 총합뿐 아니라 특정 지표(예: 요청 #1의 입력 토큰)의
   감소로 나타나야 합니다.

## 증거 기준

| 질문 | 필요한 증거 |
|------|-------------|
| 원인이 실재하는가? | 그 변수만 바꿨을 때 지표가 움직입니다. |
| 규모는 어느 정도인가? | 작업별, 전체별 반복 실행의 평균과 산포. |
| 일반적인 현상인가? | 둘 이상의 작업 / 세션 / 저장소에서 나타납니다. |
| 제거해도 안전한가? | 대조 실행의 성공률이 노이즈 범위를 넘어 낮아지지 않습니다. |

반복 실행 간 산포가 효과보다 크면 반복 횟수를 늘리거나 더 큰 대상을 선택합니다.

<a id="cost-measurements-versus-pass-rate-guardrails"></a>
### 비용 측정과 성공률 가드레일의 표본 크기

예를 들어 **가상 사례**에서 고정 접두부가 결정적으로 2,000토큰 줄었다면, 작업당 3–5회 반복으로 토큰 변화량을 확인할 수 있습니다. 단, 관측한 산포가 작아야 하며 변동이 큰 경우에는 더 반복해야 합니다. 이 정도 반복만으로 성공률이 동등하다고 결론 내릴 수는 없습니다.

Pass@1의 3–5%p 차이를 살피려면 훨씬 큰 고정 작업 집합을 사용하고 대응 표본의 신뢰구간 또는 검정력 분석을 먼저 계산하세요. 20–50개 이상의 작업은 탐색을 시작하는 규모일 뿐 **충분한 표본이라는 보장은 없습니다**. 작은 차이를 검출하는 데 필요한 작업 수는 대응 결과, 목표 검정력, 동등성 허용 범위에 따라 수백 개 이상일 수도 있습니다. 불확실성을 보고하고 구간이 너무 넓으면 가드레일 판단을 보류하세요([Adding Error Bars to Evals](https://arxiv.org/abs/2411.00640)).

<a id="when-the-fix-is-lossy-summaries-compaction"></a>
<a id="손실이-있는-수정요약-압축일-때"></a>
## 요약이나 압축으로 정보가 손실될 수 있을 때

기록을 압축하면 같은 결과를 *보장*할 수 없습니다. 자신의 작업에서 결과가 동등한지 확인해야 합니다.
실험을 실행하기 전에 "동등"의 기준을 정의합니다.

1. **성공 동등성** — 같은 작업에서 충분한 관측치를 확보해 대응 표본의 신뢰구간이 미리 정한 동등성 허용 범위 안에 들어가는지 확인합니다. 그렇지 않으면 판단을 보류합니다.
2. **보존할 정보** — 반드시 남아야 하는 항목(결정 사항, 제약 조건, 파일 경로, 오류 시그니처,
   미해결 TODO)을 목록으로 만듭니다. 압축 후 이 목록에 답이 있는 질문을 던지고 정확하게 기억해 낸 횟수를 셉니다.
3. **새로운 재작업 없음** — 작업당 재질문 횟수와 반복 탐색 횟수가 늘지 않습니다.
4. **순비용** — 절약한 입력 토큰에서 요약 호출 비용과 손실된 프롬프트 캐시 재사용분
   ([개선](../04-improve/#compaction-without-losing-the-cache) 참고)을 뺀 값이 양수입니다.

정보를 다시 찾아내지 못했을 때 요약이 무엇을 누락했는지 추적할 수 있도록 원본 대화 기록을 보관합니다.

## 구성 요소 수준 평가(라우터, 분류기, 가드레일)

파이프라인에서는 작은 결정 단계(경로 선택, 분류, 허용 또는 차단)가 최종 답변에 영향을 줄 수 있습니다. 이런 단계의 프롬프트나 도구 설명을 줄이는 수정이라면
엔드 투 엔드 성공률과 함께 **구성 요소 수준 평가 집합**으로 증명합니다.

1. 실제와 유사한 형태의 사례에 레이블을 붙인 집합을 만듭니다(보통 수백 건이며, 프로덕션에서 민감 정보를 가려 샘플링합니다).
2. 같은 사례로 변경 전후의 **결정 정확도**와 혼동 행렬을 측정합니다.
3. 실행하기 **전에** 클래스별 최소 허용 정확도를 정합니다(드물지만 비용이 큰 클래스가
   평균보다 더 중요합니다).
4. 이 점검 없이 도구 설명이나 출력 스키마를 줄이지 않습니다. 이 둘은 답변 정확도에 영향을 줍니다.
5. 정적 문서를 수정하는 경우에는 총합뿐 아니라 변경 전후의 `tokens × calls`도 함께 보고합니다.

<a id="multi-step-reliability"></a>
## 다단계 신뢰도

단계별 오류는 누적됩니다. 각 단계가 확률 *p*로 성공하면 *n*단계 실행이 성공할 확률은 (단계가 서로 독립일 때)
*pⁿ*입니다. 이는 에이전트를 측정한 값이 아니라 단순한 산술이지만, 긴 루프가 왜 취약한지 보여 줍니다.

| 단계 수 | p = 0.95 | p = 0.99 |
|--------:|---------:|---------:|
| 5 | 0.774 | 0.951 |
| 10 | 0.599 | 0.904 |
| 20 | 0.358 | 0.818 |
| 50 | 0.077 | 0.605 |

증명 시 고려할 점: 총 토큰이 아니라 **성공한 작업당 비용**을 보고합니다. 또한 루프를 *단축하는* 수정
(명확한 완료 기준, 미리 알려 둔 저장소 사실, 줄어든 재시도)은 비용을 줄이면서 성공률도 높일 수 있습니다.

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>5단계</span><div class="at-diag-bar"><span class="at-diag-seg seg-hit" style="width: 77%"></span></div><span>77%</span></div>
<div class="at-diag-row"><span>10단계</span><div class="at-diag-bar"><span class="at-diag-seg seg-hist" style="width: 60%"></span></div><span>60%</span></div>
<div class="at-diag-row"><span>20단계</span><div class="at-diag-bar"><span class="at-diag-seg seg-miss" style="width: 36%"></span></div><span>36%</span></div>
</div>
<figcaption>단계마다 독립적으로 p = 0.95의 성공 확률을 가정한 p<sup>n</sup>의 반올림 값입니다. 실제 에이전트 측정치는 아닙니다.</figcaption>
</figure>

## 템플릿

```text
Hypothesis:   <one sentence with a number>
Variable:     <the single thing changed>
Tasks / reps: <N tasks × R runs>
Baseline:     tokens=<mean±sd>  cost=<>  success=<x/N>
Counter-run:  tokens=<mean±sd>  cost=<>  success=<x/N>
Conclusion:   confirmed | rejected | inconclusive (why)
```

## 적용 예시

샘플에서 "한 턴에 동일한 `pytest: command not found` 오류가 세 번 발생해 피할 수 있는 추가 요청 두 건과 컨텍스트 재전송이 발생한다"는 가설은, 올바른 명령(`python -m pytest`)을 처음부터 알려 준 채 같은 작업을 실행해 증명합니다. 이때 턴당 요청 수가 줄고 성공률은 같게 유지되는지 확인합니다.

## 추가 자료(논문)

학회 등급 순, 같은 등급 안에서는 인용 수 순으로 정렬했습니다. 각 결과는 자신의 워크로드에서 검증해야 할 가설로 취급합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [Evaluating Very Long-Term Conversational Memory of LLM Agents](https://arxiv.org/abs/2402.17753) — ACL 2024, 인용 1,082회. 매우 장기적인 대화 메모리를 평가하는 벤치마크입니다.
- [LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory](https://arxiv.org/abs/2410.10813) — ICLR 2024, 인용 869회. 채팅 어시스턴트의 장기 메모리를 평가하는 벤치마크로, 회상 점검을 설계할 때 유용합니다.
- [AI Agents That Matter](https://arxiv.org/abs/2407.01502) — TMLR 2024, 인용 289회. 에이전트 평가는 정확도뿐 아니라 비용도 통제해야 한다고 주장합니다.
- [τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045) — 프리프린트 2024, 인용 1,669회. 반복 실행에서 에이전트의 신뢰도를 측정하는 pass^k를 도입한 벤치마크입니다.
- [Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations](https://arxiv.org/abs/2411.00640) — 프리프린트 2024, 인용 194회. LLM 평가에서 불확실성을 보고하기 위한 통계적 지침을 제시합니다.
- [Non-Determinism of"Deterministic"LLM Settings](https://arxiv.org/abs/2408.04667) — 프리프린트 2024, 인용 159회. 결정적으로 동작하도록 설정한 경우에도 실행마다 출력이 달라짐을 보여 줍니다.

## 다음 단계

가설이 **확인되면** [4. 개선](../04-improve/)으로 이동하세요.
