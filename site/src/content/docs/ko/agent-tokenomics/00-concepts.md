---
title: "0. 개념: 에이전트는 토큰을 어떻게 쓰는가"
description: 토큰 비용을 진단하기 전에 요청 구성, 에이전트 루프, 과금, 캐싱, 컨텍스트의 기본 개념을 살펴봅니다.
track: agent
verified: 2026-11-03
sidebar:
  order: 0
---

[문제 해결 단계](../01-diagnose/)를 시작하기 전에 이 페이지를 읽어 보세요. 이후 단계에서는 여기서 설명하는 개념을 바탕으로 토큰 비용을 진단합니다.

## 1. 모델은 이전 호출을 기억하지 않습니다 (무상태 호출)

모델은 호출과 호출 사이의 내용을 기억하지 않습니다. 에이전트가 이전 내용을 기억하는 것처럼 보이는 이유는 지침, 도구 정의, 지금까지의 대화를 **요청마다 다시 보내기 때문**입니다. 따라서 토큰 비용은 사용자가 입력한 내용뿐 아니라 프레임워크가 **요청마다 함께 담아 보내는 내용**에도 크게 좌우됩니다.

## 2. 요청 한 건의 구성 요소

| 구성 요소 | 입출력 | 요청마다 바뀌나요? | 흔한 낭비 |
|-----------|--------|--------------------|-----------|
| 시스템 프롬프트(프레임워크와 사용자 지침) | 입력 | 바뀌지 않음(고정) | 길고 반복되는 규칙 |
| 지침 파일 / 기본 프로젝트 정보 | 입력 | 드물게 바뀜 | 코드에서 이미 알 수 있는 정보, 중복된 내용 |
| **도구 정의**(스키마) | 입력 | 드물게 바뀜 | 한 번도 호출하지 않는 도구 |
| 대화 기록(사용자, 어시스턴트) | 입력 | 턴마다 증가 | 이전 주제, 이미 해결된 우회 작업 |
| **도구 결과**(파일 내용, 명령 출력) | 입력 | 단계마다 증가 | 파일 전체, 장황한 로그 |
| 첨부 파일 / 검색된 컨텍스트 | 입력 | 작업마다 바뀜 | 관련 없거나 중복된 컨텍스트 |
| 현재 사용자 프롬프트 | 입력 | 바뀜 | 보통 요청에서 차지하는 비중이 가장 작음 |
| 모델의 텍스트 응답, 도구 호출 | 출력 | 바뀜 | 장황한 답변, 질문 되풀이 |
| 추론("thinking") | 출력(보통 출력 토큰으로 과금) | 바뀜 | 작업에 필요한 수준보다 높은 추론 강도 |

표의 위쪽에 있는 구성 요소는 호출마다 거의 바뀌지 않는 **접두부**를 이룹니다. 접두부가 중요한 이유는 [프롬프트 캐싱](#5-프롬프트-캐싱)에서 살펴봅니다.

## 3. 에이전트 루프가 길어질수록 토큰 사용량은 빠르게 불어납니다

```text
step 1: model call ─► tool call ─► result appended
step 2: model call (re-sends prefix + history + result 1) ─► tool call ─► result appended
step 3: model call (re-sends prefix + history + result 1..2) ─► …
```

각 단계에서 모델을 호출할 때마다 요금이 발생하고, 이전 내용도 다시 전송합니다. *n*단계로 이루어진 작업의 입력 토큰은 다음과 같이 나타낼 수 있습니다.

```text
input tokens ≈ n × prefix + Σ(history and tool results accumulated before each step)
```

첫 번째 항은 단계 수에 비례해 늘어납니다. 두 번째 항은 단계마다 쌓인 내용을 이후 호출에서 반복해서 보내므로 그보다 빠르게 늘어납니다. 따라서 **단계 수를 줄이고**, **도구 결과를 간결하게 만들고**, **접두부를 줄이면** 비용을 낮출 수 있습니다. 같은 작업을 반복하는 루프에 빠지면 비용이 특히 커집니다.

**고정 파이프라인에서도 호출마다 비용이 누적됩니다.** 모든 에이전트가 자유롭게 루프를 돌며 작업하는 것은 아닙니다. 실제 서비스에서는 사용자 요청 하나에 정해진 순서로 모델을 호출하기도 합니다. 예를 들어 안전성 검사 → 쿼리 재작성 → 라우터 → 분류기 → 답변 → 후속 질문 → 최종 응답의 순서로 처리할 수 있습니다. 사용자에게는 질문 하나로 보이지만, 제공자는 **단계마다 모델 호출 한 건**을 과금합니다. 각 단계에서 사용하는 시스템 프롬프트와 참조 문서도 비용에 포함됩니다. 이렇게 사용자 요청 하나에서 발생하는 LLM 호출 수를 **팬아웃**이라고 합니다. 여러 단계에 넣거나 같은 단계를 두 번 호출할 때 사용하는 큰 자료에는 `tokens × calls`만큼의 비용이 발생합니다.

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row">
      <span>Step 1 (Call #1)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-prefix" style="width:42%">Prefix (System + Tools)</div>
        <div class="at-diag-seg seg-user" style="width:10%">User</div>
      </div>
      <span>9,200 tok</span>
    </div>
    <div class="at-diag-row">
      <span>Step 2 (Call #2)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-prefix" style="width:42%">Prefix (재전송)</div>
        <div class="at-diag-seg seg-hist" style="width:12%">History</div>
        <div class="at-diag-seg seg-tool" style="width:18%">Tool Result 1</div>
      </div>
      <span>13,800 tok</span>
    </div>
    <div class="at-diag-row">
      <span>Step 3 (Call #3)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-prefix" style="width:42%">Prefix (재전송)</div>
        <div class="at-diag-seg seg-hist" style="width:20%">History + Result 1</div>
        <div class="at-diag-seg seg-tool" style="width:28%">Tool Result 2</div>
      </div>
      <span>19,500 tok</span>
    </div>
  </div>
  <figcaption>
    그림 1. 에이전트 루프 단계가 진행될수록 고정 접두부(<code>n × prefix</code>)가 반복 전송되고 이전 단계의 대화 기록·도구 결과(<code>Σ</code>)가 누적되어 입력 토큰이 늘어나는 구조. 참조: <a href="https://arxiv.org/abs/2210.03629">ReAct (ICLR 2022)</a> 및 <a href="https://github.com/microsoft/TokenEconomizer">microsoft/TokenEconomizer</a>.
  </figcaption>
</figure>

## 4. 요금이 부과되는 토큰 유형

| 항목 | 설명 | 중요한 이유 |
|------|------|-------------|
| 캐시되지 않은 입력 토큰 | 처음부터 처리한 입력 | 입력 정가 적용 |
| **캐시된 입력 토큰** | 제공자의 프롬프트 캐시에서 처리한 입력 | 대부분의 제공자가 할인된 요율로 과금 |
| 출력 토큰(추론 포함) | 모델이 생성한 모든 내용 | 보통 입력보다 토큰당 단가가 높음 |

요율은 제공자와 모델에 따라 다르고 수시로 바뀝니다. 기사에 나온 비율에 의존하지 말고 사용하는 모델의 최신 가격표를 확인하세요. 요청 한 건의 총비용은 `uncached_in × p_in + cached_in × p_cached + out × p_out`으로 계산하고, 이를 모든 요청에 대해 합산합니다.

**비용 절감만이 유일한 목표는 아닙니다.** 최적화할 대상에 따라 살펴볼 지표도 달라집니다.

| 목표 | 일반적인 상황 | 지표 |
|------|---------------|------|
| 비용 | 종량제 또는 크레딧 | 작업당 유형별 토큰, 성공 작업당 비용 |
| 처리 용량 | 예약 / 프로비저닝 처리량, 속도 제한 | 요청당 분당 토큰, 사용률, 동시 처리 사용자 수 |
| 지연 시간 | 대화형 사용 | 호출당 입력 토큰, 첫 토큰까지의 시간, p95 종단 간 시간 |

입력 토큰을 줄이면 대체로 세 가지 목표 모두에 도움이 됩니다. 캐싱은 사용하는 배포 유형에서 제공자가 캐시된 토큰을 유리하게 계산하는 경우에만 비용과 처리 용량을 개선합니다. 5절의 점검 목록을 확인하세요.

### 언어별 토큰 계산 방식과 차이 (tiktoken)

모델은 글자나 단어 수를 그대로 세지 않습니다. OpenAI의 [`tiktoken`](https://github.com/openai/tiktoken)처럼 **바이트 페어 인코딩(BPE)**을 사용하는 토크나이저로 UTF-8 바이트를 단어보다 작은 조각으로 나눕니다. `o200k_base`는 GPT-4o, GPT-4.1, GPT-5, o-series에서, `cl100k_base`는 GPT-4와 GPT-3.5에서 사용합니다. BPE 어휘는 주로 영어와 소스 코드로 학습되므로 같은 뜻이라도 언어에 따라 토큰 수가 달라집니다. 다음은 `examples/count_tokens.py`와 `tiktoken` 0.14.0으로 측정한 결과입니다.

| 언어 | 예문 | 글자 수 | UTF-8 바이트 | `o200k_base` 토큰 | `cl100k_base` 토큰 |
|------|------|--------:|-------------:|------------------:|-------------------:|
| 영어 | I met a huge dog | 16 | 16 | **5** (1.0×) | **5** (1.0×) |
| 스페인어 | Conocí a un perro enorme | 24 | 25 | **7** (1.4×) | **8** (1.6×) |
| 한국어 | 나는 커다란 개를 만났다 | 13 | 35 | **8** (1.6×) | **13** (2.6×) |
| 일본어 | 大きな犬に出会った | 9 | 27 | **8** (1.6×) | **11** (2.2×) |
| 중국어 | 我遇見了一隻大狗 | 8 | 24 | **8** (1.6×) | **12** (2.4×) |

<figure class="at-doc-figure">
  <div class="at-tok-box">
    <div class="at-tok-row">
      <span class="at-tok-label">English (5 tokens)</span>
      <span class="at-tok-chip c1">I</span>
      <span class="at-tok-chip c2">·met</span>
      <span class="at-tok-chip c3">·a</span>
      <span class="at-tok-chip c4">·huge</span>
      <span class="at-tok-chip c5">·dog</span>
    </div>
    <div class="at-tok-row">
      <span class="at-tok-label">한국어 o200k_base (8 tokens)</span>
      <span class="at-tok-chip c1">나는</span>
      <span class="at-tok-chip c2">·커</span>
      <span class="at-tok-chip c3">다</span>
      <span class="at-tok-chip c4">란</span>
      <span class="at-tok-chip c5">·개</span>
      <span class="at-tok-chip c1">를</span>
      <span class="at-tok-chip c2">·만</span>
      <span class="at-tok-chip c3">났다</span>
    </div>
    <div class="at-tok-row">
      <span class="at-tok-label">한국어 cl100k_base (13 tokens)</span>
      <span class="at-tok-chip c1">나</span>
      <span class="at-tok-chip c2">는</span>
      <span class="at-tok-chip c-byte">·커[1/2]</span>
      <span class="at-tok-chip c-byte">커[2/2]</span>
      <span class="at-tok-chip c3">다</span>
      <span class="at-tok-chip c-byte">란[1/2]</span>
      <span class="at-tok-chip c-byte">란[2/2]</span>
      <span class="at-tok-chip c4">·개</span>
      <span class="at-tok-chip c5">를</span>
      <span class="at-tok-chip c1">·만</span>
      <span class="at-tok-chip c-byte">났[1/2]</span>
      <span class="at-tok-chip c-byte">났[2/2]</span>
      <span class="at-tok-chip c2">다</span>
    </div>
  </div>
  <figcaption>
    그림 2. <code>tiktoken</code>에서 같은 의미의 문장이 토큰 조각으로 나뉘는 실제 결과(<code>·</code>는 앞 공백 포함). <code>cl100k_base</code>에서는 <code>커</code>, <code>란</code>, <code>났</code> 같은 음절이 UTF-8 바이트 조각 2개로 쪼개져 13토큰이 되지만, <code>o200k_base</code>에서는 자주 쓰이는 한국어 음절·단어가 병합되어 8토큰으로 줄어듭니다. 출처: <a href="https://github.com/openai/tiktoken">openai/tiktoken</a>, <a href="https://platform.openai.com/tokenizer">OpenAI Tokenizer</a>, <a href="https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py"><code>examples/count_tokens.py</code></a>.
  </figcaption>
</figure>

에이전트를 만들 때 다음 세 가지를 고려하세요.

1. **글자 수가 적다고 토큰 수도 적은 것은 아닙니다.** 위 예문에서 한국어, 일본어, 중국어는 영어보다 글자 수가 적지만, 한 음절이나 한자는 UTF-8에서 2~3바이트를 차지하고 여러 토큰으로 나뉘기도 합니다.
2. **모델에 맞는 토크나이저 인코딩을 선택하세요.** 위 예문에서 새로운 어휘인 `o200k_base`는 `cl100k_base`보다 비영어 텍스트의 토큰 배율을 크게 줄였습니다. 한국어는 `2.6× → 1.6×`로 달라졌습니다. `tokprof`로 로컬에서 측정할 때도 모델에 맞는 인코딩을 지정하세요(예: `tokprof --tokenizer o200k_base`).
3. **반복되는 접두부에는 영어 또는 BPE에 유리한 표기를 고려하세요.** 시스템 프롬프트, 기본 규칙, 도구 설명은 호출마다 다시 전송되므로 언어에 따른 토큰 차이도 `tokens × calls`만큼 누적됩니다. 자세한 프롬프트 작성 방법은 [언어와 토큰화](../../copilot-tokenomics/07-language-and-tokenization/)를 참고하세요.

## 5. 프롬프트 캐싱

제공자는 최근에 처리한 요청과 **앞부분이 가장 길게 일치하는 접두부**를 재사용할 수 있습니다. 캐시가 적중하려면 요청의 시작 부분이 *완전히 동일*해야 합니다. 지침 순서를 바꾸거나 도구 목록을 수정하거나 기록을 다시 쓰면, 달라진 지점부터는 캐시를 재사용할 수 없습니다. 동작은 제공자, 모델, 배포 유형에 따라 다르며 모델을 바꾸면 이전 모델의 캐시를 재사용하지 못합니다.

설계할 때는 **고정된 내용을 앞에 두고, 바뀌는 내용을 뒤에 두며, 기록은 기존 내용을 수정하지 않고 추가**하세요. 직접 개발한 에이전트에서는 [`tokprof`](../01-diagnose/#코드-변경-없이-프로파일링하기-tokprof)로 호출별 `cached_tokens` 비중과 시스템 프롬프트 해시 변동을 확인할 수 있습니다. 제공자의 공식 안내와 가격표도 참고하세요. [Azure OpenAI](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching), [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching), [Anthropic Claude](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [Google Gemini](https://ai.google.dev/gemini-api/docs/caching), [Azure OpenAI 요금](https://azure.microsoft.com/ko-kr/pricing/details/cognitive-services/openai-service/).

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row">
      <span>정적 접두부 유지</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-hit" style="width:74%">✓ CACHE HIT: System + Tools + 이전 기록 (≥ 1,024 tok 동일)</div>
        <div class="at-diag-seg seg-user" style="width:26%">신규 턴</div>
      </div>
      <span>적중 (할인)</span>
    </div>
    <div class="at-diag-row">
      <span>앞부분 변경 발생</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-hit" style="width:18%">일치</div>
        <div class="at-diag-seg seg-miss" style="width:82%">✗ CACHE MISS: 동적 시각·도구 순서 변경 지점부터 전체 재계산</div>
      </div>
      <span>미적중 (정가)</span>
    </div>
  </div>
  <figcaption>
    그림 3. 요청 앞부분의 접두부(최소 1,024토큰 이상)가 완전히 동일할 때만 프롬프트 캐시가 적중하며, 앞쪽에 동적 값(현재 시각, 순서가 바뀐 도구 목록 등)이 들어가면 그 지점부터 캐시가 무효화됩니다. 참조: <a href="https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching">Azure OpenAI 프롬프트 캐싱</a>, <a href="https://developers.openai.com/api/docs/guides/prompt-caching">OpenAI Prompt Caching</a>, <a href="https://arxiv.org/abs/2312.07104">SGLang RadixAttention (NeurIPS 2023)</a>.
  </figcaption>
</figure>

### 제공자 캐싱: 확인할 사항

캐싱 규칙은 제공자, 모델 계열, 배포 유형에 따라 다르고 수시로 바뀝니다. 캐싱을 전제로 설계하기 전에 다음 질문의 답을 공식 문서에서 확인해 기록하세요. 오른쪽 열은 주로 [Azure OpenAI](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching)와 [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching)의 문서를 2026-11-03 기준으로 정리한 것입니다. 실제 사용하는 모델의 조건은 다시 확인하세요.

| 질문 | 중요한 이유 | 문서 내용(2026-11-03 기준) |
|------|-------------|----------------------------|
| 캐시할 수 있는 최소 접두부 길이는? | 짧은 프롬프트(가드레일, 소형 분류기, 요약기)는 캐시되지 않을 수 있음 | [Azure OpenAI](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching): 요청이 1,024토큰 이상이어야 하고 처음 1,024토큰이 동일해야 하며, 이후 128토큰 단위로 일치 여부를 확인합니다. [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching): GPT-5.6 이상은 최소 1,024토큰이며 이전 모델은 기준이 다릅니다. [Anthropic Claude](https://platform.claude.com/docs/en/build-with-claude/prompt-caching): Claude 모델에 따라 1,024~4,096토큰입니다. [Google Gemini](https://ai.google.dev/gemini-api/docs/caching)도 모델별 조건을 확인하세요. |
| 어떤 모델과 보존 정책을 지원하나요? | 모델이나 확장 보존 지원 여부에 따라 계획이 달라짐 | [Azure OpenAI 지원 모델](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching#supported-models): GPT-4o 이상에서 인메모리 보존을 지원합니다. 확장 보존은 목록에 있는 일부 모델에서만 지원하므로 `mini` 변형을 포함해 정확한 모델을 확인하세요. |
| 적중 여부는 어디에서 확인하나요? | 적중 여부를 확인할 수 있어야 최적화할 수 있음 | [Azure chat completions](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching): `usage.prompt_tokens_details.cached_tokens`에서 확인합니다. 0이면 미적중입니다. GPT-5.6 이상 Standard 배포에서는 `cache_write_tokens`도 제공합니다. |
| 캐시 키가 도움이 되나요? | 관련 요청을 함께 라우팅하면 적중률이 높아짐 | [Azure / OpenAI (`prompt_cache_key`)](https://developers.openai.com/api/docs/guides/prompt-caching): GPT-5.6 이상에서는 공유 접두부마다 `prompt_cache_key` 하나를 재사용합니다. 하나의 접두부와 키 조합에서 분당 약 15개 요청을 넘으면 일부 요청이 미적중할 수 있으므로 일정한 매핑 규칙으로 여러 키에 분산하세요. |
| 얼마 동안 보관되나요? | 사용하지 않는 동안 캐시가 유지되는지 판단 | [Azure OpenAI 보존 기간](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching): 인메모리 캐시는 보통 5~10분 동안 사용하지 않으면 삭제되며, 최대 1시간 이내에는 항상 삭제됩니다. 확장 보존은 최대 24시간이며 키/값 텐서를 GPU 로컬 저장소로 옮깁니다. GPT-5.6 이상에서는 정책 필드가 사용 중단되고 최소 30분 수명이 적용됩니다. |
| 과금과 집계는 어떻게 이루어지나요? | 캐싱으로 비용, 처리 용량 또는 둘 다 절약할 수 있는지 판단 | [Azure OpenAI 요금](https://azure.microsoft.com/ko-kr/pricing/details/cognitive-services/openai-service/): Standard에서는 캐시 읽기에 할인이 적용되고, Provisioned 배포에서는 입력을 최대 100% 할인합니다. GPT-5.6 이상에서는 캐시 쓰기에 요금이 부과될 수 있습니다. [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching): 가격은 모델마다 다릅니다. 캐시된 토큰이 처리 용량 사용률에 어떻게 반영되는지도 확인하세요. |
| 무엇을 어디에 저장하나요? | 보안과 데이터 보관 위치 검토 | [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching): 캐시에는 원문 텍스트가 아닌 키/값 텐서를 저장합니다. [Azure OpenAI](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching): 구독 간에 캐시를 공유하지 않으며, 확장 보존 데이터는 배포 유형의 지역 또는 데이터 영역 경계 안에 유지됩니다. |
| 비활성화하거나 삭제할 수 있나요? | 일부 규정 준수 상황에서 필요 | [Azure OpenAI](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching): GPT-5.6 이상 Standard 배포에서는 중단점 없이 `explicit` 모드를 사용하면 캐싱이 비활성화됩니다. 이전 모델과 관리형 프로비저닝 배포에서는 비활성화할 수 없습니다. |

캐시할 수 없을 만큼 짧은 프롬프트라면 설계를 점검해 보세요. 해당 단계를 인접한 단계와 합치거나, 일반 코드(규칙 기반 검사)로 처리하거나, 작은 비용을 그대로 받아들일 수 있습니다. 캐시 기준을 맞추려고 의미 없는 토큰을 추가하지 마세요.

## 6. 도구가 발생시키는 두 가지 토큰 비용

1. **정의 비용**: 활성화한 모든 도구의 스키마는 실제 사용 여부와 관계없이 매 요청의 접두부에 포함됩니다.
2. **결과 비용**: 도구가 반환한 내용은 기록에 추가되어 이후 단계에서 다시 전송됩니다.

사용하지 않는 도구를 제외하는 **도구 가지치기**로 첫 번째 비용을 줄일 수 있습니다. 결과를 잘라 내거나 페이지로 나누거나 요약하면 두 번째 비용을 줄일 수 있습니다.

## 7. 품질이 뒷받침되지 않는 비용 절감은 의미가 없습니다

비용이 적게 들더라도 작업에 실패했다면 절감이라고 볼 수 없습니다. **성공 작업당 비용**을 추적하세요. 긴 루프에서는 단계별 오류가 누적되므로([입증](../03-prove/#multi-step-reliability)의 표 참조), 루프를 줄이면 성공률을 높이면서 비용도 줄일 수 있습니다.

## 8. 핵심 개념과 낭비 원인 연결하기

| 개념 | 발생하는 낭비 | 진단할 위치 |
|------|---------------|-------------|
| 무상태 호출의 재전송, 큰 접두부 | 고정 오버헤드, 프로젝트 정보를 반복해서 다시 파악 | [진단](../01-diagnose/), [원인 분리](../02-isolate/) |
| 도구 정의 비용 | 사용하지 않는 도구 | [원인 분리](../02-isolate/) |
| 도구 결과 비용 | 과도하게 큰 출력 | [개선](../04-improve/) |
| 루프에 따른 비용 증가 | 실패 루프, 재시도 | [입증](../03-prove/) |
| 기록 누적 | 기록 증가, 긴 문맥에서의 성능 저하 | [개선: 컨텍스트 압축](../04-improve/#compaction-without-losing-the-cache) |
| 캐시 접두부 규칙 | 접두부 변경, 캐시 미적중 | [진단](../01-diagnose/) |
| 출력 요금, 추론 | 장황한 출력, 과도한 추론 강도 | [개선](../04-improve/) |

## 적용 예시

합성 로그 [`sample-requests.jsonl`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-requests.jsonl)에서 세션 `s1`의 첫 요청은 입력 토큰 9,200개를 사용합니다. 도구 정의 네 개를 합치면 220 + 180 + 900 + 1,100 = **2,400토큰(해당 요청의 약 26%)**이며, 그중 두 도구는 한 번도 호출되지 않습니다.

두 번째 요청부터는 입력 토큰 9,600개 중 9,000개를 캐시에서 처리합니다. *접두부가 동일하게 유지되었기 때문*입니다. 세션 도중 도구를 제거하거나 지침 순서를 바꾸면 적중률이 떨어집니다. 이 로그에서 접두부의 고정 비용, 도구 정의 비용, 캐싱이라는 세 가지 개념을 확인할 수 있습니다.

## 다음 단계

[1. 진단](../01-diagnose/)에서 측정을 시작하세요.

## 추가 자료(논문)

학회 등급과 인용 수 순으로 정렬했습니다. 논문의 결과는 가설로 보고 자신의 워크로드에서 검증하세요. 전체 목록은 [논문](../../papers/)에서 볼 수 있습니다.

- [ReAct: Synergizing Reasoning and Acting in Language Models](https://arxiv.org/abs/2210.03629) — ICLR 2022, 인용 13,383회. 추론과 도구 사용을 번갈아 수행하는 방식으로, 이 가이드에서 비용을 분석하는 에이전트 루프에 해당합니다.
- [Efficient Memory Management for Large Language Model Serving with PagedAttention](https://arxiv.org/abs/2309.06180) — SOSP 2023, 인용 9,724회. PagedAttention으로 KV 캐시 메모리를 관리해 서빙 처리량을 높이며, vLLM의 기반입니다.
- [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172) — TACL 2023, 인용 5,818회. 관련 정보가 긴 입력의 중간에 있으면 정확도가 떨어집니다.
- [SGLang: Efficient Execution of Structured Language Model Programs](https://arxiv.org/abs/2312.07104) — NeurIPS 2023, 인용 1,783회. RadixAttention으로 접두부를 공유하는 호출 간에 KV 캐시를 재사용하며, 접두부 캐싱의 토대입니다.
- [RULER: What's the Real Context Size of Your Long-Context Language Models?](https://arxiv.org/abs/2404.06654) — 프리프린트 2024, 인용 1,580회. 실제로 유효한 컨텍스트 길이가 공시된 윈도우보다 짧은 경우가 많음을 보여 주는 벤치마크입니다.

## 출처

- [openai/tiktoken — OpenAI 모델용 BPE 토크나이저](https://github.com/openai/tiktoken), [`examples/count_tokens.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py)
- [Azure OpenAI 프롬프트 캐싱](https://learn.microsoft.com/ko-kr/azure/ai-foundry/openai/how-to/prompt-caching), [Azure OpenAI 요금](https://azure.microsoft.com/ko-kr/pricing/details/cognitive-services/openai-service/)
- [OpenAI](https://developers.openai.com/api/docs/guides/prompt-caching), [Anthropic Claude](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [Google Gemini](https://ai.google.dev/gemini-api/docs/caching)의 프롬프트 캐싱 안내
- [TokenEconomizer](https://github.com/microsoft/TokenEconomizer): 지침, 활성화된 도구, 대화 기록을 요청마다 함께 전송하면서 발생하는 비용을 설명합니다.
