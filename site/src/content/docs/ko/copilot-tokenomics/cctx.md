---
created: 2026-11-06
updated: 2026-11-19
dateSource: git
title: "cctx: VS Code Copilot 토큰 절감 및 개발 가속"
description: VS Code Marketplace에서 Code Context(cctx)를 설치해 코드 탐색 토큰을 줄이고, 에이전트 개발 속도를 가속하며, 실시간 AIC/USD 절감량을 확인합니다.
track: copilot
verified: 2026-11-19
sidebar:
  order: 11
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

[Code Context(cctx)](https://github.com/dotnetpower/code-context)는 VS Code 네이티브 도구와 MCP를 통해 로컬 색인 검색·범위가 제한된 소스 문맥을 제공합니다. 선택 가능한 조회 구현이지 과금 서비스나 절감 보장은 아닙니다.

## 조치: 지원되는 연동을 선택합니다

[Marketplace](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)에서 확장을 설치하고 프로젝트의 현재 백엔드·설정 안내를 따릅니다. 확장·백엔드 리비전, 운영 환경, VS Code 하네스를 기록합니다. 이번 문서 수정에서 설치·벤치마크 호환성을 독립적으로 재실행한 것은 아닙니다.

| 세션 | 확인할 내용 |
|---|---|
| 네이티브 도구가 있는 전경 VS Code | 실제로 도구가 노출되고 호출되는지 확인합니다. |
| Copilot SDK·Agent Host·백그라운드 | 백엔드 MCP 등 런타임이 지원하는 연동을 확인하며 편집기 네이티브 호출이 가능하다고 가정하지 않습니다. |
| 콜드·사용 불가 색인 | 캐시 미적중은 검색 결과 없음이 아닙니다. 제외 정책을 우회하지 않는 허용된 대체 경로를 사용합니다. |

소스 접근 정책, 비밀 파일 제외, 작업 영역 신뢰 경계를 유지합니다. 색인 조회가 클라이언트에서 금지된 접근을 허가하지는 않습니다.

## 확인 방법

일부러 큰 전체 파일 출력이 아니라 범위를 제한한 기본 검색·읽기를 기준선으로 삼습니다. 작업·리비전·모델·강도·합격 검사를 고정합니다. 콜드·웜 조회 지연과 전체 세션 크레딧·재시도·성공을 구분해 비교합니다.

| 관측값 | 의미 | 동일하지 않은 값 |
|---|---|---|
| 로컬 웜 색인 적중 | 로컬 색인을 재사용한 조회 | 제공자 프롬프트 캐시 적중 |
| 전달 결과 토큰 감소 | 명시한 계수 도구 기준으로 줄어든 소스 결과 | 실제 청구 크레딧 절감 |
| 반사실적 절감 추정 | 모델·단가·가정한 기준선에 따른 추정 | 관측 청구서·절감 보장 |
| 계정 크레딧 차이 | 측정한 실행 사이의 차이 | 통제 조건 없는 인과관계 증명 |

[GitHub AI Credits의 환산은 1크레딧당 $0.01 USD입니다](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing). 과거 `$0.001/AIC` 대시보드 예시를 GitHub 크레딧 환산으로 재사용하지 않습니다. 도구의 내부 단위가 다르면 원래 이름과 문서화된 환산을 유지하고 AIC로 바꿔 부르지 않습니다. 이미 추론을 포함한 출력에 추론을 더하지도 않습니다.

[프로젝트 벤치마크 보고서](https://github.com/dotnetpower/code-context/blob/main/docs/BENCHMARKS.md)의 결과는 **작성자 보고이며 이 저장소에서 독립 재현하지 않았습니다.** 수치를 인용하려면 리비전·작업·모델·표본 조건을 고정해야 합니다. 이 페이지는 새로운 가속·정확도·금액 절감률을 주장하지 않습니다.

## 절충점

데몬 시작, 색인 생성, 도구 정의, 라우팅 지침에도 비용이 있습니다. 짧고 위치를 아는 읽기에는 이득이 없을 수 있습니다. [사용량 측정](../02-measure-usage/)과 [문맥 관리](../03-context-hygiene/)로 성공·지연·비용이 정당화될 때만 채택합니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [marketplace.visualstudio.com: marketplace](https://marketplace.visualstudio.com/items?itemName=dotnetpower.code-context-native)
- [github.com: cctx](https://github.com/dotnetpower/code-context)
- [github.com: benchmarks](https://github.com/dotnetpower/code-context/blob/main/docs/BENCHMARKS.md)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
- [code.visualstudio.com: debug](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view)
