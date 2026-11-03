---
title: Papers
description: 75 papers on saving tokens, ranked by venue tier and citations.
sidebar:
  order: 91
---

75 papers relevant to saving tokens, compiled on **2026-11-03** and ranked by a transparent
authority proxy. Each entry links to its arXiv page; takeaways are our own one-line summaries.

## How the ranking works

1. **Tier 1** — peer-reviewed at a flagship ML / NLP / systems venue (ICLR, NeurIPS, ICML, ACL, EMNLP, NAACL,
   TACL, TMLR, SOSP, MLSys).
2. **Tier 2** — other peer-reviewed venues (journals and other conferences).
3. **Tier 3** — preprints (no venue listed).
4. Within a tier, **more citations first**.

:::caution[Read the ranking as a reading order, not a verdict]
- Year is the year Semantic Scholar lists for the paper (often its first preprint year, which can precede the
  venue year).
- Venue and citation count come from the [Semantic Scholar](https://www.semanticscholar.org/) Graph API on
  2026-11-03. Venues can lag the final publication, and citations keep changing. Re-run
  `examples/fetch_paper_meta.py` then `examples/build_papers.py` to refresh.
- Citations favor older and foundational work; recent preprints rank low simply because they are new.
- Peer review and citations do not guarantee a result holds for your agent or model. Treat every finding as a
  hypothesis and verify it with the [troubleshooting procedure](../agent-tokenomics/).
- We summarize the claim, not the numbers. Read the paper for the setup before relying on a figure.
:::

## Where each topic is applied

| Topic | Papers | Used in |
|-------|-------:|---------|
| Foundations | 3 | [Concepts](../agent-tokenomics/00-concepts/) |
| Tokenization and language | 3 | [Language and tokenization](../copilot-tokenomics/07-language-and-tokenization/) |
| Long context and context rot | 9 | [Concepts](../agent-tokenomics/00-concepts/), [Keep context lean](../copilot-tokenomics/03-context-hygiene/), [Improve](../agent-tokenomics/04-improve/) |
| Prompt / KV caching | 10 | [Concepts](../agent-tokenomics/00-concepts/), [Measure your usage](../copilot-tokenomics/02-measure-usage/), [Improve](../agent-tokenomics/04-improve/) |
| Prompt compression | 10 | [Prompt and output style](../copilot-tokenomics/06-prompt-and-output-style/), [Improve](../agent-tokenomics/04-improve/) |
| Output format | 1 | [Prompt and output style](../copilot-tokenomics/06-prompt-and-output-style/) |
| Batching | 1 | [Instructions and tools](../copilot-tokenomics/04-instructions-and-tools/), [Improve](../agent-tokenomics/04-improve/) |
| Model routing and cascades | 5 | [Models, planning, delegation](../copilot-tokenomics/05-models-and-planning/), [Improve](../agent-tokenomics/04-improve/) |
| Efficient reasoning / output length | 6 | [Agent modes and budget guards](../copilot-tokenomics/08-agent-modes-and-budget/), [Improve](../agent-tokenomics/04-improve/) |
| Agent cost and context management | 9 | [Isolate](../agent-tokenomics/02-isolate/), [Improve](../agent-tokenomics/04-improve/), [TokenEconomizer case study](../agent-tokenomics/case-tokeneconomizer/) |
| Tool selection and schema size | 3 | [MCP audit](../copilot-tokenomics/09-mcp-audit/), [Improve](../agent-tokenomics/04-improve/) |
| Instruction / context files | 1 | [Instructions and tools](../copilot-tokenomics/04-instructions-and-tools/) |
| Memory and summarization | 4 | [Prove](../agent-tokenomics/03-prove/), [Improve: compaction](../agent-tokenomics/04-improve/#compaction-without-losing-the-cache) |
| Evaluation and statistics | 5 | [Prove](../agent-tokenomics/03-prove/), [Result](../agent-tokenomics/05-result/) |
| Surveys | 5 | [Resources](../resources/) |

## Ranked list

| # | Paper | Venue · Year | Cited | Topic | Takeaway |
|--:|-------|--------------|------:|-------|----------|
| 1 | [Chain of Thought Prompting Elicits Reasoning in Large Language Models](https://arxiv.org/abs/2201.11903) | NeurIPS · 2022 | 23,523 | Foundations | Step-by-step prompting improves reasoning, and also increases output tokens. |
| 2 | [ReAct: Synergizing Reasoning and Acting in Language Models](https://arxiv.org/abs/2210.03629) | ICLR · 2022 | 13,383 | Foundations | Interleaves reasoning and tool actions: the agent loop whose cost this guide analyzes. |
| 3 | [Efficient Memory Management for Large Language Model Serving with PagedAttention](https://arxiv.org/abs/2309.06180) | SOSP · 2023 | 9,724 | Prompt / KV caching | PagedAttention manages KV-cache memory to raise serving throughput; basis of vLLM. |
| 4 | [Neural Machine Translation of Rare Words with Subword Units](https://arxiv.org/abs/1508.07909) | ACL · 2015 | 9,243 | Tokenization and language | Introduced the subword (BPE) units that underlie most LLM tokenizers. |
| 5 | [Lost in the Middle: How Language Models Use Long Contexts](https://arxiv.org/abs/2307.03172) | TACL · 2023 | 5,818 | Long context and context rot | Accuracy drops when the relevant information sits in the middle of a long input. |
| 6 | [SWE-bench: Can Language Models Resolve Real-World GitHub Issues?](https://arxiv.org/abs/2310.06770) | ICLR · 2023 | 4,683 | Foundations | Benchmark of real GitHub issues used to evaluate coding agents, including by TokenEconomizer. |
| 7 | [Efficient Streaming Language Models with Attention Sinks](https://arxiv.org/abs/2309.17453) | ICLR · 2023 | 2,768 | Long context and context rot | Keeps attention-sink tokens plus a recent window so inference can stream without full history. |
| 8 | [SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering](https://arxiv.org/abs/2405.15793) | NeurIPS · 2024 | 2,357 | Agent cost and context management | Shows that interface design, including concise and bounded observations, changes agent success. |
| 9 | [LongBench: A Bilingual, Multitask Benchmark for Long Context Understanding](https://arxiv.org/abs/2308.14508) | ACL · 2023 | 1,888 | Long context and context rot | Bilingual, multi-task benchmark for long-context understanding. |
| 10 | [SGLang: Efficient Execution of Structured Language Model Programs](https://arxiv.org/abs/2312.07104) | NeurIPS · 2023 | 1,783 | Prompt / KV caching | RadixAttention reuses KV cache across calls that share a prefix; foundation for prefix caching. |
| 11 | [Large Language Models Can Be Easily Distracted by Irrelevant Context](https://arxiv.org/abs/2302.00093) | ICML · 2023 | 1,358 | Long context and context rot | Irrelevant context in the prompt can substantially reduce model accuracy. |
| 12 | [FrugalGPT: How to Use Large Language Models While Reducing Cost and Improving Performance](https://arxiv.org/abs/2305.05176) | TMLR · 2023 | 1,247 | Model routing and cascades | Selects and cascades LLMs to approach a strong model's quality at lower cost. |
| 13 | [SnapKV: LLM Knows What You are Looking for Before Generation](https://arxiv.org/abs/2404.14469) | NeurIPS · 2024 | 1,132 | Prompt / KV caching | Selects the important KV positions per attention head to shrink the cache. |
| 14 | [H2O: Heavy-Hitter Oracle for Efficient Generative Inference of Large Language Models](https://arxiv.org/abs/2306.14048) | NeurIPS · 2023 | 1,091 | Prompt / KV caching | Evicts KV-cache entries, keeping 'heavy hitter' tokens to shrink memory. |
| 15 | [Evaluating Very Long-Term Conversational Memory of LLM Agents](https://arxiv.org/abs/2402.17753) | ACL · 2024 | 1,082 | Memory and summarization | Benchmark of very long-term conversational memory. |
| 16 | [LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory](https://arxiv.org/abs/2410.10813) | ICLR · 2024 | 869 | Memory and summarization | Benchmark of long-term memory in chat assistants; useful for designing recall checks. |
| 17 | [KIVI: A Tuning-Free Asymmetric 2bit Quantization for KV Cache](https://arxiv.org/abs/2402.02750) | ICML · 2024 | 825 | Prompt / KV caching | Quantizes the KV cache to 2 bits without tuning. |
| 18 | [LongLLMLingua: Accelerating and Enhancing LLMs in Long Context Scenarios via Prompt Compression](https://arxiv.org/abs/2310.06839) | ACL · 2023 | 658 | Prompt compression | Question-aware compression for long contexts, aiming to cut cost while keeping or improving accuracy. |
| 19 | [Scissorhands: Exploiting the Persistence of Importance Hypothesis for LLM KV Cache Compression at Test Time](https://arxiv.org/abs/2305.17118) | NeurIPS · 2023 | 598 | Prompt / KV caching | Exploits the persistence of token importance to compress the KV cache. |
| 20 | [LLMs Get Lost In Multi-Turn Conversation](https://arxiv.org/abs/2505.06120) | ICLR · 2025 | 562 | Long context and context rot | Models perform worse in multi-turn, underspecified conversations than in single-turn ones. |
| 21 | [Learning to Compress Prompts with Gist Tokens](https://arxiv.org/abs/2304.08467) | NeurIPS · 2023 | 534 | Prompt compression | Trains a model to compress a prompt into a few reusable 'gist' tokens. |
| 22 | [Hybrid LLM: Cost-Efficient and Quality-Aware Query Routing](https://arxiv.org/abs/2404.14618) | ICLR · 2024 | 519 | Model routing and cascades | A router predicts query difficulty and sends easy queries to a smaller model. |
| 23 | [Agentic Context Engineering: Evolving Contexts for Self-Improving Language Models](https://arxiv.org/abs/2510.04618) | ICLR · 2025 | 505 | Agent cost and context management | Treats the context as an evolving playbook updated incrementally to avoid context collapse from rewriting. |
| 24 | [Stop Overthinking: A Survey on Efficient Reasoning for Large Language Models](https://arxiv.org/abs/2503.16419) | TMLR · 2025 | 499 | Surveys | Survey of methods for efficient reasoning in large reasoning models. |
| 25 | [LLMLingua-2: Data Distillation for Efficient and Faithful Task-Agnostic Prompt Compression](https://arxiv.org/abs/2403.12968) | ACL · 2024 | 487 | Prompt compression | Distills a faster, task-agnostic prompt compressor that generalizes across tasks. |
| 26 | [Adapting Language Models to Compress Contexts](https://arxiv.org/abs/2305.14788) | EMNLP · 2023 | 465 | Prompt compression | Fine-tunes a model to compress long contexts into short summary vectors reused as soft prompts. |
| 27 | [Agent Workflow Memory](https://arxiv.org/abs/2409.07429) | ICML · 2024 | 451 | Agent cost and context management | Agents induce reusable workflows from past trajectories and reuse them, similar in spirit to micro-skills. |
| 28 | [LLMLingua: Compressing Prompts for Accelerated Inference of Large Language Models](https://arxiv.org/abs/2310.05736) | EMNLP · 2023 | 418 | Prompt compression | Uses a small language model to drop low-information tokens from a prompt, with a controllable budget. |
| 29 | [Prompt Cache: Modular Attention Reuse for Low-Latency Inference](https://arxiv.org/abs/2311.04934) | MLSys · 2023 | 417 | Prompt / KV caching | Reuses precomputed attention states of recurring prompt modules to cut latency. |
| 30 | [RECOMP: Improving Retrieval-Augmented LMs with Compression and Selective Augmentation](https://arxiv.org/abs/2310.04408) | ICLR · 2023 | 340 | Prompt compression | Compresses retrieved documents into short summaries before they enter a retrieval-augmented prompt. |
| 31 | [Language Model Tokenizers Introduce Unfairness Between Languages](https://arxiv.org/abs/2305.15425) | NeurIPS · 2023 | 319 | Tokenization and language | The same text tokenizes to very different lengths across languages. |
| 32 | [AI Agents That Matter](https://arxiv.org/abs/2407.01502) | TMLR · 2024 | 289 | Evaluation and statistics | Argues agent evaluations must control for cost, not just accuracy. |
| 33 | [Training Language Models to Reason Efficiently](https://arxiv.org/abs/2502.04463) | NeurIPS · 2025 | 285 | Efficient reasoning / output length | Trains reasoning models to trade accuracy against reasoning length. |
| 34 | [Do All Languages Cost the Same? Tokenization in the Era of Commercial Language Models](https://arxiv.org/abs/2305.13707) | EMNLP · 2023 | 278 | Tokenization and language | Shows that tokenization differences make commercial API usage cost different amounts per language. |
| 35 | [Compressing Context to Enhance Inference Efficiency of Large Language Models](https://arxiv.org/abs/2310.06201) | EMNLP · 2023 | 272 | Prompt compression | Prunes redundant lexical units by self-information to shorten inputs before inference. |
| 36 | [Same Task, More Tokens: the Impact of Input Length on the Reasoning Performance of Large Language Models](https://arxiv.org/abs/2402.14848) | ACL · 2024 | 270 | Long context and context rot | Isolates input length as a factor and shows reasoning degrades as inputs grow. |
| 37 | [Token-Budget-Aware LLM Reasoning](https://arxiv.org/abs/2412.18547) | ACL · 2024 | 259 | Efficient reasoning / output length | Estimates a token budget per problem to shorten chain-of-thought reasoning. |
| 38 | [In-context Autoencoder for Context Compression in a Large Language Model](https://arxiv.org/abs/2307.06945) | ICLR · 2023 | 242 | Prompt compression | Encodes a long context into a few memory slots that a frozen decoder can read. |
| 39 | [xRAG: Extreme Context Compression for Retrieval-augmented Generation with One Token](https://arxiv.org/abs/2405.13792) | NeurIPS · 2024 | 174 | Prompt compression | Represents each retrieved document as a single embedding token to shrink RAG context. |
| 40 | [AutoMix: Automatically Mixing Language Models](https://arxiv.org/abs/2310.12963) | NeurIPS · 2023 | 169 | Model routing and cascades | Has a small model verify its own answer and escalates to a larger model only when needed. |
| 41 | [Retrieval meets Long Context Large Language Models](https://arxiv.org/abs/2310.03025) | ICLR · 2023 | 159 | Long context and context rot | Compares retrieval augmentation with long context windows; retrieval can rival a longer window with less computation. |
| 42 | [Batch Prompting: Efficient Inference with Large Language Model APIs](https://arxiv.org/abs/2301.08721) | EMNLP · 2023 | 154 | Batching | Packs several samples into one prompt so instruction tokens are shared. |
| 43 | [Found in the Middle: Calibrating Positional Attention Bias Improves Long Context Utilization](https://arxiv.org/abs/2406.16008) | ACL · 2024 | 151 | Long context and context rot | Calibrates positional attention bias to reduce the lost-in-the-middle effect. |
| 44 | [Let Me Speak Freely? A Study On The Impact Of Format Restrictions On Large Language Model Performance.](https://arxiv.org/abs/2408.02442) | EMNLP · 2024 | 142 | Output format | Strict output-format restrictions can lower reasoning performance. |
| 45 | [Prompt Compression for Large Language Models: A Survey](https://arxiv.org/abs/2410.12388) | NAACL · 2024 | 96 | Surveys | Survey of hard (token-level) and soft prompt-compression methods. |
| 46 | [500xCompressor: Generalized Prompt Compression for Large Language Models](https://arxiv.org/abs/2408.03094) | ACL · 2024 | 57 | Prompt compression | Targets extreme prompt compression into very few special tokens. |
| 47 | [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) | ICLR · 2025 | 56 | Evaluation and statistics | Defines the expected cost to obtain a correct answer and compares models on it. |
| 48 | [Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory](https://arxiv.org/abs/2504.19413) | ECAI · 2025 | 868 | Memory and summarization | Memory layer that extracts and retrieves salient facts instead of replaying the full history. |
| 49 | [CacheBlend: Fast Large Language Model Serving for RAG with Cached Knowledge Fusion](https://arxiv.org/abs/2405.16444) | ACM TOCS · 2024 | 342 | Prompt / KV caching | Reuses KV caches of retrieved chunks in RAG even when they are not a shared prefix. |
| 50 | [Mooncake: A KVCache-centric Disaggregated Architecture for LLM Serving](https://arxiv.org/abs/2407.00079) | ACM ToS · 2024 | 313 | Prompt / KV caching | Production serving architecture for Kimi built around a scheduler that reuses cached KV prefixes. |
| 51 | [Recursively Summarizing Enables Long-Term Dialogue Memory in Large Language Models](https://arxiv.org/abs/2308.15022) | Neurocomputing · 2023 | 114 | Memory and summarization | Recursive summaries serve as memory for long dialogue. |
| 52 | [Reducing Cost of LLM Agents with Trajectory Reduction](https://arxiv.org/abs/2509.23586) | PACMSE · 2025 | 68 | Agent cost and context management | Reduces agent cost by trimming waste out of the trajectory. |
| 53 | [MemGPT: Towards LLMs as Operating Systems](https://arxiv.org/abs/2310.08560) | preprint · 2023 | 1,764 | Agent cost and context management | Manages memory tiers so an agent can work beyond its context window. |
| 54 | [τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045) | preprint · 2024 | 1,669 | Evaluation and statistics | Benchmark that introduces pass^k to measure agent reliability across repeated runs. |
| 55 | [RULER: What's the Real Context Size of Your Long-Context Language Models?](https://arxiv.org/abs/2404.06654) | preprint · 2024 | 1,580 | Long context and context rot | Benchmark showing the effective context length is often shorter than the advertised window. |
| 56 | [RouteLLM: Learning to Route LLMs with Preference Data](https://arxiv.org/abs/2406.18665) | preprint · 2024 | 922 | Model routing and cascades | Learns from preference data to route queries between a strong and a cheaper model. |
| 57 | [Do NOT Think That Much for 2+3=? On the Overthinking of o1-Like LLMs](https://arxiv.org/abs/2412.21187) | preprint · 2024 | 605 | Efficient reasoning / output length | Shows o1-like models spend excessive tokens on simple questions and proposes ways to cut them. |
| 58 | [Agentless: Demystifying LLM-based Software Engineering Agents](https://arxiv.org/abs/2407.01489) | preprint · 2024 | 566 | Agent cost and context management | A fixed localize-repair-validate pipeline competes with open-ended agent loops on SWE-bench Lite at low cost. |
| 59 | [L1: Controlling How Long A Reasoning Model Thinks With Reinforcement Learning](https://arxiv.org/abs/2503.04697) | preprint · 2025 | 416 | Efficient reasoning / output length | Trains reasoning models to obey a user-specified length budget. |
| 60 | [Chain of Draft: Thinking Faster by Writing Less](https://arxiv.org/abs/2502.18600) | preprint · 2025 | 324 | Efficient reasoning / output length | Prompts the model to write minimal draft steps instead of verbose chain-of-thought. |
| 61 | [RouterBench: A Benchmark for Multi-LLM Routing System](https://arxiv.org/abs/2403.12031) | preprint · 2024 | 300 | Model routing and cascades | Benchmark and evaluation framework for multi-LLM routing systems. |
| 62 | [Efficient Large Language Models: A Survey](https://arxiv.org/abs/2312.03863) | preprint · 2023 | 276 | Surveys | Broad survey of efficiency techniques across data, model, and system levels. |
| 63 | [Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations](https://arxiv.org/abs/2411.00640) | preprint · 2024 | 194 | Evaluation and statistics | Statistical guidance for reporting uncertainty in LLM evaluations. |
| 64 | [A Survey of Context Engineering for Large Language Models](https://arxiv.org/abs/2507.13334) | preprint · 2025 | 171 | Surveys | Survey organizing techniques for selecting, compressing, and managing LLM context. |
| 65 | [Non-Determinism of"Deterministic"LLM Settings](https://arxiv.org/abs/2408.04667) | preprint · 2024 | 159 | Evaluation and statistics | Shows outputs vary across runs even in settings meant to be deterministic. |
| 66 | [Efficient Prompting Methods for Large Language Models: A Survey](https://arxiv.org/abs/2404.01077) | preprint · 2024 | 71 | Surveys | Survey of prompt compression and prompt-optimization methods for efficiency. |
| 67 | [Concise Reasoning via Reinforcement Learning](https://arxiv.org/abs/2504.05185) | preprint · 2025 | 69 | Efficient reasoning / output length | Uses reinforcement learning to encourage concise reasoning. |
| 68 | [The Complexity Trap: Simple Observation Masking Is as Efficient as LLM Summarization for Agent Context Management](https://arxiv.org/abs/2508.21433) | preprint · 2025 | 66 | Agent cost and context management | Simple observation masking is as efficient as LLM summarization for agent context management. |
| 69 | [MCP-Zero: Active Tool Discovery for Autonomous LLM Agents](https://arxiv.org/abs/2506.01056) | preprint · 2025 | 55 | Tool selection and schema size | Lets the agent request tools on demand instead of injecting all tool schemas. |
| 70 | [RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation](https://arxiv.org/abs/2505.03275) | preprint · 2025 | 41 | Tool selection and schema size | Retrieves only the relevant MCP tool descriptions instead of loading every tool (prompt bloat). |
| 71 | [Efficient Agents: Building Effective Agents While Reducing Cost](https://arxiv.org/abs/2508.02694) | preprint · 2025 | 40 | Agent cost and context management | Systematic study of the efficiency-effectiveness trade-off in agent system design. |
| 72 | [Evaluating AGENTS.md: Are Repository-Level Context Files Helpful for Coding Agents?](https://arxiv.org/abs/2602.11988) | preprint · 2026 | 36 | Instruction / context files | Repository context files did not generally improve task success and raised inference cost by over 20% on average. |
| 73 | [Less is More: Optimizing Function Calling for LLM Execution on Edge Devices](https://arxiv.org/abs/2411.15399) | preprint · 2024 | 33 | Tool selection and schema size | Presents a smaller selected tool set at function-calling time to cut cost on edge devices. |
| 74 | [Don't Break the Cache: An Evaluation of Prompt Caching for Long-Horizon Agentic Tasks](https://arxiv.org/abs/2601.06007) | preprint · 2026 | 25 | Prompt / KV caching | Evaluates prompt caching on long-horizon agentic tasks. |
| 75 | [TokenEconomizer: Experience-Driven Black-Box Cost Control for Coding Agents](https://arxiv.org/abs/2610.22732) | preprint · 2026 | 0 | Agent cost and context management | Micro-skills, tool pruning, and a loop guard cut coding-agent cost using experience from earlier sessions. |

## Suggest a paper

Add an entry to `examples/papers.json` (arXiv id, one topic, a one-line note in your own words), run the two
scripts, and open a PR. The metadata fetch fails on an unknown id, which guards against typos.
