---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: 프롬프트와 출력 스타일
description: 간결한 프롬프트, 구조화된 요청, 출력 제한으로 입력과 출력 비용을 줄이는 방법을 설명합니다.
track: copilot
verified: 2026-11-19
sidebar:
  order: 6
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

화면의 답변이 짧아졌다고 내부 추론 사용량이나 전체 세션 비용이 줄었다고 단정할 수는 없습니다. 불필요한 설명을 줄이되 요구 사항·검증·실패 보고는 보존합니다.

## 조치

### 1. 출력 길이를 제한합니다

예를 들어 “수정 요약, 실제 실행한 명령, 테스트 개수, 남은 차단 사유를 다섯 항목 이내로 보고합니다”처럼 작업에 맞게 지정합니다. 이는 프롬프트 지시이지 API의 강제 토큰 제한이 아닙니다.

작은 변환에 “코드만 출력”은 적합할 수 있지만 테스트 실패나 안전 경고를 숨겨서는 안 됩니다. 채팅에서 JSON을 요청한다고 제공자가 스키마를 강제하는 것은 아닙니다. [출력·캐시 단가는 모델마다 다르며](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing) [추론 제어](https://code.visualstudio.com/docs/agent-customization/language-models)는 설명 길이와 별개입니다.

### 2. 프롬프트를 다듬습니다

목표, 기준 입력, 제약, 합격 검사를 명시합니다. 같은 결과를 위한 구현·테스트·직접 관련 문서는 하나의 일관된 작업으로 유지할 수 있습니다. 무관한 목표를 나누되 완료에 필요한 조건까지 분리하지 않습니다.

### 3. 간결한 지침을 사용합니다

새로 만든 약어보다 명확한 목록을 우선합니다. 압축하면 부정·예외·의존 조건이 빠질 수 있으므로 문자열 길이뿐 아니라 해석도 평가합니다.

<a id="4-bpe-aligned-formula-notation"></a>
### 4. BPE 친화적 수식 표기법

로컬 [수식 측정 스크립트](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_en.py)는 지정된 tiktoken 인코딩에서 산문·수식 표현을 비교합니다.

```bash
python3 examples/formula_prompt.py --lang en
```

토큰 수는 **인코딩별 텍스트 측정값**으로 해석합니다. ASCII 연산자가 모든 위치·모델에서 항상 한 토큰인 것은 아닙니다. 문자열·슬롯 보존은 의미 동등성이나 에이전트 작업 성공을 증명하지 않습니다. 측정값을 Copilot 크레딧 절감률 보장으로 바꾸지 않습니다.

## 확인 방법

원문·간결한 프롬프트로 같은 작업을 실행합니다. 합격 조건을 모두 유지하고 [기준선 표](../02-measure-usage/)로 화면 출력, 공개되는 추론 포함 출력·추론 내역, 재시도, 크레딧, 테스트를 비교합니다.

## 절충점

설명은 학습·검토에 필요합니다. 불필요한 서술을 줄이되 증거를 줄이지 않습니다. 원문 링크가 없는 기존 발표 자료의 절감 수치는 근거로 사용하지 않으며, 인코딩 실험만으로 종단 간 품질을 입증할 수는 없습니다.

## 추가 자료(논문)

학회 등급, 인용 수 순으로 정렬했습니다. 각 연구 결과는 가설이므로 자신의 워크로드에서 검증해야 합니다. 전체 목록은 [논문](../../papers/)을 참고하세요.

- [LongLLMLingua: Accelerating and Enhancing LLMs in Long Context Scenarios via Prompt Compression](https://arxiv.org/abs/2310.06839) - ACL 2023, 인용 658회. 긴 컨텍스트를 질문 기반으로 압축해, 정확도를 유지하거나 높이면서 비용을 줄이는 것을 목표로 합니다.
- [LLMLingua-2: Data Distillation for Efficient and Faithful Task-Agnostic Prompt Compression](https://arxiv.org/abs/2403.12968) - ACL 2024, 인용 487회. 더 빠르고 작업에 구애받지 않는 프롬프트 압축기를 증류하여, 여러 작업에 일반화합니다.
- [LLMLingua: Compressing Prompts for Accelerated Inference of Large Language Models](https://arxiv.org/abs/2310.05736) - EMNLP 2023, 인용 418회. 작은 언어 모델로 정보량이 낮은 토큰을 프롬프트에서 제거하며, 예산을 조절할 수 있습니다.
- [Compressing Context to Enhance Inference Efficiency of Large Language Models](https://arxiv.org/abs/2310.06201) - EMNLP 2023, 인용 272회. 자기 정보량(self-information)을 기준으로 중복된 어휘 단위를 제거해, 추론 전에 입력을 줄입니다.
- [Let Me Speak Freely? A Study On The Impact Of Format Restrictions On Large Language Model Performance.](https://arxiv.org/abs/2408.02442) - EMNLP 2024, 인용 142회. 출력 형식을 엄격하게 제한하면 추론 성능이 떨어질 수 있음을 보여 줍니다.
- [Chain of Draft: Thinking Faster by Writing Less](https://arxiv.org/abs/2502.18600) - 프리프린트 2025, 인용 324회. 장황한 chain-of-thought 대신 최소한의 초안 단계만 쓰도록 모델에 유도합니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [docs.github.com: rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)
- [code.visualstudio.com: models](https://code.visualstudio.com/docs/agent-customization/language-models)
- [github.com: tiktoken](https://github.com/openai/tiktoken)
