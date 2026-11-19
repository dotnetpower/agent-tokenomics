---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: 언어와 토큰화
description: 같은 문장도 언어에 따라 토큰 수가 달라지는 이유와 대응 방법을 설명합니다.
track: copilot
verified: 2026-11-19
sidebar:
  order: 7
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

언어별 비용은 실제 모델의 토크나이저와 작업에 따라 다릅니다. 문장 하나나 대체 인코딩의 결과를 보편적인 언어 배수로 사용할 수는 없습니다.

## 직접 측정합니다

기존 로컬 예시를 사용합니다.

```bash
python3 examples/count_tokens.py
python3 examples/formula_prompt.py --lang ko
```

스크립트 리비전, 토크나이저 패키지·버전, 인코딩, 정확한 입력, 개수를 기록합니다. tiktoken 인코딩이 Copilot의 모든 모델을 재현하는 것은 아닙니다. 가능하면 실제 모델 사용량이나 지원되는 제공자 계수 도구를 사용하고 근삿값을 정확한 값으로 부르지 않습니다.

## 조치

- 복잡한 요구 사항은 의미를 잘 보존하는 언어로 씁니다. 영어 지침은 시험할 선택지이지 필수가 아닙니다.
- 코드 식별자와 제품·설정의 공식 이름을 유지합니다. 주석·문서는 저장소의 현지화 규칙을 따릅니다.
- 언어를 바꾸는 비교에는 번역 노력, 재시도, 답변 품질, 실제 크레딧을 포함합니다.
- 화면의 프롬프트가 짧아졌다고 내부 추론·과금 절감까지 추정하지 않습니다.

<a id="hybrid-korean-bpe-formula-benchmark"></a>
### 한국어 핵심어 보존 + BPE 수식 비교

스크립트는 고정된 입력의 텍스트 압축을 보여 줍니다. 기존 설명은 비교 가능한 평가 기록 없이 47개 제약 평가와 44/54 결과를 함께 제시했습니다. 해당 모델 정확도 비율은 제거하며 올바른 분모를 임의로 만들어 대체하지 않습니다.

한국어 동작, 부정, 수량, 예외, 응답 언어 조건을 보존합니다. 두 표현을 같은 루브릭과 실제 작업 완수 결과로 비교합니다.

<a id="hugging-face-customer-support-and-counseling-benchmark"></a>
### Hugging Face 고객 지원·상담 비교

[한국어 스크립트](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_ko.py)와 [테스트](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/test_formula_prompt.py)는 인식한 슬롯과 토큰 감소를 확인합니다. 슬롯을 복구하거나 원문으로 되돌릴 수도 있으므로 **모델 이해도 검사가 아닙니다.** 재현 시 데이터셋·리비전·행을 기록하고 슬롯 보존을 의미 정확도로 일반화하지 않습니다. 이번 수정에서 새로운 모델 품질·실시간 데이터셋 벤치마크를 수행했다고 주장하지 않습니다.

## 확인 방법

작업, 모델·강도, 합격 검사, 캐시 조건을 비교 가능하게 유지합니다. 모국어·영어·간결한 모국어 프롬프트를 [기준선 기록](../02-measure-usage/)으로 비교합니다. 실패도 포함하며 사용량 비공개 항목은 **확인 불가**로 기록합니다.

## 절충점

영어로 바꾸다 요구 사항을 오해하면 명확한 한국어보다 비쌀 수 있습니다. 토큰 효율이 명확성·접근성·저장소 언어 규칙보다 우선하지 않습니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 각 연구 결과는 가설이므로 사용자의 워크로드에서 검증해야 합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [Neural Machine Translation of Rare Words with Subword Units](https://arxiv.org/abs/1508.07909) - ACL 2015, 인용 9,243회. 대부분의 LLM 토크나이저의 기반이 되는 서브워드(BPE) 단위를 제안했습니다.
- [Language Model Tokenizers Introduce Unfairness Between Languages](https://arxiv.org/abs/2305.15425) - NeurIPS 2023, 인용 319회. 같은 텍스트라도 언어에 따라 토큰 길이가 크게 달라짐을 보여 줍니다.
- [Do All Languages Cost the Same? Tokenization in the Era of Commercial Language Models](https://arxiv.org/abs/2305.13707) - EMNLP 2023, 인용 278회. 토큰화 차이 때문에 상용 API 사용 비용이 언어마다 다르다는 점을 보여 줍니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [github.com: tiktoken](https://github.com/openai/tiktoken)
- [code.visualstudio.com: models](https://code.visualstudio.com/docs/agent-customization/language-models)
- [docs.github.com: rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)
