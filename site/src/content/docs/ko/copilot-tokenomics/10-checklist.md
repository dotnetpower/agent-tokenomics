---
created: 2026-11-03
updated: 2026-11-19
dateSource: git
title: "체크리스트"
description: "VS Code GitHub Copilot 토큰과 AI 크레딧 비용을 낮추기 위해 세션 시작 전·중·후에 점검하는 한 페이지 토큰 이코노믹스 루틴입니다."
track: copilot
verified: 2026-11-19
sidebar:
  order: 10
---

> **적용 기준: 2026-11-19 공개 문서 검토.** 가격·정책·화면·설정은 바뀔 수 있습니다. 클라이언트·버전·하네스에 맞는 원문을 확인합니다. 이 날짜는 설치 환경·실청구·모델 품질 실험을 새로 재현했다는 뜻이 아닙니다.

## 세션 시작 전

- [ ] 클라이언트·버전, Copilot 또는 Local 하네스, 인증·과금 방식을 기록합니다.
- [ ] 작업에 맞는 모델·강도를 고르고 공개되는 Auto의 실제 선택도 기록합니다.
- [ ] 관련 파일·오류·제약·합격 검사를 제공합니다.
- [ ] 설치 서버 수가 아니라 로드한 지침과 필요한 도구를 확인합니다.
- [ ] 명확한 언어와 테스트·실패 보고를 유지하며 영어·위임을 필수로 만들지 않습니다.
- [ ] 무관한 작업은 새 채팅, 기존 문맥이 필요한 대안은 포크로 시작합니다.

## 진행 중

- [ ] 검색 범위와 새 출력을 제한하되 필요한 정의는 완결된 범위로 읽습니다.
- [ ] 종료 상태·경고·테스트 개수·미해결 실패를 보존합니다.
- [ ] 불확실하면 계획하고 충분한 규모의 독립 작업만 위임합니다.
- [ ] 부모·자식 사용량은 한 번씩 집계하고 재시도도 기록합니다.
- [ ] 필요할 때 압축하고 핵심 결정이 남았는지 확인합니다.
- [ ] 프롬프트 재시도 가드·지원되는 요청 제한·강제 예산을 구분합니다.

## 종료 후·주간 검토

- [ ] [기준선 기록](../02-measure-usage/)에 실제 크레딧·시간·재시도·성공을 채웁니다.
- [ ] 비공개 필드는 확인 불가로 표시하고 캐시 입력·추론을 중복 합산하지 않습니다.
- [ ] 로컬 조회 추정치와 제공자 캐시·실청구를 구분합니다.
- [ ] 계정 제공량과 추가 사용·예산 정책을 확인합니다.
- [ ] 비용과 품질이 함께 개선될 때만 변경을 유지합니다.

## 확인 방법

완료한 작업 하나에서 체크한 항목을 로그·실제 설정·검사 결과와 연결합니다. 체크 표시만으로 검증이 끝난 것은 아닙니다.

## 절충점

이는 운영 절차이지 절감 보장이 아닙니다. 적용 조건은 [모델 선택](../05-models-and-planning/), [MCP 감사](../09-mcp-audit/), [예산 가드](../08-agent-modes-and-budget/)에서 확인합니다. 캐시 적중률을 높이기 위해 필요한 문맥을 제거하지 않습니다.

## 출처

- [code.visualstudio.com: harness](https://code.visualstudio.com/blogs/2026/11/18/copilot-harness)
- [code.visualstudio.com: optimize](https://code.visualstudio.com/docs/agents/guides/optimize-usage)
- [code.visualstudio.com: cache](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)
- [docs.github.com: billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)
