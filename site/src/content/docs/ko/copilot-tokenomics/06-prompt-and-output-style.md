---
title: 프롬프트와 출력 스타일
description: 더 짧은 프롬프트, 구조화된 요청, 제한된 출력 등 요청의 양쪽에서 쓸 수 있는 절감 방법을 설명합니다.
track: copilot
verified: 2026-11-03
sidebar:
  order: 6
---

## 문제

비용은 보낸 토큰과 모델이 작성한 토큰 **모두**에 발생합니다. 프롬프트에 섞인 군더더기는 크지 않지만, 모든 *답변*에 섞인 군더더기는 요청마다 반복됩니다. 대부분의 모델에서 출력 토큰은 입력 토큰보다 단가도 높습니다. 따라서 응답 길이를 조절하는 것이 들인 노력 대비 가장 효과가 큰 방법인 경우가 많습니다. 모델별 요금은 GitHub Docs의 모델 가격 정보에서 확인하세요. 입력과 출력의 요금 비율은 모델마다 다릅니다.

## 조치

### 1. 출력을 제한합니다(효과가 가장 큼)

`copilot-instructions.md`에 상시 규칙을 추가하거나, 프롬프트 끝에 형식 제한을 덧붙입니다.

| 지시문 | 효과 |
|-------------|--------|
| `Code only, no explanation.` | 변경 내용 주변의 설명 문장이 사라집니다. 이유가 필요하면 "왜"를 명시적으로 물어봅니다. |
| `Answer in one sentence.` | 장황한 답변을 막습니다. |
| `3 bullet points max.` | 항목 수를 강제로 제한합니다. |
| `Yes or no, then one line why.` | 쓸 만한 답변 중 가장 짧은 형태입니다. |
| `Reply as JSON.` / `Table format.` | 서술 없이 구조화된 출력만 받습니다. |

*프롬프트*가 간결하다고 *답변*도 간결해지지는 않습니다. 둘 다 지정합니다.

### 2. 프롬프트를 다듬습니다

- 공손한 표현, 완곡한 표현, 군더더기(`please`, `I think maybe`, `basically`)를 뺍니다. 기술 용어, 코드, 구체적인 내용은 그대로 둡니다.
- 줄글보다 **구조화된** 입력(글머리 기호, `key: value`)을 사용합니다.
- 절차보다 **선언형** 규칙을 사용합니다. "먼저 파일을 읽고, 그다음 …를 찾아서" 같은 단계별 설명 대신 `All exported functions: JSDoc required.`처럼 씁니다.
- 가장 짧고 정확한 형태는 코드에 가까운 경우가 많습니다. 타입 시그니처, 의사 코드, 또는 "`getUserById`와 비슷하게, 이메일로 조회. 없으면 404 반환" 같은 식입니다.
- 일반적인 약어(`DB`, `auth`, `config`, `env`, `repo`)는 모델이 이미 아는 경우에만 사용합니다. 프로젝트 고유의 약어는 먼저 지침에 정의합니다.
- **프롬프트 하나에는 작업 하나만 담습니다.** 리팩터링, 테스트, 문서, 보안을 한 프롬프트에 섞으면 각 부분이 불완전하게 처리되기 쉽습니다. 작업을 나누고 단계마다 검토하여, 실패한 단계만 다시 수행합니다.

### 3. 간결한 스타일 지침

커뮤니티의 "caveman" 스타일 스킬은 *관사와 군더더기 생략, 단편적 문장 허용, 코드는 그대로 유지*와 같은 짧은 규칙으로 입력과 출력을 모두 압축합니다. [caveman](https://github.com/JuliusBrussee/caveman)과 [측정 평가](https://github.com/danielle34/cavewoman)를 참고하세요. 압축 수준이 높을수록 토큰은 줄지만 명확성이 떨어집니다. 복잡한 지시가 모호해질 수 있으므로, 고객 대상 문서나 온보딩 문서에는 가볍게만 적용합니다.

<a id="4-bpe-aligned-formula-notation"></a>
### 4. BPE 친화적 수식 표기법

도구 분기 조건이나 다단계 워크플로를 적을 때 **BPE 친화적 ASCII 수식**을 쓰면 줄글보다 짧고 모호성도 줄어듭니다. 다만 **유니코드 수학 기호(`∀`, `∈`, `∪`, `≻`, `∘`)는 오히려 함정**입니다. `o200k_base`와 `cl100k_base`에서 이러한 다중 바이트 기호는 2~3토큰으로 쪼개질 뿐 아니라, 뒤에 오는 영단어의 선행 공백 병합까지 깨뜨려 바이트가 절반으로 줄어도 토큰 수는 거의 줄지 않습니다. 반면 공백을 둔 ASCII 연산자(` ->`, ` =>`, ` >`, ` <=`, ` >=`, ` &`, ` |`)는 선행 공백을 포함해 각각 **단일 1토큰**으로 병합됩니다.

[`examples/formula_prompt_en.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_en.py)(또는 `python3 examples/formula_prompt.py --lang en`)로 측정한 결과입니다(`tiktoken` 0.14.0, 2026-11-04 확인).

| 표현 방식 | 텍스트 | 바이트 | `o200k_base` | 줄글 대비 | `cl100k_base` |
|-------|------|------:|-------------:|----------:|--------------:|
| 줄글 (백틱 포함) | ``When you need to find a symbol or literal and read its surrounding lines, use `code_context` in one call instead of searching first and reading later.`` | 150 | 31 | 0.0% | 31 |
| 유니코드 수학 수식 (붙여쓰기) | `∀x∈Sym∪Lit: need(x+ctx) ⇒ code_context(x) ≻ (grep(x) ∘ read(x))` | 75 | 30 | -3.2% | 31 |
| ASCII 수식 (붙여쓰기) | `δ(symbol\|literal+surrounding_lines)=>code_context(root,pattern,radius<=20)` | 75 | 20 | -35.5% | 20 |
| **BPE 정렬 수식 (띄어쓰기)** | `symbol + context => code_context (1 call > grep + read)` | **55** | **14** | **-54.8%** | **14** |

BPE 정렬 수식 작성 원칙은 다음과 같습니다.
- 유니코드 수학 기호(`∀`, `∈`, `≻`, `∘`) 대신 **앞에 공백을 둔 ASCII 연산자**(` ->`, ` =>`, ` >=`, ` <=`, ` !=`, ` &`, ` |`)를 사용합니다.
- 모든 식별자를 인라인 백틱(`` `name` ``)으로 감싸지 않습니다(식별자마다 양쪽 백틱에 2토큰씩 추가 소모됩니다).
- 연산자 앞뒤에 공백을 두어 `" 단어"`가 하나의 BPE 토큰으로 묶이게 합니다.

Hugging Face 공개 데이터셋의 영문 고객지원 SOP 및 실시간 라이브챗 안내문 7종(`examples/formula_prompt_en.py`)에 이 규칙을 적용하면 필수 슬롯과 플레이스홀더를 **43/43 (100%)** 보존하면서 토큰 수를 **`o200k_base` 기준 75.9%** 줄일 수 있습니다(한국어 전용 채팅·상담 예제는 `examples/formula_prompt_ko.py` 및 [언어와 토큰화](../07-language-and-tokenization/#hugging-face-customer-support-and-counseling-benchmark)를 참고하세요).

| 영문 고객지원 SOP·채팅 사례 및 Hugging Face 데이터셋 (`row`) | 원문 (`o200k` / `cl100k`) | BPE 수식 (`o200k` / `cl100k`) | `o200k_base` 변화 | `cl100k_base` 변화 | 핵심 슬롯 보존 |
|--------------------------------------------------------------|--------------------------:|------------------------------:|------------------:|-------------------:|---------------:|
| 주문 취소 절차 안내 ([`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) `train[2]`) | 223 / 226 | 55 / 55 | **-75.3%** | **-75.7%** | 9/9 |
| 주문 변경 절차 안내 ([`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) `train[1001]`) | 225 / 230 | 54 / 54 | **-76.0%** | **-76.5%** | 9/9 |
| 위약금·취소 수수료 조회 ([`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) `train[3001]`) | 98 / 100 | 26 / 27 | **-73.5%** | **-73.0%** | 5/5 |
| 환불 소요 기간 및 주문번호 조회 ([`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) `train[6000]`) | 221 / 225 | 34 / 34 | **-84.6%** | **-84.9%** | 5/5 |
| 리콜 모니터 반품·환불 안내 ([`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[1]`) | 131 / 131 | 42 / 42 | **-67.9%** | **-67.9%** | 6/6 |
| 로봇 암 보정 기술지원 채팅 ([`FunDialogues/customer-service-robot-support`](https://huggingface.co/datasets/FunDialogues/customer-service-robot-support) `train[0]`) | 60 / 61 | 18 / 18 | **-70.0%** | **-70.5%** | 5/5 |
| 배송 지연 실시간 라이브챗 응답 ([`Kaludi/Customer-Support-Responses`](https://huggingface.co/datasets/Kaludi/Customer-Support-Responses) `train[0]`) | 28 / 29 | 9 / 9 | **-67.9%** | **-69.0%** | 4/4 |
| **합계 (7개 영문 고객지원 SOP·채팅 사례)** | **986 / 1,002** | **238 / 239** | **-75.9%** | **-76.1%** | **43/43 (100%)** |

## 확인 방법

가정하지 말고 측정합니다. `o200k_base` 인코딩으로 [토큰 카운터](../07-language-and-tokenization/)를 실행하면, 예시 프롬프트 하나가 38토큰에서 11토큰으로 줄어듭니다.

```text
Hey, could you please help me refactor this function? I think it might have some issues with how it handles
the authentication, and I'd really like it to be more efficient. Thanks!          -> 38 tokens
Refactor function. Fix auth handling. Make efficient.                              -> 11 tokens
```

그런 다음 실제 작업 하나에서 적용 전후의 **세션 크레딧**을 비교하고, 출력이 여전히 수락 기준을 충족하는지 확인합니다.

## 트레이드오프

- 학습이나 디버깅에는 설명이 필요합니다. 필요할 때 요청합니다(`Explain why this beats X`).
- 지나치게 압축한 프롬프트는 제약 조건을 잃습니다. 정확한 기술 용어와 구체적인 내용은 유지합니다.
- 이 문서의 바탕이 된 발표 자료는 이러한 지침으로 출력이 크게 줄었다고 보고합니다(수십 퍼센트이며, 예/아니오 형식에서는 더 큽니다). 이 수치는 독립적으로 재현하지 못했습니다. 방향성만 참고하고, 직접 작업에서 측정하세요.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 각 연구 결과는 가설이므로 자신의 워크로드에서 검증해야 합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [LongLLMLingua: Accelerating and Enhancing LLMs in Long Context Scenarios via Prompt Compression](https://arxiv.org/abs/2310.06839) — ACL 2023, 인용 658회. 긴 컨텍스트를 질문 기반으로 압축해, 정확도를 유지하거나 높이면서 비용을 줄이는 것을 목표로 합니다.
- [LLMLingua-2: Data Distillation for Efficient and Faithful Task-Agnostic Prompt Compression](https://arxiv.org/abs/2403.12968) — ACL 2024, 인용 487회. 더 빠르고 작업에 구애받지 않는 프롬프트 압축기를 증류하여, 여러 작업에 일반화합니다.
- [LLMLingua: Compressing Prompts for Accelerated Inference of Large Language Models](https://arxiv.org/abs/2310.05736) — EMNLP 2023, 인용 418회. 작은 언어 모델로 정보량이 낮은 토큰을 프롬프트에서 제거하며, 예산을 조절할 수 있습니다.
- [Compressing Context to Enhance Inference Efficiency of Large Language Models](https://arxiv.org/abs/2310.06201) — EMNLP 2023, 인용 272회. 자기 정보량(self-information)을 기준으로 중복된 어휘 단위를 제거해, 추론 전에 입력을 줄입니다.
- [Let Me Speak Freely? A Study On The Impact Of Format Restrictions On Large Language Model Performance.](https://arxiv.org/abs/2408.02442) — EMNLP 2024, 인용 142회. 출력 형식을 엄격하게 제한하면 추론 성능이 떨어질 수 있음을 보여 줍니다.
- [Chain of Draft: Thinking Faster by Writing Less](https://arxiv.org/abs/2502.18600) — 프리프린트 2025, 인용 324회. 장황한 chain-of-thought 대신 최소한의 초안 단계만 쓰도록 모델에 유도합니다.

## 출처

- Token Efficiency and Context Engineering — 개발자 발표 자료(2026년 6월), 프롬프트 압축 및 출력 제어 부분(수치는 보고된 그대로이며 독립적으로 검증하지 않음)
- [Optimize AI credit usage in VS Code](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [`examples/formula_prompt_en.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_en.py), [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) — 재현 가능한 영문 BPE 수식 토큰 측정 및 프롬프트 변환 스크립트
- [caveman](https://github.com/JuliusBrussee/caveman), [cavewoman](https://github.com/danielle34/cavewoman)
