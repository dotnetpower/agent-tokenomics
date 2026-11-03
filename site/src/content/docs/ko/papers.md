---
title: 논문
description: 토큰 절약과 관련된 논문 75편을 학회 등급과 인용 수 순으로 정리했습니다.
sidebar:
  order: 91
---

토큰 절약과 관련된 논문 75편을 **2026-11-03** 기준으로 모아 공개된 권위 지표로 순위를 매겼습니다.
각 항목은 arXiv 페이지로 연결되며, 핵심 내용은 직접 작성한 한 줄 요약입니다.

## 순위 기준

1. **1등급**: 주요 ML/NLP/시스템 학회(ICLR, NeurIPS, ICML, ACL, EMNLP, NAACL, TACL, TMLR, SOSP, MLSys)에서 심사를 통과한 논문입니다.
2. **2등급**: 그 외 학회·저널에서 심사를 통과한 논문입니다.
3. **3등급**: 프리프린트(게재 학회 정보 없음)입니다.
4. 같은 등급 안에서는 **인용 수가 많은 순**입니다.

:::caution[순위는 읽는 순서일 뿐 평가가 아닙니다]
- 학회와 인용 수는 2026-11-03에 [Semantic Scholar](https://www.semanticscholar.org/) Graph API에서 가져온 값입니다.
  연도는 Semantic Scholar가 표시하는 연도이며 첫 프리프린트 연도일 수 있습니다.
  학회 정보는 최종 게재보다 늦게 반영될 수 있고 인용 수는 계속 변합니다.
  갱신하려면 `examples/fetch_paper_meta.py`와 `examples/build_papers.py`를 다시 실행합니다.
- 인용 수는 오래된 기초 논문에 유리합니다. 최근 프리프린트는 새로운 논문이라는 이유만으로 순위가 낮습니다.
- 심사 통과나 인용 수가 사용 중인 에이전트나 모델에서 결과가 재현된다는 뜻은 아닙니다.
  모든 결과를 가설로 취급하고 [문제 해결 절차](../agent-tokenomics/)로 직접 검증하세요.
- 수치는 옮기지 않고 주장만 요약했습니다. 수치를 인용하기 전에 실험 설정을 논문에서 확인하세요.
:::

## 주제별 적용 위치

| 주제 | 논문 수 | 적용 문서 |
|------|-------:|-----------|
| 기초 | 3 | [개념](../agent-tokenomics/00-concepts/) |
| 토큰화와 언어 | 3 | [언어와 토큰화](../copilot-tokenomics/07-language-and-tokenization/) |
| 긴 컨텍스트와 컨텍스트 열화 | 9 | [개념](../agent-tokenomics/00-concepts/), [컨텍스트를 가볍게 유지하기](../copilot-tokenomics/03-context-hygiene/), [개선](../agent-tokenomics/04-improve/) |
| 프롬프트/KV 캐싱 | 10 | [개념](../agent-tokenomics/00-concepts/), [사용량 측정](../copilot-tokenomics/02-measure-usage/), [개선](../agent-tokenomics/04-improve/) |
| 프롬프트 압축 | 10 | [프롬프트와 출력 스타일](../copilot-tokenomics/06-prompt-and-output-style/), [개선](../agent-tokenomics/04-improve/) |
| 출력 형식 | 1 | [프롬프트와 출력 스타일](../copilot-tokenomics/06-prompt-and-output-style/) |
| 배치 처리 | 1 | [지침과 도구](../copilot-tokenomics/04-instructions-and-tools/), [개선](../agent-tokenomics/04-improve/) |
| 모델 라우팅과 캐스케이드 | 5 | [모델, 계획, 위임](../copilot-tokenomics/05-models-and-planning/), [개선](../agent-tokenomics/04-improve/) |
| 효율적인 추론/출력 길이 | 6 | [에이전트 모드와 예산 보호 장치](../copilot-tokenomics/08-agent-modes-and-budget/), [개선](../agent-tokenomics/04-improve/) |
| 에이전트 비용과 컨텍스트 관리 | 9 | [격리](../agent-tokenomics/02-isolate/), [개선](../agent-tokenomics/04-improve/), [TokenEconomizer 사례 연구](../agent-tokenomics/case-tokeneconomizer/) |
| 도구 선택과 스키마 크기 | 3 | [MCP 감사](../copilot-tokenomics/09-mcp-audit/), [개선](../agent-tokenomics/04-improve/) |
| 지침/컨텍스트 파일 | 1 | [지침과 도구](../copilot-tokenomics/04-instructions-and-tools/) |
| 메모리와 요약 | 4 | [증명](../agent-tokenomics/03-prove/), [개선: 컨텍스트 압축](../agent-tokenomics/04-improve/#compaction-without-losing-the-cache) |
| 평가와 통계 | 5 | [증명](../agent-tokenomics/03-prove/), [결과](../agent-tokenomics/05-result/) |
| 서베이 | 5 | [리소스](../resources/) |

## 순위표

| # | 논문 | 학회 · 연도 | 인용 | 주제 | 핵심 내용 |
|--:|------|-------------|-----:|------|-----------|
| 1 | [Chain of Thought Prompting Elicits Reasoning in Large Language Models](https://arxiv.org/abs/2201.11903) | NeurIPS · 2022 | 23,523 | 기초 | 단계별 프롬프트는 추론 성능을 높이지만 출력 토큰도 늘립니다. |
| 2 | [ReAct: Synergizing Reasoning and Acting in Language Models](https://arxiv.org/abs/2210.03629) | ICLR · 2022 | 13,383 | 기초 | 추론과 도구 동작을 번갈아 수행하는 방식으로, 이 가이드가 비용을 분석하는 에이전트 루프에 해당합니다. |
| 3 | [Efficient Memory Management for Large Language Model Serving with PagedAttention](https://arxiv.org/abs/2309.06180) | SOSP · 2023 | 9,724 | 프롬프트/KV 캐싱 | PagedAttention으로 KV cache 메모리를 관리해 서빙 처리량을 높이며, vLLM의 기반이 되는 기술입니다. |
| 4 | [Neural Machine Translation of Rare Words with Subword Units](https://arxiv.org/abs/1508.07909) | ACL · 2015 | 9,243 | 토큰화와 언어 | 대부분의 LLM 토크나이저의 기반이 되는 서브워드(BPE) 단위를 처음 제안했습니다. |
| 5 | [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172) | TACL · 2023 | 5,818 | 긴 컨텍스트와 컨텍스트 열화 | 관련 정보가 긴 입력의 중간에 있을 때 정확도가 떨어짐을 보여 줍니다. |
| 6 | [SWE-bench: Can Language Models Resolve Real-World GitHub Issues?](https://arxiv.org/abs/2310.06770) | ICLR · 2023 | 4,683 | 기초 | TokenEconomizer를 비롯해 코딩 에이전트를 평가하는 데 쓰는, 실제 GitHub 이슈 기반 벤치마크입니다. |
| 7 | [Efficient Streaming Language Models with Attention Sinks](https://arxiv.org/abs/2309.17453) | ICLR · 2023 | 2,768 | 긴 컨텍스트와 컨텍스트 열화 | 어텐션 싱크 토큰과 최근 윈도우만 유지하여, 전체 기록 없이도 스트리밍 추론을 수행합니다. |
| 8 | [SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering](https://arxiv.org/abs/2405.15793) | NeurIPS · 2024 | 2,357 | 에이전트 비용과 컨텍스트 관리 | 간결하고 제한된 관찰 결과를 포함한 인터페이스 설계가 에이전트 성공률을 좌우함을 보여 줍니다. |
| 9 | [LongBench: A Bilingual, Multitask Benchmark for Long Context Understanding](https://arxiv.org/abs/2308.14508) | ACL · 2023 | 1,888 | 긴 컨텍스트와 컨텍스트 열화 | 장문 컨텍스트 이해를 평가하는 이중 언어, 다중 작업 벤치마크입니다. |
| 10 | [SGLang: Efficient Execution of Structured Language Model Programs](https://arxiv.org/abs/2312.07104) | NeurIPS · 2023 | 1,783 | 프롬프트/KV 캐싱 | RadixAttention으로 접두부를 공유하는 호출 간에 KV cache를 재사용하며, 접두부 캐싱의 기반이 되는 기술입니다. |
| 11 | [Large Language Models Can Be Easily Distracted by Irrelevant Context](https://arxiv.org/abs/2302.00093) | ICML · 2023 | 1,358 | 긴 컨텍스트와 컨텍스트 열화 | 프롬프트에 관련 없는 컨텍스트가 있으면 모델 정확도가 크게 떨어질 수 있습니다. |
| 12 | [FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance](https://arxiv.org/abs/2305.05176) | TMLR · 2023 | 1,247 | 모델 라우팅과 캐스케이드 | LLM을 선택하고 연쇄 호출하여 더 낮은 비용으로 강력한 모델에 근접한 품질을 얻습니다. |
| 13 | [SnapKV: LLM Knows What You are Looking for Before Generation](https://arxiv.org/abs/2404.14469) | NeurIPS · 2024 | 1,132 | 프롬프트/KV 캐싱 | 어텐션 헤드별로 중요한 KV 위치를 선택해 캐시 크기를 줄입니다. |
| 14 | [H2O: Heavy-Hitter Oracle for Efficient Generative Inference of Large Language Models](https://arxiv.org/abs/2306.14048) | NeurIPS · 2023 | 1,091 | 프롬프트/KV 캐싱 | KV cache 항목을 제거하되 'heavy hitter' 토큰은 유지해 메모리를 줄입니다. |
| 15 | [Evaluating Very Long-Term Conversational Memory of LLM Agents](https://arxiv.org/abs/2402.17753) | ACL · 2024 | 1,082 | 메모리와 요약 | 매우 긴 기간에 걸친 대화 메모리를 평가하는 벤치마크입니다. |
| 16 | [LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory](https://arxiv.org/abs/2410.10813) | ICLR · 2024 | 869 | 메모리와 요약 | 채팅 어시스턴트의 장기 메모리를 평가하는 벤치마크로, 회상 검사를 설계할 때 유용합니다. |
| 17 | [KIVI: A Tuning-Free Asymmetric 2bit Quantization for KV Cache](https://arxiv.org/abs/2402.02750) | ICML · 2024 | 825 | 프롬프트/KV 캐싱 | 별도 튜닝 없이 KV cache를 2비트로 양자화합니다. |
| 18 | [LongLLMLingua: Accelerating and Enhancing LLMs in Long Context Scenarios via Prompt Compression](https://arxiv.org/abs/2310.06839) | ACL · 2023 | 658 | 프롬프트 압축 | 질문을 고려해 긴 컨텍스트를 압축하며, 정확도를 유지하거나 높이면서 비용을 줄이는 것을 목표로 합니다. |
| 19 | [Scissorhands: Exploiting the Persistence of Importance Hypothesis for LLM KV Cache Compression at Test Time](https://arxiv.org/abs/2305.17118) | NeurIPS · 2023 | 598 | 프롬프트/KV 캐싱 | 토큰 중요도가 지속되는 특성을 활용해 KV cache를 압축합니다. |
| 20 | [LLMs Get Lost In Multi-Turn Conversation](https://arxiv.org/abs/2505.06120) | ICLR · 2025 | 562 | 긴 컨텍스트와 컨텍스트 열화 | 모델은 명세가 불충분한 다중 턴 대화에서 단일 턴보다 성능이 낮아집니다. |
| 21 | [Learning to Compress Prompts with Gist Tokens](https://arxiv.org/abs/2304.08467) | NeurIPS · 2023 | 534 | 프롬프트 압축 | 프롬프트를 재사용 가능한 소수의 'gist' 토큰으로 압축하도록 모델을 학습합니다. |
| 22 | [Hybrid LLM: Cost-Efficient and Quality-Aware Query Routing](https://arxiv.org/abs/2404.14618) | ICLR · 2024 | 519 | 모델 라우팅과 캐스케이드 | 라우터가 쿼리 난이도를 예측해 쉬운 쿼리를 더 작은 모델로 보냅니다. |
| 23 | [Agentic Context Engineering: Evolving Contexts for Self-Improving Language Models](https://arxiv.org/abs/2510.04618) | ICLR · 2025 | 505 | 에이전트 비용과 컨텍스트 관리 | 컨텍스트를 점진적으로 갱신되는 플레이북으로 다루어, 다시 쓰기로 인한 컨텍스트 붕괴를 방지합니다. |
| 24 | [Stop Overthinking: A Survey on Efficient Reasoning for Large Language Models](https://arxiv.org/abs/2503.16419) | TMLR · 2025 | 499 | 서베이 | 대규모 추론 모델의 효율적인 추론 방법을 정리한 서베이입니다. |
| 25 | [LLMLingua-2: Data Distillation for Efficient and Faithful Task-Agnostic Prompt Compression](https://arxiv.org/abs/2403.12968) | ACL · 2024 | 487 | 프롬프트 압축 | 작업에 구애받지 않고 여러 작업에 일반화되는, 더 빠른 프롬프트 압축기를 증류해 만듭니다. |
| 26 | [Adapting Language Models to Compress Contexts](https://arxiv.org/abs/2305.14788) | EMNLP · 2023 | 465 | 프롬프트 압축 | 모델을 파인튜닝해 긴 컨텍스트를 짧은 요약 벡터로 압축하고, 이를 소프트 프롬프트로 재사용합니다. |
| 27 | [Agent Workflow Memory](https://arxiv.org/abs/2409.07429) | ICML · 2024 | 451 | 에이전트 비용과 컨텍스트 관리 | 에이전트가 과거 궤적에서 재사용 가능한 워크플로를 도출해 다시 활용하며, 마이크로 스킬과 취지가 비슷합니다. |
| 28 | [LLMLingua: Compressing Prompts for Accelerated Inference of Large Language Models](https://arxiv.org/abs/2310.05736) | EMNLP · 2023 | 418 | 프롬프트 압축 | 프롬프트에서 정보량이 적은 토큰을 작은 언어 모델로 제거하며, 압축 예산을 조절할 수 있습니다. |
| 29 | [Prompt Cache: Modular Attention Reuse for Low-Latency Inference](https://arxiv.org/abs/2311.04934) | MLSys · 2023 | 417 | 프롬프트/KV 캐싱 | 반복되는 프롬프트 모듈의 사전 계산된 어텐션 상태를 재사용해 지연 시간을 줄입니다. |
| 30 | [RECOMP: Improving Retrieval-Augmented LMs with Compression and Selective Augmentation](https://arxiv.org/abs/2310.04408) | ICLR · 2023 | 340 | 프롬프트 압축 | 검색한 문서를 짧은 요약으로 압축한 뒤 검색 증강 프롬프트에 넣습니다. |
| 31 | [Language Model Tokenizers Introduce Unfairness Between Languages](https://arxiv.org/abs/2305.15425) | NeurIPS · 2023 | 319 | 토큰화와 언어 | 같은 텍스트라도 언어에 따라 토큰화 길이가 크게 달라집니다. |
| 32 | [AI Agents That Matter](https://arxiv.org/abs/2407.01502) | TMLR · 2024 | 289 | 평가와 통계 | 에이전트 평가는 정확도뿐 아니라 비용도 통제해야 한다고 주장합니다. |
| 33 | [Training Language Models to Reason Efficiently](https://arxiv.org/abs/2502.04463) | NeurIPS · 2025 | 285 | 효율적인 추론/출력 길이 | 추론 모델이 정확도와 추론 길이 사이에서 균형을 잡도록 학습합니다. |
| 34 | [Do All Languages Cost the Same? Tokenization in the Era of Commercial Language Models](https://arxiv.org/abs/2305.13707) | EMNLP · 2023 | 278 | 토큰화와 언어 | 토큰화 방식의 차이로 상용 API의 사용 비용이 언어별로 달라짐을 보여 줍니다. |
| 35 | [Compressing Context to Enhance Inference Efficiency of Large Language Models](https://arxiv.org/abs/2310.06201) | EMNLP · 2023 | 272 | 프롬프트 압축 | 자기 정보량(self-information)을 기준으로 중복된 어휘 단위를 제거해 추론 전에 입력을 줄입니다. |
| 36 | [Same Task, More Tokens: the Impact of Input Length on the Reasoning Performance of Large Language Models](https://arxiv.org/abs/2402.14848) | ACL · 2024 | 270 | 긴 컨텍스트와 컨텍스트 열화 | 입력 길이만 따로 분리해, 입력이 길어질수록 추론 성능이 떨어짐을 보여 줍니다. |
| 37 | [Token-Budget-Aware LLM Reasoning](https://arxiv.org/abs/2412.18547) | ACL · 2024 | 259 | 효율적인 추론/출력 길이 | 문제별 토큰 예산을 추정해 chain-of-thought 추론을 짧게 만듭니다. |
| 38 | [In-context Autoencoder for Context Compression in a Large Language Model](https://arxiv.org/abs/2307.06945) | ICLR · 2023 | 242 | 프롬프트 압축 | 긴 컨텍스트를 소수의 메모리 슬롯으로 인코딩하여, 고정된 디코더가 이를 읽도록 합니다. |
| 39 | [xRAG: Extreme Context Compression for Retrieval-augmented Generation with One Token](https://arxiv.org/abs/2405.13792) | NeurIPS · 2024 | 174 | 프롬프트 압축 | 검색한 문서 하나를 임베딩 토큰 하나로 표현해 RAG 컨텍스트를 줄입니다. |
| 40 | [AutoMix: Automatically Mixing Language Models](https://arxiv.org/abs/2310.12963) | NeurIPS · 2023 | 169 | 모델 라우팅과 캐스케이드 | 작은 모델이 자신의 답을 검증하고, 필요할 때만 더 큰 모델로 넘깁니다. |
| 41 | [Retrieval meets Long Context Large Language Models](https://arxiv.org/abs/2310.03025) | ICLR · 2023 | 159 | 긴 컨텍스트와 컨텍스트 열화 | 검색 증강과 긴 컨텍스트 윈도우를 비교하며, 검색이 더 적은 연산으로 긴 윈도우에 맞먹을 수 있음을 보여 줍니다. |
| 42 | [Batch Prompting: Efficient Inference with Large Language Model APIs](https://arxiv.org/abs/2301.08721) | EMNLP · 2023 | 154 | 배치 처리 | 여러 샘플을 프롬프트 하나에 담아 지시문 토큰을 공유합니다. |
| 43 | [Found in the Middle: Calibrating Positional Attention Bias Improves Long Context Utilization](https://arxiv.org/abs/2406.16008) | ACL · 2024 | 151 | 긴 컨텍스트와 컨텍스트 열화 | 위치 어텐션 편향을 보정해 lost-in-the-middle 현상을 줄입니다. |
| 44 | [Let Me Speak Freely? A Study On The Impact Of Format Restrictions On Large Language Model Performance.](https://arxiv.org/abs/2408.02442) | EMNLP · 2024 | 142 | 출력 형식 | 엄격한 출력 형식 제한은 추론 성능을 떨어뜨릴 수 있습니다. |
| 45 | [Prompt Compression for Large Language Models: A Survey](https://arxiv.org/abs/2410.12388) | NAACL · 2024 | 96 | 서베이 | 하드(토큰 수준) 및 소프트 프롬프트 압축 방법을 정리한 서베이입니다. |
| 46 | [500xCompressor: Generalized Prompt Compression for Large Language Models](https://arxiv.org/abs/2408.03094) | ACL · 2024 | 57 | 프롬프트 압축 | 프롬프트를 아주 적은 수의 특수 토큰으로 극단적으로 압축하는 것을 목표로 합니다. |
| 47 | [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) | ICLR · 2025 | 56 | 평가와 통계 | 정답을 얻는 데 드는 기대 비용을 정의하고, 이를 기준으로 모델을 비교합니다. |
| 48 | [Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory](https://arxiv.org/abs/2504.19413) | ECAI · 2025 | 868 | 메모리와 요약 | 전체 기록을 다시 보내는 대신, 핵심 사실을 추출하고 검색하는 메모리 계층입니다. |
| 49 | [CacheBlend: Fast Large Language Model Serving for RAG with Cached Knowledge Fusion](https://arxiv.org/abs/2405.16444) | ACM TOCS · 2024 | 342 | 프롬프트/KV 캐싱 | 공유 접두부가 아니더라도 RAG에서 검색된 청크의 KV cache를 재사용합니다. |
| 50 | [Mooncake: A KVCache-centric Disaggregated Architecture for LLM Serving](https://arxiv.org/abs/2407.00079) | ACM ToS · 2024 | 313 | 프롬프트/KV 캐싱 | 캐시된 KV 접두부를 재사용하는 스케줄러를 중심으로 구성한 Kimi의 프로덕션 서빙 아키텍처입니다. |
| 51 | [Recursively Summarizing Enables Long-Term Dialogue Memory in Large Language Models](https://arxiv.org/abs/2308.15022) | Neurocomputing · 2023 | 114 | 메모리와 요약 | 재귀적 요약을 긴 대화의 메모리로 활용합니다. |
| 52 | [Reducing Cost of LLM Agents with Trajectory Reduction](https://arxiv.org/abs/2509.23586) | PACMSE · 2025 | 68 | 에이전트 비용과 컨텍스트 관리 | 궤적에서 낭비 요소를 덜어 내어 에이전트 비용을 줄입니다. |
| 53 | [MemGPT: Towards LLMs as Operating Systems](https://arxiv.org/abs/2310.08560) | 프리프린트 · 2023 | 1,764 | 에이전트 비용과 컨텍스트 관리 | 메모리 계층을 관리해 에이전트가 컨텍스트 윈도우를 넘어서는 작업을 수행하도록 합니다. |
| 54 | [τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045) | 프리프린트 · 2024 | 1,669 | 평가와 통계 | 반복 실행에서 에이전트의 신뢰성을 측정하는 pass^k를 도입한 벤치마크입니다. |
| 55 | [RULER: What's the Real Context Size of Your Long-Context Language Models?](https://arxiv.org/abs/2404.06654) | 프리프린트 · 2024 | 1,580 | 긴 컨텍스트와 컨텍스트 열화 | 실제로 활용할 수 있는 컨텍스트 길이가 광고된 윈도우보다 짧은 경우가 많음을 보여 주는 벤치마크입니다. |
| 56 | [RouteLLM: Learning to Route LLMs with Preference Data](https://arxiv.org/abs/2406.18665) | 프리프린트 · 2024 | 922 | 모델 라우팅과 캐스케이드 | 선호도 데이터로 학습해 쿼리를 강력한 모델과 저렴한 모델 사이에서 라우팅합니다. |
| 57 | [Do NOT Think That Much for 2+3=? On the Overthinking of o1-Like LLMs](https://arxiv.org/abs/2412.21187) | 프리프린트 · 2024 | 605 | 효율적인 추론/출력 길이 | o1 계열 모델이 단순한 질문에도 토큰을 과도하게 쓰는 문제를 보이고, 이를 줄이는 방법을 제안합니다. |
| 58 | [Agentless: Demystifying LLM-based Software Engineering Agents](https://arxiv.org/abs/2407.01489) | 프리프린트 · 2024 | 566 | 에이전트 비용과 컨텍스트 관리 | 위치 파악, 수정, 검증으로 이어지는 고정 파이프라인이 SWE-bench Lite에서 개방형 에이전트 루프에 견줄 만한 성능을 낮은 비용으로 냅니다. |
| 59 | [L1: Controlling How Long A Reasoning Model Thinks With Reinforcement Learning](https://arxiv.org/abs/2503.04697) | 프리프린트 · 2025 | 416 | 효율적인 추론/출력 길이 | 사용자가 지정한 길이 예산을 따르도록 추론 모델을 학습합니다. |
| 60 | [Chain of Draft: Thinking Faster by Writing Less](https://arxiv.org/abs/2502.18600) | 프리프린트 · 2025 | 324 | 효율적인 추론/출력 길이 | 장황한 chain-of-thought 대신 최소한의 초안 단계만 작성하도록 모델에 요청합니다. |
| 61 | [RouterBench: A Benchmark for Multi-LLM Routing System](https://arxiv.org/abs/2403.12031) | 프리프린트 · 2024 | 300 | 모델 라우팅과 캐스케이드 | 다중 LLM 라우팅 시스템을 위한 벤치마크 및 평가 프레임워크입니다. |
| 62 | [Efficient Large Language Models: A Survey](https://arxiv.org/abs/2312.03863) | 프리프린트 · 2023 | 276 | 서베이 | 데이터, 모델, 시스템 수준에 걸친 효율화 기법을 폭넓게 정리한 서베이입니다. |
| 63 | [Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations](https://arxiv.org/abs/2411.00640) | 프리프린트 · 2024 | 194 | 평가와 통계 | LLM 평가에서 불확실성을 보고하는 통계적 방법을 안내합니다. |
| 64 | [A Survey of Context Engineering for Large Language Models](https://arxiv.org/abs/2507.13334) | 프리프린트 · 2025 | 171 | 서베이 | LLM 컨텍스트를 선택, 압축, 관리하는 기법을 체계적으로 정리한 서베이입니다. |
| 65 | [Non-Determinism of"Deterministic"LLM Settings](https://arxiv.org/abs/2408.04667) | 프리프린트 · 2024 | 159 | 평가와 통계 | 결정적으로 동작하도록 설정한 환경에서도 실행마다 출력이 달라질 수 있음을 보여 줍니다. |
| 66 | [Efficient Prompting Methods for Large Language Models: A Survey](https://arxiv.org/abs/2404.01077) | 프리프린트 · 2024 | 71 | 서베이 | 효율성을 위한 프롬프트 압축과 프롬프트 최적화 방법을 정리한 서베이입니다. |
| 67 | [Concise Reasoning via Reinforcement Learning](https://arxiv.org/abs/2504.05185) | 프리프린트 · 2025 | 69 | 효율적인 추론/출력 길이 | 강화 학습으로 간결한 추론을 유도합니다. |
| 68 | [The Complexity Trap: Simple Observation Masking Is as Efficient as LLM Summarization for Agent Context Management](https://arxiv.org/abs/2508.21433) | 프리프린트 · 2025 | 66 | 에이전트 비용과 컨텍스트 관리 | 에이전트 컨텍스트 관리에서 단순한 관찰 마스킹이 LLM 요약만큼 효율적입니다. |
| 69 | [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) | 프리프린트 · 2025 | 55 | 도구 선택과 스키마 크기 | 모든 도구 스키마를 주입하는 대신, 에이전트가 필요할 때 도구를 요청하도록 합니다. |
| 70 | [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) | 프리프린트 · 2025 | 41 | 도구 선택과 스키마 크기 | 모든 도구를 로드하지 않고 관련 있는 MCP 도구 설명만 검색해 가져오므로, 프롬프트 비대화를 막을 수 있습니다. |
| 71 | [Efficient Agents: Building Effective Agents While Reducing Cost](https://arxiv.org/abs/2508.02694) | 프리프린트 · 2025 | 40 | 에이전트 비용과 컨텍스트 관리 | 에이전트 시스템을 설계할 때 효율성과 효과성 사이의 트레이드오프를 체계적으로 연구합니다. |
| 72 | [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents?](https://arxiv.org/abs/2602.11988) | 프리프린트 · 2026 | 36 | 지침/컨텍스트 파일 | 리포지토리 컨텍스트 파일은 대체로 작업 성공률을 높이지 못했고, 추론 비용은 평균 20% 넘게 늘렸습니다. |
| 73 | [Less is More: Optimizing Function Calling for LLM Execution on Edge Devices](https://arxiv.org/abs/2411.15399) | 프리프린트 · 2024 | 33 | 도구 선택과 스키마 크기 | 함수 호출 시점에 선별한 더 작은 도구 집합만 제시해 엣지 기기에서 비용을 줄입니다. |
| 74 | [Don't Break the Cache: An Evaluation of Prompt Caching for Long-Horizon Agentic Tasks](https://arxiv.org/abs/2601.06007) | 프리프린트 · 2026 | 25 | 프롬프트/KV 캐싱 | 장기 에이전트 작업에서 프롬프트 캐싱의 효과를 평가합니다. |
| 75 | [TokenEconomizer: Experience-Driven Black-Box Cost Control for Coding Agents](https://arxiv.org/abs/2610.22732) | 프리프린트 · 2026 | 0 | 에이전트 비용과 컨텍스트 관리 | 이전 세션의 경험을 바탕으로 마이크로 스킬, 도구 가지치기, 루프 가드를 적용해 코딩 에이전트의 비용을 줄입니다. |

## 논문 제안하기

`examples/papers.json`에 항목(arXiv ID, 주제 하나, 직접 작성한 한 줄 요약)과 `examples/papers-ko.json`에 한국어 요약을 추가한 뒤
스크립트 두 개를 실행하고 PR을 엽니다. 알 수 없는 ID는 메타데이터 수집 단계에서 실패하므로 오타를 방지할 수 있습니다.
