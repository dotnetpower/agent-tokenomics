---
title: 언어와 토큰화
description: 같은 문장도 언어에 따라 토큰 수가 달라지는 이유와 대응 방법을 설명합니다.
track: copilot
verified: 2026-11-03
sidebar:
  order: 7
---

## 문제

"프롬프트를 모국어로 쓰는 편이 저렴할까요, 아니면 번역해서 보내는 편이 저렴할까요?" 토큰 수는 토크나이저에 따라 달라지는데, 토크나이저는 대부분 영어 텍스트로 학습됩니다. 같은 의미를 표현하는 데 보통 영어가 아닌 언어에서 **더 많은 토큰**이 필요하므로, "모국어가 더 저렴하다"는 결론은 거의 나오지 않습니다.

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

여기서 알 수 있는 점은 두 가지입니다.

1. **문자 수가 적다고 토큰 수가 적은 것은 아닙니다.** 중국어, 일본어, 한국어는 문자 수가 적지만 문자 하나가 토큰 하나 이상을 차지할 수 있습니다.
2. **차이는 토크나이저에 따라 달라집니다.** 새 어휘(`o200k_base`)는 이전 어휘(`cl100k_base`)보다 차이가 크게 줄어듭니다. Copilot은 여러 공급자로 요청을 라우팅할 수 있으므로, 사용하는 모델과 가까운 인코딩으로 스크립트를 실행하거나 해당 공급자의 토큰 계산 API를 사용해 정확한 수치를 확인합니다.

## 조치

- **지침 파일과 상시 규칙은 영어로 작성합니다.** 이 내용은 모든 요청에 전송되므로, 여기서 발생하는 지속적인 배수 차이가 가장 큰 영향을 줍니다.
- **토큰을 아끼려고 프롬프트를 번역하지 않습니다.** 직접 측정한 경우가 아니라면 그렇습니다. 번역 비용이 절감액보다 클 수 있고 의미가 바뀔 수도 있습니다.
- 채팅에 사용하는 언어와 관계없이 **코드, 식별자, 주석, 커밋 메시지는 영어로 유지합니다**.
- 비라틴 문자는 라틴 문자로 음역하면 토큰이 줄어들 수 있지만, 가독성이 떨어집니다. 도입하기 전에 측정합니다.
- 가장 읽기 편한 언어로 *답변*하도록 요청하는 것은 꼭 필요할 때만 합니다. 답변은 출력 토큰이며 단가가 더 높습니다.

## 트레이드오프

명확성과 정확성이 작은 토큰 절감보다 중요합니다. 한국어(또는 다른 언어)로 생각할 때 더 정확하다면, 복잡한 요구 사항은 그 언어로 작성합니다. 지침이나 도구 설명처럼 *상시* 전송되는 파일만 영어로 유지합니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 각 연구 결과는 가설이므로 자신의 워크로드에서 검증해야 합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [Neural Machine Translation of Rare Words with Subword Units](https://arxiv.org/abs/1508.07909) — ACL 2015, 인용 9,243회. 대부분의 LLM 토크나이저의 기반이 되는 서브워드(BPE) 단위를 제안했습니다.
- [Language Model Tokenizers Introduce Unfairness Between Languages](https://arxiv.org/abs/2305.15425) — NeurIPS 2023, 인용 319회. 같은 텍스트라도 언어에 따라 토큰 길이가 크게 달라짐을 보여 줍니다.
- [Do All Languages Cost the Same? Tokenization in the Era of Commercial Language Models](https://arxiv.org/abs/2305.13707) — EMNLP 2023, 인용 278회. 토큰화 차이 때문에 상용 API 사용 비용이 언어마다 다르다는 점을 보여 줍니다.

## 출처

- [`examples/count_tokens.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py) — 재현 가능한 측정
- [tiktoken](https://github.com/openai/tiktoken)
- Token Efficiency and Context Engineering — 개발자 발표 자료(2026년 6월). 다른 토크나이저를 사용했지만 같은 방향(영어 외 텍스트에 더 많은 토큰이 듦)을 보고합니다. 이 문서의 표는 직접 측정한 결과입니다.
