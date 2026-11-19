---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "Copilot 토큰 이코노믹스 (Tokenomics · Token Economics)"
description: "VS Code GitHub Copilot을 위한 실전 토큰 이코노믹스(Token Economics, 토큰 경제학): 토큰과 AI 크레딧 절약, 컨텍스트·MCP 최적화, cctx 개발 가속."
track: copilot
verified: 2026-11-19
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 문제

트랙 A는 **작업 성공률을 유지하면서** GitHub Copilot 비용을 줄이는 방법을 다룹니다. 약속된 절감률보다 실제 과금 방식과 클라이언트·하네스를 먼저 확인합니다.

## 조치

[2026-11-18 공식 발표](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)는 새 VS Code 에이전트 세션의 기본값을 Copilot 하네스로 안내하며 Local은 전환 중입니다. Local 전용 설정이나 편집기 네이티브 도구가 모든 SDK·Agent Host 세션에서도 지원된다고 가정하지 않습니다.

| 단계 | 문서 |
|---|---|
| 계기판·기준선 확립 | [과금](01-how-usage-is-charged/) · [사용량 측정](02-measure-usage/) |
| 반복 문맥 줄이기 | [컨텍스트 관리](03-context-hygiene/) · [지침과 도구](04-instructions-and-tools/) |
| 작업에 맞는 역량 선택 | [모델·계획·위임](05-models-and-planning/) |
| 간결하지만 완전한 요청 | [프롬프트·출력](06-prompt-and-output-style/) · [언어·토큰화](07-language-and-tokenization/) |
| 실행·도구 목록 제어 | [모드·예산](08-agent-modes-and-budget/) · [MCP 감사](09-mcp-audit/) |
| 적용·재확인 | [체크리스트](10-checklist/) · [선택적 cctx 조회](cctx/) |
| 다른 제품과 비교 | [Copilot·Codex·Claude Code 해부](coding-agent-quotas-and-caching/) |

## 확인 방법

같은 대표 작업·리비전에서 모델·강도, 클라이언트·하네스, 인증, 캐시 상태를 기록하고 한 변수만 바꿉니다. 실제 크레딧·시간·재시도·합격 검사를 비교하며 비공개 지표를 0으로 처리하지 않습니다.

## 절충점

제품 동작, 제공자 API 동작, 로컬 텍스트 측정을 구분합니다. 연결한 연구·작성자 벤치마크는 가설이지 본인 작업의 절감 보장이 아닙니다. 기존 논문 목록의 메타데이터 조회일은 유지하며 문서 수정이 새 실험을 뜻하지는 않습니다.

## 문서 날짜

모든 사이트 문서는 작성일·수정일·해당되는 검증일을 구분해 표시합니다. 과거 작성일은 Git 최초 등록 기준이며 최초 초안 작성 시점을 단정하지 않습니다. 작성 근거가 없는 페이지는 기록 시작일로 표시합니다. 수정일은 내용 변경 시, 검증일은 관련 출처 확인 시 갱신하며 단순 재빌드로 바꾸지 않습니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
