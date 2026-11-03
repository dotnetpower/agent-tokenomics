---
title: 리소스
description: 토큰을 절약하는 데 도움이 되는 리포지토리, 논문, 문서입니다. 2026-11-03에 존재 여부를 확인하여 엄선했습니다.
sidebar:
  order: 90
---

엄선한 목록입니다. 모든 링크는 **2026-11-03**에 확인했습니다. 목록에 포함했다고 해서 **추천하는 것은 아닙니다**.
각 프로젝트의 절감 수치는 제작자가 직접 밝힌 주장이며, 이 문서에서 재현하지 않았습니다. 사용할 때는 자신의 작업으로
벤치마크하고, 비용과 함께 품질 지표도 추적하세요([Agent Tokenomics](../agent-tokenomics/) 참조).

:::caution
프록시, 게이트웨이, 압축 도구는 에이전트와 모델 사이에 위치하므로 **프롬프트와 코드를 볼 수 있습니다**.
라이선스와 데이터 처리 방침을 읽어 보고, 비공개 리포지토리에서 사용하기 전에 검토하세요.
:::

## 시작하기

| 리소스 | 설명 |
|----------|-----|
| [microsoft/TokenEconomizer](https://github.com/microsoft/TokenEconomizer) | 경험 기반으로 코딩 에이전트의 비용을 제어합니다(마이크로 스킬, 도구 가지치기, 루프 가드). 논문: [arXiv:2610.22732](https://arxiv.org/abs/2610.22732), [프로젝트 페이지](https://tokeneconomizer.github.io). |
| [Optimize AI credit usage in VS Code](https://code.visualstudio.com/docs/agents/guides/optimize-usage) | 공식 실무 가이드입니다. |
| [Diagnose prompt caching with the Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer) | 공식 문서로, 요청 간에 접두부가 처음 달라지는 지점을 찾는 방법을 설명합니다. |
| [Azure OpenAI prompt caching](https://learn.microsoft.com/en-us/azure/foundry/openai/how-to/prompt-caching) | Azure OpenAI의 최소 길이, `cached_tokens`, `prompt_cache_key`, 보존 기간, 과금 및 데이터 상주 규칙을 설명합니다. |
| [OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching) | 캐시 가능 접두부 규칙, 캐시를 무효화하는 설정, 진단 방법을 설명합니다. |
| [Usage-based billing for individuals](https://docs.github.com/en/copilot/concepts/billing/usage-based-billing-for-individuals) | AI 크레딧이 청구되는 방식을 설명합니다. |
| [github/awesome-copilot](https://github.com/github/awesome-copilot) | 커뮤니티가 만든 Copilot용 지침, 에이전트, 스킬 모음입니다. |
| [pleasedodisturb/awesome-llm-token-optimization](https://github.com/pleasedodisturb/awesome-llm-token-optimization) | 전략, 도구, 논문을 정리한 또 다른 큐레이션 목록입니다. |

## 도구 출력과 명령 노이즈 줄이기

| 프로젝트 | 기능 |
|---------|--------------|
| [rtk-ai/rtk](https://github.com/rtk-ai/rtk) | 일반적인 개발 명령의 출력을 모델에 전달하기 전에 요약하는 CLI 프록시입니다. |
| [headroomlabs-ai/headroom](https://github.com/headroomlabs-ai/headroom) | 도구 출력, 로그, 파일, RAG 청크를 LLM에 전달하기 전에 압축합니다. |
| [gglucass/headroom-desktop](https://github.com/gglucass/headroom-desktop) | 같은 아이디어를 코딩 에이전트용 데스크톱 앱으로 감싼 도구입니다. |
| [Dave-London/Pare](https://github.com/Dave-London/Pare) | git, 테스트 러너, npm, Docker용 구조화된 토큰 효율적 MCP 서버입니다. |
| [claw-compactor](https://github.com/open-compress/claw-compactor) | 코드를 인식하는 분석을 갖춘 다단계 가역 압축 파이프라인입니다. |
| [optscale-kompressor](https://github.com/hystax/optscale-kompressor) | 코딩 에이전트를 위한 결정적 프롬프트 압축을 Rust 바이너리 하나로 제공합니다. |
| [fkiene/llmtrim](https://github.com/fkiene/llmtrim) | API 요청의 프롬프트, 기록, 도구 출력을 줄이는 로컬 프록시입니다. |
| [Paritok-official/paritok-4b-v1](https://github.com/Paritok-official/paritok-4b-v1) | 코딩 에이전트를 위한 비파괴 압축 게이트웨이입니다. |

## 프롬프트와 데이터 압축

| 프로젝트 | 기능 |
|---------|--------------|
| [microsoft/LLMLingua](https://github.com/microsoft/LLMLingua) | 프롬프트 압축 연구(LLMLingua, LLMLingua-2)입니다. 논문: [arXiv:2310.05736](https://arxiv.org/abs/2310.05736). |
| [jia-gao/leanctx](https://github.com/jia-gao/leanctx) | 프로덕션 앱에 바로 적용할 수 있는 프롬프트 압축 SDK입니다(LLMLingua-2 기반). |
| [3DAgentWorld/Toolkit-for-Prompt-Compression](https://github.com/3DAgentWorld/Toolkit-for-Prompt-Compression) | 여러 프롬프트 압축 방법을 모아 둔 툴킷입니다. |
| [ZongqianLi/Prompt-Compression-Survey](https://github.com/ZongqianLi/Prompt-Compression-Survey) | 프롬프트 압축 방법을 정리한 서베이입니다(NAACL 2025). |
| [sriinnu/clipforge-PAKT](https://github.com/sriinnu/clipforge-PAKT) | JSON, YAML, CSV, Markdown을 무손실 우선으로 압축합니다. |
| [ISON-format/ison](https://github.com/ISON-format/ison) | LLM 데이터용으로 토큰 효율이 높은 JSON 대체 형식입니다. |

## 간결한 응답 스타일과 측정 기반 평가

| 프로젝트 | 기능 |
|---------|--------------|
| [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman) | 에이전트가 간결하게 답하도록 만드는 스킬/프록시입니다. |
| [danielle34/cavewoman](https://github.com/danielle34/cavewoman) | 입력 압축과 출력 압축이 실제로 얼마나 절약하는지 측정하는 평가 프레임워크입니다. |
| [YangJunMan/claude-code-token-savings-benchmark](https://github.com/YangJunMan/claude-code-token-savings-benchmark) | 여러 토큰 절약 도구를 비교하는 재현 가능한 벤치마크입니다. |

## 코드는 적게 가져오고 메모리는 작게 유지하기

| 프로젝트 | 기능 |
|---------|--------------|
| [Graphify-Labs/graphify](https://github.com/Graphify-Labs/graphify) | 폴더(코드, 문서, PDF, 이미지)를 질의할 수 있는 지식 그래프로 변환합니다. |
| [jgravelle/jcodemunch-mcp](https://github.com/jgravelle/jcodemunch-mcp) | 심볼 단위로 검색합니다. 파일 전체가 아니라 필요한 함수만 가져옵니다. |
| [redhat-et/ripwire](https://github.com/redhat-et/ripwire) | 리포지토리를 읽지 않고도 관련 코드를 찾는 CLI + MCP 서버입니다. |
| [yamadashy/repomix](https://github.com/yamadashy/repomix) | 리포지토리를 AI 친화적인 파일 하나로 묶습니다(선택적으로 사용하세요. 큰 묶음은 토큰이 많이 듭니다). |
| [radimsem/remindb](https://github.com/radimsem/remindb) | SQLite 파일 하나로 휴대할 수 있는 에이전트 메모리입니다. |
| [cytostack/openwolf](https://github.com/cytostack/openwolf) | 에이전트 간에 공유하는 프로젝트 메모리로, 대화 기록에서 토큰을 집계합니다. |
| [fajarhide/contexere](https://github.com/fajarhide/contexere) | 여러 에이전트의 스킬, 키, 메모리를 한곳에서 관리합니다. |

## 토큰, 도구, 비용 감사

| 프로젝트 | 기능 |
|---------|--------------|
| [tokprof](https://github.com/dotnetpower/agent-tokenomics/tree/main/profiler)(이 저장소) | 코드를 수정하지 않고 호출마다 입력 토큰을 시스템 프롬프트, 이력, 사용자 프롬프트, 도구 항목으로 분해합니다. 초기 개발 단계입니다. |
| [alexgreensh/token-optimizer](https://github.com/alexgreensh/token-optimizer) | 에이전트 세션에서 "유령 토큰"과 컨텍스트 품질 저하를 찾아냅니다. |
| [activeing123/mcptoon](https://github.com/activeing123/mcptoon) | 도구 검색과 컨텍스트 압축을 목표로 하는 MCP 게이트웨이 CLI입니다. |
| [ccusage/ccusage](https://github.com/ccusage/ccusage) | 로컬 Claude Code 로그로 사용량과 비용 보고서를 만듭니다. |
| [Maciek-roboblog/Claude-Code-Usage-Monitor](https://github.com/Maciek-roboblog/Claude-Code-Usage-Monitor) | 예측 기능을 갖춘 실시간 사용량 모니터입니다. |
| [junhoyeo/tokscale](https://github.com/junhoyeo/tokscale) | 터미널에서 여러 AI 코딩 에이전트의 토큰 사용량을 확인합니다. |

## 관측 가능성과 게이트웨이(직접 운영하는 에이전트용)

| 프로젝트 | 기능 |
|---------|--------------|
| [langfuse/langfuse](https://github.com/langfuse/langfuse) | LLM 앱을 위한 오픈 소스 트레이싱, 평가, 비용 추적 도구입니다. |
| [Arize-ai/phoenix](https://github.com/Arize-ai/phoenix) | OpenTelemetry 기반의 AI 관측 가능성 및 평가 도구입니다([문서](https://arize.com/docs/phoenix)). |
| [Helicone/helicone](https://github.com/Helicone/helicone) | LLM 관측 가능성 플랫폼이자 OpenAI 호환 게이트웨이입니다([문서](https://docs.helicone.ai/getting-started/quick-start)). |
| [LangSmith](https://docs.langchain.com/langsmith/cost-tracking) | 토큰과 비용 내역을 제공하는 LangChain의 호스팅 트레이싱 및 평가 플랫폼입니다(SDK: [Python](https://github.com/langchain-ai/langsmith-python), [JS](https://github.com/langchain-ai/langsmith-javascript)). |
| [Portkey-AI/gateway](https://github.com/Portkey-AI/gateway) | 가드레일을 갖춘 AI 게이트웨이이며, Portkey 플랫폼에서 모니터링을 추가로 제공합니다([문서](https://portkey.ai/docs/introduction/what-is-portkey)). |
| [openlit/openlit](https://github.com/openlit/openlit) | 에이전트를 위한 OpenTelemetry 기반 트레이싱 및 평가 도구입니다. |
| [AgentOps-AI/agentops](https://github.com/AgentOps-AI/agentops) | 에이전트 모니터링과 LLM 비용 추적용 SDK입니다. |
| [BerriAI/litellm](https://github.com/BerriAI/litellm) | 100개 이상의 공급자에 대한 비용 추적과 라우팅을 제공하는 게이트웨이입니다. |
| [maximhq/bifrost](https://github.com/maximhq/bifrost) | 장애 조치를 지원하는 LLM / MCP / 에이전트 게이트웨이입니다. |

이 도구들은 [1단계(진단)](../agent-tokenomics/01-diagnose/#collect-the-request-log)와 연결되며, 해당 단계에서 Phoenix, Langfuse, LangSmith, Helicone, Portkey를 비교합니다. 이 도구로 요청별 로그를 수집하면
[예제 스크립트](https://github.com/dotnetpower/agent-tokenomics/tree/main/examples)에서 분석할 수 있습니다.

## 컨텍스트 엔지니어링과 스펙 주도 워크플로

| 프로젝트 | 기능 |
|---------|--------------|
| [github/spec-kit](https://github.com/github/spec-kit) | 스펙 주도 개발 툴킷입니다. 먼저 계획하고 나서 구현합니다. |
| [Fission-AI/OpenSpec](https://github.com/Fission-AI/OpenSpec) | AI 코딩 도우미를 위한 스펙 주도 개발 도구입니다. |
| [shanraisshan/claude-code-best-practice](https://github.com/shanraisshan/claude-code-best-practice) | Claude Code로 에이전트 엔지니어링을 하는 방법을 정리한 모범 사례입니다. |
| [microsoft/vscode-copilot-chat](https://github.com/microsoft/vscode-copilot-chat) | VS Code Copilot Chat 확장의 소스 코드입니다. |

## 논문

전체 순위 목록(학회 등급, 인용 수 순)은 [논문](../papers/) 페이지에 있습니다. 주요 논문은 다음과 같습니다.

| 논문 | 핵심 내용 |
|-------|----------|
| [Evaluating AGENTS.md (arXiv:2602.11988)](https://arxiv.org/abs/2602.11988) | 컨텍스트 파일은 대체로 작업 성공률을 높이지 못했고 비용은 평균 20% 넘게 늘렸습니다. 표준에서 벗어난 관행을 알려 줄 때 유용합니다. |
| [Lost in the Middle (arXiv:2307.03172)](https://arxiv.org/abs/2307.03172) | 모델은 긴 컨텍스트의 중간에 있는 정보를 시작이나 끝에 있는 정보보다 덜 안정적으로 활용합니다. |
| [TokenEconomizer (arXiv:2610.22732)](https://arxiv.org/abs/2610.22732) | 보고된 바에 따르면 에이전트–모델 조합 5개에서 SWE-Bench 평균 비용이 18.4–30.9% 낮아졌습니다. |
| [LLMLingua (arXiv:2310.05736)](https://arxiv.org/abs/2310.05736) | 소형 언어 모델을 이용한 프롬프트 압축입니다. |
| [tiktoken](https://github.com/openai/tiktoken) | 이 프로젝트의 [토큰 카운터](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/count_tokens.py)가 사용하는 토크나이저입니다. |

## 리소스 제안하기

링크, 직접 쓴 한 줄 설명, 존재 여부를 확인한 방법을 적어 이슈나 PR로 보내 주세요.
