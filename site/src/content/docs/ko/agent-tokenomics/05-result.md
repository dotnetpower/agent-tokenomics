---
title: "5. 결과"
description: 기준선과 비교해 다시 측정하고 유지할지 되돌릴지 결정합니다.
track: agent
stage: result
verified: 2026-11-03
sidebar:
  order: 5
---

**목표:** 변경 전후를 공정하게 비교하고 결정을 기록합니다.

## 점검 항목

기준선과 *같은* 작업 집합, 반복 횟수, 환경으로 변경을 적용한 뒤 다시 실행합니다.

| 지표 | 기준선 | 수정 적용 후 | 변화 | 허용 가능한가? |
|------|--------|--------------|------|----------------|
| 작업당 평균 비용 | | | | |
| 입력 / 캐시 / 출력 토큰 | | | | |
| **성공률(가드레일)** | | | | 노이즈 범위를 넘어 떨어지면 안 됩니다 |
| 중앙값 지연 시간 | | | | |
| 수정 자체의 비용(추가 모델 호출, 유지 관리) | | | | 반드시 포함합니다(예: 요약 호출) |
| 캐시 적중률 | | | | 캐시를 무효화하는 압축은 절감 효과를 상쇄할 수 있습니다 |
| 용량과 지연 시간(이것이 목표인 경우) | | | | 요청당 분당 토큰 수, 사용률, p95 지연 시간, 첫 토큰까지의 시간 |
| 변경한 구성 요소(라우터, 분류기)의 결정 정확도 | | | | 3단계의 구성 요소 평가 기준. 평균만이 아니라 클래스별로 확인합니다 |
| 반드시 보존할 사실의 회상(손실이 있는 수정에만 해당) | | | | 3단계의 회상 점검 기준 |

TokenEconomizer가 보고서에서 하는 것처럼, 변경으로 발생하는 비용(예: 판정 모델, 요약기)을 **모두** 포함합니다.

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>기준선</span><div class="at-diag-bar"><span class="at-diag-seg seg-miss" style="width: 100%"></span></div><span>39,200 입력 토큰</span></div>
<div class="at-diag-row"><span>개선 가정</span><div class="at-diag-bar"><span class="at-diag-seg seg-hit" style="width: 38.8%"></span></div><span>약 15,200 입력 토큰</span></div>
</div>
<figcaption><a href="https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-requests.jsonl">샘플</a> s1의 턴 1 기준선은 4회 호출의 입력 합계입니다. 개선 수치는 도구 가지치기와 루프 가드 후 각각 7,200과 8,000 입력 토큰을 쓰는 호출 2회를 가정한 예시이며 측정 결과가 아닙니다. 성공률 유지도 가정일 뿐이므로 같은 작업을 반복해 품질과 비용을 검증해야 합니다. 두 변경은 각각 따로 실험하세요.</figcaption>
</figure>

## 결정

- 비용이 줄고 성공률이 유지되었다면 **유지**합니다. 변경 내용, 수치, 날짜를 기록합니다.
- 성공률이 떨어졌거나 절감 효과가 노이즈 범위 안이라면 **되돌립니다**.
- **반복:** 2단계로 돌아가 다음으로 큰 원인을 살펴봅니다. 절감 효과는 대개 하나의 큰 수정이 아니라
  여러 개의 작은 수정에서 나옵니다.

## 보고서 템플릿

```text
Change:        <what was done>
Cause proven:  <link to stage-3 record>
Cost / task:   <before> -> <after>  (<delta %>)
Success:       <before> -> <after>
Side costs:    <included>
Decision:      keep | revert
Next cause:    <from stage 2 ranking>
```

## 상태 유지

진단을 주기적으로 다시 실행합니다. 새로운 도구, 길어진 지침, 모델 변경으로 다시 낭비가 생길 수 있습니다.

## 추가 자료(논문)

학회 등급 순, 같은 등급 안에서는 인용 수 순으로 정렬했습니다. 각 결과는 자신의 워크로드에서 검증해야 할 가설로 취급합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [AI Agents That Matter](https://arxiv.org/abs/2407.01502) — TMLR 2024, 인용 289회. 에이전트 평가는 정확도뿐 아니라 비용도 통제해야 한다고 주장합니다.
- [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) — ICLR 2025, 인용 56회. 정답 하나를 얻는 데 드는 기대 비용을 정의하고 이를 기준으로 모델을 비교합니다.
- [τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045) — 프리프린트 2024, 인용 1,669회. 반복 실행에서 에이전트의 신뢰도를 측정하는 pass^k를 도입한 벤치마크입니다.
- [Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations](https://arxiv.org/abs/2411.00640) — 프리프린트 2024, 인용 194회. LLM 평가에서 불확실성을 보고하기 위한 통계적 지침을 제시합니다.
