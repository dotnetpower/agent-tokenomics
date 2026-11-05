---
title: 언어와 토큰화
description: 같은 문장도 언어에 따라 토큰 수가 달라지는 이유와 대응 방법을 설명합니다.
track: copilot
verified: 2026-11-03
sidebar:
  order: 7
---

## 문제

"프롬프트를 모국어로 쓰는 편이 저렴할까요, 아니면 번역해서 보내는 편이 저렴할까요?" 토큰 수는 토크나이저에 따라 달라지는데, 토크나이저는 대부분 영어 텍스트로 학습됩니다. 같은 의미를 표현하더라도 영어가 아닌 언어에 **더 많은 토큰**이 필요한 경우가 많습니다. 따라서 언어를 선택하기 전에 사용하는 모델로 직접 측정하는 편이 좋습니다.

## 직접 측정하기

[`examples/count_tokens.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py)는 한 문장("I met a huge dog")을 여러 언어로 바꿔 [tiktoken](https://github.com/openai/tiktoken)으로 토큰 수를 셉니다. 측정 결과는 다음과 같습니다(tiktoken 0.14.0, 2026-11-03).

| 언어 | 문자 수 | o200k_base 토큰 | × 영어 | cl100k_base 토큰 | × 영어 |
|----------|-----------:|------------------:|----------:|-------------------:|----------:|
| English | 16 | 5 | 1.0 | 5 | 1.0 |
| Spanish | 24 | 7 | 1.4 | 8 | 1.6 |
| Korean | 13 | 8 | 1.6 | 13 | 2.6 |
| Chinese | 8 | 8 | 1.6 | 12 | 2.4 |
| Japanese | 9 | 8 | 1.6 | 11 | 2.2 |
| Russian | 26 | 7 | 1.4 | 14 | 2.8 |
| Hebrew | 13 | 7 | 1.4 | 16 | 3.2 |

이 예제에서 확인할 수 있는 점은 두 가지입니다.

1. **문자 수가 적다고 토큰 수가 적은 것은 아닙니다.** 중국어, 일본어, 한국어는 문자 수가 적지만 문자 하나가 토큰 하나 이상을 차지할 수 있습니다.
2. **차이는 토크나이저에 따라 달라집니다.** 새 어휘(`o200k_base`)를 사용하면 이전 어휘(`cl100k_base`)보다 차이가 크게 줄어듭니다. Copilot은 여러 공급자로 요청을 라우팅할 수 있으므로, 사용하는 모델과 가까운 인코딩으로 스크립트를 실행하거나 해당 공급자의 토큰 계산 API를 사용해 정확한 수치를 확인합니다.

## 조치

- **지침 파일과 상시 규칙은 영어로 작성합니다.** 이 내용은 모든 요청에 전송되므로, 여기서 발생하는 지속적인 배수 차이가 가장 큰 영향을 줍니다.
- **토큰을 아끼려고 프롬프트를 번역하지 않습니다.** 번역 비용이 절감액보다 클 수 있고 의미가 바뀔 수도 있습니다. 먼저 직접 측정하세요.
- **한국어 요청·답변 워크플로에서는 한국어 핵심 명사·동사를 유지하고 연결 어미·조사만 BPE 수식 연산자로 바꿉니다.**
  한국어 프롬프트에서 토큰을 많이 쓰는 부분은 도메인 명사가 아니라 서술어 어미와 보조용언(`~을 완료할수 있도록`, `~에 대한 계획을 세워서`, `~한 다음`)입니다. 핵심 한글 어근(`남은작업 계획수립`, `보강점 >=100 발굴`, `모두조치`, `수치표 보고`)은 그대로 두고 연결부만 공백을 둔 ASCII 수식 연산자(` ->`, ` =>`, ` >=`, ` <=`, ` &`)와 `[ko]` 응답 언어 태그로 치환하면, 의미 왜곡 없이 입력 토큰을 **`o200k_base` 기준 22~39%**(**`cl100k_base` 기준 31~49%**) 줄이면서 한국어 답변을 100% 유지할 수 있습니다.
- 채팅에 사용하는 언어와 관계없이 **코드, 식별자, 주석, 커밋 메시지는 영어로 유지합니다**.
- 비라틴 문자는 라틴 문자로 음역하면 토큰이 줄어들 수 있지만, 가독성이 떨어집니다. 도입하기 전에 측정합니다.
- 가장 읽기 편한 언어로 *답변*하도록 요청하는 것은 꼭 필요할 때만 합니다. 답변은 출력 토큰이며 단가가 더 높습니다.

<a id="hybrid-korean--bpe-formula-benchmark"></a>
### 한국어 핵심어 보존 + BPE 수식 실측 비교

[`examples/formula_prompt_ko.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_ko.py)(또는 `python3 examples/formula_prompt.py --lang ko`)로 측정한 결과입니다(`tiktoken` 0.14.0, 2026-11-04 확인).

| 작업 유형 | 한국어 원문 (`o200k` / `cl100k`) | 한국어 핵심어 + BPE 수식 (`[ko] :: ...`) | `o200k_base` 변화 | `cl100k_base` 변화 |
|------------------|----------------------------------:|---------------------------------------------|-------------------:|--------------------:|
| 이슈 완결 파이프라인 | 44 / 72 | `[ko] :: #1945 남은작업 계획수립 -> 모두조치 -> push & merge -> 이슈닫기` | **27 (-38.6%)** | **37 (-48.6%)** |
| 대량 발굴 및 반복 보강 | 46 / 74 | `[ko] :: 보강점 >=100 발굴 -> 모두조치 -> loop 10x -> 수치표 보고 -> commit & push & merge` | **31 (-32.6%)** | **38 (-48.6%)** |
| 조건부 하드닝 루프 | 54 / 91 | `[ko] :: 모두적용 -> loop(비평 >=10 하드닝, until 심각도 <= Low) -> 개선점 >=100 발굴 -> 모두보강 -> push & merge -> 보고` | **42 (-22.2%)** | **57 (-37.4%)** |
| 최적화 발굴 및 목록화 | 31 / 51 | `[ko] :: 절감액 기반 최적화 >=30 발굴 -> 계획수립 -> 목록 보고` | **22 (-29.0%)** | **35 (-31.4%)** |
| **합계 (4개 프롬프트)** | **175 / 288** | — | **122 (-30.3%)** | **167 (-42.0%)** |

한국어 명사까지 억지로 영문 약어로 바꾸면 어떤 문제가 생길까요? 실제 한국어 Copilot 요청 8종(세부 제약 47개 항목)으로 검증한 결과, `"보강점 100개 이상 찾아서"`를 단순히 영문 `gaps >=100`으로 치환하면 모델이 이를 **"100개 이상 발굴하라(명령)"**가 아니라 **"갭이 100개 이상인지 확인하라(조건문)"**로 오독하여 제약 준수율이 **81.5% (44/54)**로 떨어졌습니다. 반면 한국어 핵심 동작 명사(`보강점 >=100 발굴`, `남은작업 계획수립`)를 보존하고 연산자만 수식화했을 때는 **제약 준수율 100.0% (47/47)**와 **한국어 응답 100%**를 달성했습니다.

**한국어 핵심 명사 앞에는 공백을 두고 불필요한 따옴표·괄호를 제거합니다.** `o200k_base` 어휘 사전에는 `" 고객"`, `" 담당"`, `" 주문"`, `" 완료"`, `" 직접"`, `" 처리"`, `" 확인"`, `" 전달"`, `" 안내"`, `" 기준"` 등을 포함해 **앞에 공백이 있을 때만 단일 1토큰으로 병합되는 2음절 이상 한국어 단어가 643개** 있습니다. 공백 없이 붙여 쓰거나(`직접처리` 3토큰 vs `직접 처리` 2토큰, `주문완료` 3토큰 vs `주문 완료` 2토큰) 따옴표·낫표(`'무선 이어폰'`, `‘무선 이어폰’`, `「근로기준법」`)로 감싸면 선행 공백 병합이 깨져 오히려 토큰 수가 늘어납니다.

<a id="hugging-face-customer-support-and-counseling-benchmark"></a>
### Hugging Face 공개 상담·고객지원 데이터셋 실측 비교

이 원칙은 인사말, 사과 표현, 반복적인 약관·법령 안내문이 토큰의 상당 부분을 차지하는 한국어 고객센터·법률·노무 상담 데이터에서도 동일하게 작동합니다. [`examples/formula_prompt_ko.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_ko.py)와 [`examples/test_formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/test_formula_prompt.py)로 측정·검증한 결과입니다(`tiktoken` 0.14.0, 2026-11-04 확인. 영문 SOP 전용 예제는 `examples/formula_prompt_en.py` 및 [프롬프트와 출력 스타일](../06-prompt-and-output-style/#4-bpe-aligned-formula-notation)을 참고하세요).

| 한국어 채팅·상담 사례 및 Hugging Face 데이터셋 (`row`) | 원문 (`o200k` / `cl100k`) | BPE 수식 (`o200k` / `cl100k`) | `o200k_base` 변화 | `cl100k_base` 변화 | 핵심 슬롯 보존 |
|--------------------------------------------------------|--------------------------:|------------------------------:|------------------:|-------------------:|---------------:|
| 이커머스 배송 조회·환불/취소 약관 ([`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[5]`) | 103 / 158 | 64 / 94 | **-37.9%** | **-40.5%** | 11/11 |
| 노트북 불량 기술지원·영수증 이관 ([`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[2]`) | 90 / 147 | 45 / 73 | **-50.0%** | **-50.3%** | 8/8 |
| 이커머스 반품·교환 약관 안내 ([`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[6]`) | 101 / 173 | 59 / 99 | **-41.6%** | **-42.8%** | 9/9 |
| 배달 앱 주문 요약·결제·도착시간 이관 ([`jjun123/delivery-app-function-calling-datasets-korean`](https://huggingface.co/datasets/jjun123/delivery-app-function-calling-datasets-korean) `train[0]`) | 173 / 229 | 127 / 162 | **-26.6%** | **-29.3%** | 11/11 |
| 퇴직급여제도 설정 의무·예외 요건 ([`jihye-moon/klac_legal_aid_counseling`](https://huggingface.co/datasets/jihye-moon/klac_legal_aid_counseling) `train[1]`) | 173 / 256 | 73 / 109 | **-57.8%** | **-57.4%** | 7/7 |
| 최저임금법 제6조 및 수습 90% 특례 ([`jihye-moon/klac_legal_aid_counseling`](https://huggingface.co/datasets/jihye-moon/klac_legal_aid_counseling) `train[0]`) | 124 / 189 | 65 / 101 | **-47.6%** | **-46.6%** | 7/7 |
| 평균임금 산정 및 최저기준 원칙 ([`seventy4/labor-counsel-qa`](https://huggingface.co/datasets/seventy4/labor-counsel-qa) `train[2,3]`) | 104 / 177 | 65 / 113 | **-37.5%** | **-36.2%** | 6/6 |
| 해고 30일 전 예고 의무 및 예외 요건 ([`seventy4/labor-counsel-qa`](https://huggingface.co/datasets/seventy4/labor-counsel-qa) `train[34]`) | 90 / 142 | 51 / 74 | **-43.3%** | **-47.9%** | 7/7 |
| 무인보관함 배송지 변경 채팅 상담 ([`schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1`](https://huggingface.co/datasets/schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1) `train[2]`) | 51 / 68 | 30 / 36 | **-41.2%** | **-47.1%** | 4/4 |
| 병원 검진 슬롯 조회 채팅 안내 ([`schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1`](https://huggingface.co/datasets/schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1) `train[11]`) | 91 / 164 | 42 / 68 | **-53.8%** | **-58.5%** | 7/7 |
| **합계 (10개 한국어 채팅·상담·법률 사례)** | **1,100 / 1,703** | **621 / 929** | **-43.5%** | **-45.4%** | **77/77 (100%)** |

임의의 채팅·상담 데이터에서 **100% 슬롯 정확도와 토큰 비팽창**을 보장하기 위해, `examples/formula_prompt_ko.py`와 `examples/formula_prompt_en.py`는 압축 전 법조문(`제N조`, `제N항`, `제N호`), 대법원 사건번호(`98다46198`), 이메일, 전화번호, 케밥케이스 엔티티 ID(`parcel-610`, `locker-16`), 고객·주문·상품 ID, 수량, 금액, 날짜, 템플릿 플레이스홀더(`{{...}}`)를 전수 추출해 누락된 슬롯을 자동 복원(`ensure_slot_preservation`)하며, 압축 후보가 `o200k_base` 토큰 수를 줄이지 못하면 원문으로 폴백합니다. 다중 턴 채팅·상담 대화 이력 압축과 580건 실시간 벤치마크(`--compress-dialogue`, `--hf-live --hf-rows 50`)는 [트랙 B — 4. 개선](../../agent-tokenomics/04-improve/#compaction-without-losing-the-cache)을 참고하세요.

## 트레이드오프

명확성과 정확성이 작은 토큰 절감보다 중요합니다. 한국어(또는 다른 언어)로 생각할 때 더 정확하다면, 복잡한 요구 사항은 그 언어로 작성합니다. 지침이나 도구 설명처럼 *상시* 전송되는 파일만 영어로 유지합니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 각 연구 결과는 가설이므로 사용자의 워크로드에서 검증해야 합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [Neural Machine Translation of Rare Words with Subword Units](https://arxiv.org/abs/1508.07909) — ACL 2015, 인용 9,243회. 대부분의 LLM 토크나이저의 기반이 되는 서브워드(BPE) 단위를 제안했습니다.
- [Language Model Tokenizers Introduce Unfairness Between Languages](https://arxiv.org/abs/2305.15425) — NeurIPS 2023, 인용 319회. 같은 텍스트라도 언어에 따라 토큰 길이가 크게 달라짐을 보여 줍니다.
- [Do All Languages Cost the Same? Tokenization in the Era of Commercial Language Models](https://arxiv.org/abs/2305.13707) — EMNLP 2023, 인용 278회. 토큰화 차이 때문에 상용 API 사용 비용이 언어마다 다르다는 점을 보여 줍니다.

## 출처

- [`examples/count_tokens.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py) — 재현 가능한 측정
- [`examples/formula_prompt_ko.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_ko.py), [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) — 재현 가능한 한국어 + BPE 수식 측정 및 프롬프트 변환 스크립트
- [tiktoken](https://github.com/openai/tiktoken)
- Token Efficiency and Context Engineering — 개발자 발표 자료(2026년 6월). 다른 토크나이저를 사용했지만 같은 방향(영어 외 텍스트에 더 많은 토큰이 듦)을 보고합니다. 이 문서의 표는 직접 측정한 결과입니다.
