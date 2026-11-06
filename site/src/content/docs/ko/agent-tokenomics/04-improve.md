---
title: "4. 개선"
description: "LLM 에이전트 토큰 이코노믹스 4단계(개선): 마이크로 스킬, 도구 가지치기, 루프 가드, 도구 출력 축소, 프롬프트 캐싱을 실험당 하나씩 적용합니다."
track: agent
stage: improve
verified: 2026-11-03
sidebar:
  order: 4
---

**목표:** 검증된 낭비를 없애는 최소한의 변경을 적용합니다. 실험당 수정은 하나만 적용합니다.

## 수정 카탈로그

| 증명된 원인 | 수정 | 참고 |
|-------------|------|------|
| 저장소 정보 반복 탐색 | 확인된 짧은 저장소 사실을 세션 시작 부분에 둡니다(지침 파일 또는 제공하는 "마이크로 스킬"). 예: 올바른 테스트 명령, 필요한 환경 변수, 생성된 파일의 위치 | 사실은 적게, 검증된 것만 유지합니다. 이후 세션에서 반박되면 해당 사실을 삭제합니다 |
| 사용하지 않는 도구 | 도구 정의를 **가지치기**합니다. 도구를 필요할 때 로드합니다. 서브에이전트에는 좁은 도구 집합을 부여합니다 | 가지치기한 도구를 쉽게 다시 켤 수 있게 합니다 |
| 실패 루프 | 한 턴에서 반복되는 동일한 오류를 감지합니다. **한 번** 방향 전환("다른 접근을 시도하세요")을 제안합니다. 그래도 막혀 있으면 사용자에게 중단하는 방법을 알립니다 | 턴을 조용히 종료하지 않습니다. 턴당 방향 전환 제안 횟수에 상한을 둡니다 |
| 비대한 도구 출력 | 결과를 잘라내거나 페이지로 나눕니다. 파일 전체를 읽기 전에 먼저 검색합니다. 요약을 반환합니다. 대량 반복 작업은 스크립트로 실행하고 결과만 반환합니다 | 에이전트가 더 요청할 수 있도록 잘렸다는 사실을 눈에 보이게 표시합니다 |
| 기록 증가 | 오래된 턴을 압축/요약합니다. 작업마다 새 세션을 시작합니다. 탐색은 서브에이전트에 격리합니다 | 요약에는 손실이 있습니다. [캐시를 잃지 않는 컨텍스트 압축](#compaction-without-losing-the-cache)을 참고하세요 |
| 접두부 변동 | 안정적인 내용(시스템 프롬프트, 지침, 도구 정의)을 앞에, 자주 바뀌는 내용을 뒤에 둡니다. 세션 도중에 접두부를 수정하지 않습니다 | 캐시 diff 뷰로 확인합니다 |
| 과도하게 큰 모델 / 강도 | 쉬운 단계는 더 작은 모델로 라우팅합니다. 추론 강도를 기본값으로 낮춥니다 | 3단계의 품질 점검을 통과해야 합니다 |
| 재시도와 재작업 | 프롬프트에 더 나은 성공 기준을 제시합니다. 비용이 큰 단계 전에 저렴하게 검증합니다 | 품질도 함께 좋아지는 경우가 많습니다 |
| 컨텍스트 열화 | 핵심 규칙을 시작 부분에 두고, 핵심 제약은 끝부분 근처에서도 다시 언급합니다. 컨텍스트 윈도우가 거의 차기 전에 압축하거나 다시 시작합니다 | 모델은 긴 컨텍스트의 중간 부분에 덜 안정적으로 주의를 기울입니다([Lost in the Middle](https://arxiv.org/abs/2307.03172)) |
| 도구 수 × 단계 수 승수 | 워크스페이스별 도구 집합을 사용합니다. 사용량을 기준으로 서버를 점검합니다. 단계에 상한(`maxTurns` 방식의 제한)을 두고 완료 기준을 정의합니다 | [MCP 감사](../../copilot-tokenomics/09-mcp-audit/)를 참고하세요 |
| 지나치게 넓은 프롬프트 / 지침 | "지뢰 규칙"만 남깁니다. 선언적 규칙을 씁니다. 프롬프트 하나에 작업 하나만 담습니다. 복잡한 작업에는 계획 파일을 사용합니다 | 컨텍스트 파일은 성공률을 높이지 못한 채 비용만 늘리는 경우가 많습니다([arXiv:2602.11988](https://arxiv.org/abs/2602.11988)) |
| 모든 프롬프트에 들어가는 길고 안정적인 지식 | 항상 포함하는 텍스트 대신 필요할 때 불러오는 **스킬**로 묶습니다 | 단일 정보 출처(single source of truth)를 유지합니다 |
| 장황한 모델 출력 | 상시 적용되는 출력 제한("코드만", 고정 형식)을 두고 구조화된 출력을 사용합니다 | 출력 토큰은 대개 입력보다 비쌉니다 |
| 큰 정적 문서 × 호출 횟수 | 질문에 필요할 때만 참조 문서를 첨부합니다(규칙 또는 검색). 섹션으로 나누어 관련 있는 부분만 포함합니다. 도구나 스킬을 통해 필요할 때 로드합니다. 여러 번 호출되는 단계에서는 매번이 아니라 한 번만 첨부합니다 | 조건 판단은 3단계의 구성 요소 수준 평가로 검증합니다. `tokens × calls`를 측정합니다 |
| 중복된 파이프라인 단계(팬아웃) | 결정적 검사로 단계를 건너뜁니다. 인접한 분류 단계를 한 번의 호출로 합칩니다. 가드레일과 라우팅에는 더 작은 모델을 사용합니다. 출력이 거의 쓰이지 않는 단계는 제거합니다 | 제거하는 각 단계는 3단계 평가를 통과해야 합니다. 증명될 때까지 기존 경로를 플래그 뒤에 유지합니다 |
| 세션별 무제한 상태 | 내부 메시지 수를 제한하거나 최근 메시지만 유지합니다. 이후 단계에 필요한 것만 유지합니다(전체 보고서가 아닌 짧은 결과). 아래의 압축 규칙에 따라 요약합니다 | 먼저 빠진 잘라내기 로직이 없는지 찾습니다. 한 줄 수정으로 끝나는 경우가 많습니다 |
| 짧아서 캐시할 수 없는 프롬프트 | 단계를 인접 단계에 합치거나, 규칙으로 대체하거나, 그대로 둡니다(작으므로). 최소 길이를 채우려고 내용을 부풀리지는 않습니다 | 공급자의 현재 최소 길이와 `cached_tokens`를 확인합니다 |
| 대량 / 폴링 도구 루프 | 루프를 스크립트로 실행하고 요약만 반환합니다 | 스크립트 방식과 대화형 방식을 벤치마크합니다 |

<figure class="at-doc-figure">
  <img src="https://raw.githubusercontent.com/microsoft/TokenEconomizer/main/docs/assets/framework_overview.png" alt="에이전트 세션의 비용을 제어하는 TokenEconomizer 프레임워크" loading="lazy" />
  <figcaption>경험 기반 비용 제어를 위한 참고 프레임워크입니다. 출처: <a href="https://github.com/microsoft/TokenEconomizer">microsoft/TokenEconomizer (Apache 2.0)</a>, <a href="https://arxiv.org/abs/2610.22732">arXiv:2610.22732</a>.</figcaption>
</figure>

<a id="observation-masking-for-old-tool-output"></a>
## 오래된 도구 출력에 관찰 마스킹 적용

[비대한 도구 출력이나 기록 증가](../02-isolate/)가 원인이라면 추가 LLM 호출로 요약하는 대신 최근 `K`단계보다 오래된 큰 도구 결과를 정해진 형식의 자리 표시자로 바꾸세요. 다음은 **가상 예시**입니다. 도구 호출과 결과의 짝을 유지하고 최근 작업 구간은 그대로 보존합니다.

```text
current_step = 5; K = 3  # keep results from steps 3–5 unchanged
turn 1 read_file result → [omitted 4,200-token read_file output from turn 1; path=src/checkout.py lines=1..180]
```

마스킹에는 요약 모델 호출이 필요하지 않지만 자리 표시자에도 입력 토큰이 듭니다. 정보가 손실되고 프롬프트 캐시 일부가 무효화될 수 있으므로 원본은 필요할 때 다시 읽도록 안전하게 보관하고 필요한 경우 경로도 마스킹하세요. 기준선과 비용, 캐시 재사용률, 작업 성공률을 비교합니다. 이 방법은 [The Complexity Trap](https://arxiv.org/abs/2508.21433)을 참고했으며, 논문의 결과는 저자가 보고한 것으로 이곳에서 독립적으로 재현하지 않았습니다.

<a id="compaction-without-losing-the-cache"></a>
<a id="캐시를-잃지-않는-컨텍스트-압축"></a>
## 캐시를 유지하는 컨텍스트 압축

컨텍스트 압축과 프롬프트 캐싱은 서로 반대 방향으로 작용합니다. 기록의 *앞부분*을 다시 쓰면 요청 접두부가 바뀌어
공급자가 캐시를 재사용할 수 없게 되고, 입력 비용을 정가로 다시 지불하게 됩니다.

<figure class="at-doc-figure">
<div class="at-diag-stack">
<div class="at-diag-row"><span>접두부 유지</span><div class="at-diag-bar"><span class="at-diag-seg seg-prefix" style="width: 40%">안정적 접두부</span><span class="at-diag-seg seg-hist" style="width: 30%">요약</span><span class="at-diag-seg seg-user" style="width: 30%">최근 턴</span></div><span>CACHE HIT</span></div>
<div class="at-diag-row"><span>전체 재작성</span><div class="at-diag-bar"><span class="at-diag-seg seg-miss" style="width: 100%">바뀐 접두부</span></div><span>CACHE MISS</span></div>
</div>
<figcaption>개념도: 안정적인 접두부 뒤에 요약과 최근 턴을 배치하면 해당 접두부의 캐시를 재사용할 수 있습니다. 접두부를 다시 쓰면 이전 캐시가 맞지 않습니다. 실제 적중 여부는 공급자의 최소 길이, 보존 기간과 캐시 조건에 따라 달라집니다.</figcaption>
</figure>

- **접두부를 안정적으로 유지합니다.** 시스템 프롬프트, 지침, 도구 정의를 앞에 두고 변경하지 않습니다.
  압축 사이에는 기록에 추가만 합니다.
- **드물게, 한 번에 압축합니다.** 임계값(예: 컨텍스트 윈도우의 일정 비율)에 도달했을 때나
  작업 경계에서 압축하며, 매 턴마다 하지 않습니다.
- **비용을 비교합니다.** 다음 조건에서만 압축합니다.
  `saved tokens × remaining turns × (1 − cached-input discount)`가
  `summarization call cost + cache rebuild cost`를 초과할 때입니다. 남은 세션이 짧으면 비용을 회수하기 어렵습니다.
- **요약을 구조화합니다.** 시간순이 아니라 에이전트에게 필요한 기준으로 정리합니다.

```text
Goal:            <one line>
Decisions:       <what was decided and why, briefly>
Constraints:     <rules, versions, "do not touch X">
Done / artifacts:<files changed, commands that worked, with paths>
Open items:      <unresolved questions, next step>
Failures to avoid: <error signatures already seen and what fixed them>
```

  자연어 몇 줄은 필드로 담기 어려운 뉘앙스를 전달할 수 있습니다. 구조를 중심으로 삼고,
  맞지 않는 내용에만 서술형 문장을 추가합니다.
- **응답을 지연하지 않도록 백그라운드에서 요약합니다.** 컨텍스트가 토큰 임계값을 넘으면 트리거하고,
  답변 이후에 비동기로 실행한 뒤 다음 턴부터 결과를 사용합니다.
- **요약과 최근 원문을 함께 사용합니다.** 요약과 마지막 몇 턴의 원문을 함께 보내고, 요약 길이에 상한을 둡니다.
  요약기 자체의 프롬프트는 대체로 짧아서(캐시 가능 최소 길이보다 짧은 경우가 많음) 비용 대부분이
  캐시되지 않는 입력에서 발생합니다. 이후 호출에서 절약하는 양이 이 비용보다 큰지 확인합니다(위의 손익분기 참고).
- **무엇을 반드시 남길지는 도메인 담당자가 정합니다**(예: 사람 상담원에게 꼭 필요한 정보).
  엔지니어는 이를 구현하고 테스트합니다. 도메인이 바뀌면 목록을 다시 검토합니다.
- **가능하면 요약하지 말고 오프로드합니다.** 긴 원본 출력은 파일에 저장하고 한 줄 요약과 경로만 남기면,
  에이전트가 필요할 때 다시 읽을 수 있습니다(거의 무손실).
- **요약을 보호합니다.** 요약을 반복해서 요약하지 않습니다(오류가 누적됩니다). 원본을 보관하고,
  비밀 정보와 개인 데이터는 요약에 들어가기 전에 가립니다.
- [손실이 있는 수정의 기준](../03-prove/#when-the-fix-is-lossy-summaries-compaction)으로 검증합니다.

<a id="worked-example-multi-turn-counselor-dialogue-compaction"></a>
### 실전 예시: 다중 턴 상담 대화 이력 압축

고객지원·상담 에이전트에서는 이전 턴의 인사말, 사과 문구, `<tool_call>` JSON 래퍼, 반복되는 세션 UUID, 마무리 멘트가 매 턴 누적됩니다. 예제에서 완료된 상담 에피소드를 BPE 친화적 상태·조치 수식 체인(`문의 -> tool(인자) => 결과 | ...`)으로 압축하면 고객 ID, 주문번호, 날짜, 금액, 송장번호, 담당자 이관 결과를 **100%** 보존하면서 대화 이력 토큰을 크게 줄일 수 있습니다. 영문 전용 예제는 [`examples/formula_prompt_en.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_en.py), 한국어 전용 예제는 [`examples/formula_prompt_ko.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt_ko.py)로 분리되어 있습니다(통합 실행: [`examples/formula_prompt.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/formula_prompt.py) `--lang {en,ko,all}`, 2026-11-04 확인).

**영문 다중 턴 채팅·상담 이력 (`examples/formula_prompt_en.py`):**

| 영문 다중 턴 채팅·상담 및 Hugging Face 데이터셋 (`row`) | 원본 대화 (`o200k` / `cl100k`) | BPE 수식 압축 (`o200k` / `cl100k`) | `o200k_base` 변화 | `cl100k_base` 변화 | 핵심 슬롯 보존 |
|---------------------------------------------------------|-------------------------------:|-----------------------------------:|------------------:|-------------------:|---------------:|
| 로그인 인증번호 이메일 전환 상담 (13턴, [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[0]`) | 336 / 350 | 57 / 58 | **-83.0%** | **-83.4%** | 7/7 |
| 리콜 모니터 반품·환불 안내 (13턴, [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[1]`) | 339 / 347 | 53 / 53 | **-84.4%** | **-84.7%** | 8/8 |
| 보증기간 교환 신청 및 취소 버튼 오류 상담 (19턴, [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[2]`) | 526 / 535 | 81 / 81 | **-84.6%** | **-84.9%** | 9/9 |
| 인증번호 횟수 초과 및 비밀번호 재설정 (17턴, [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) `train[3]`) | 460 / 471 | 49 / 49 | **-89.3%** | **-89.6%** | 6/6 |
| 심계항진 의료 문진 인테이크 채팅 (10턴, [`nwirandx/Korean-OSCE-history-taking-dialogues`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) `sessions[0..9]`) | 321 / 322 | 74 / 73 | **-76.9%** | **-77.3%** | 9/9 |
| 생산라인 로봇 과열·소음 기술지원 채팅 (4턴, [`FunDialogues/customer-service-robot-support`](https://huggingface.co/datasets/FunDialogues/customer-service-robot-support) `train[1,2]`) | 112 / 114 | 35 / 36 | **-68.8%** | **-68.4%** | 6/6 |
| **합계 (6개 영문 다중 턴 채팅·상담 대화)** | **2,094 / 2,139** | **349 / 350** | **-83.3%** | **-83.6%** | **45/45 (100%)** |

**한국어 다중 턴 채팅·상담 이력 (`examples/formula_prompt_ko.py`):**

| 한국어 다중 턴 채팅·상담 및 Hugging Face 데이터셋 (`row`) | 원본 대화 (`o200k` / `cl100k`) | BPE 수식 압축 (`o200k` / `cl100k`) | `o200k_base` 변화 | `cl100k_base` 변화 | 핵심 슬롯 보존 |
|-----------------------------------------------------------|-------------------------------:|-----------------------------------:|------------------:|-------------------:|---------------:|
| 주소 조회 + 배송 상태 + 스팸 신고 (14턴, [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[0]`) | 304 / 460 | 84 / 129 | **-72.4%** | **-72.0%** | 9/9 |
| 장바구니 + 노트북 불량 + 영수증 이관 (15턴, [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[2]`) | 386 / 565 | 127 / 190 | **-67.1%** | **-66.4%** | 9/9 |
| 배송 조회 + 멤버십 + 환불/결제취소 (16턴, [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[5]`) | 436 / 638 | 158 / 225 | **-63.8%** | **-64.7%** | 11/11 |
| 반품·교환 약관 + 범위 외 거절 + 주문내역 (13턴, [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) `train[6]`) | 368 / 545 | 124 / 194 | **-66.3%** | **-64.4%** | 11/11 |
| 택배 조회 및 무인보관함 배송지 변경 채팅 (6턴, [`schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1`](https://huggingface.co/datasets/schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1) `train[2]`) | 195 / 226 | 55 / 61 | **-71.8%** | **-73.0%** | 5/5 |
| 심계항진 의료 문진 인테이크 채팅 (10턴, [`nwirandx/Korean-OSCE-history-taking-dialogues`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) `sessions[0..9]`) | 402 / 613 | 102 / 144 | **-74.6%** | **-76.5%** | 10/10 |
| 멀티세션 페르소나·가족 일상 메신저 채팅 (18턴, [`nayohan/141_korean_multi_session_dialogue`](https://huggingface.co/datasets/nayohan/141_korean_multi_session_dialogue) `train[0]`) | 359 / 567 | 77 / 103 | **-78.6%** | **-81.8%** | 11/11 |
| **합계 (7개 한국어 다중 턴 채팅·상담 대화)** | **2,450 / 3,614** | **727 / 1,046** | **-70.3%** | **-71.1%** | **66/66 (100%)** |

나아가 Hugging Face 공개 데이터셋에서 실시간 조회한 **580개 채팅 대화·고객지원·법률·노무 상담 세션**(영문 5개 데이터셋 250건 + 한국어 7개 데이터셋 330건, `python3 examples/formula_prompt.py --hf-live --hf-rows 50`)에 범용 BPE 문법·담화 압축과 무손실 슬롯 복원 가드를 적용한 결과, 법조문·사건번호·이메일·UUID·케밥케이스 ID·주문번호·날짜·금액·플레이스홀더 슬롯 **1,731 / 1,731개(100.0%)** 를 무손실로 보존하면서 전체 토큰을 **`o200k_base` 기준 56.3%**(**`cl100k_base` 기준 54.9%**) 절감했습니다.

| 언어 및 Hugging Face 데이터셋 (`--hf-live --hf-rows 50`) | 행 수 | `o200k_base` (원문 → BPE) | `cl100k_base` (원문 → BPE) | 엄밀 슬롯 보존 |
|----------------------------------------------------------|------:|--------------------------:|---------------------------:|---------------:|
| EN: [`NebulaByte/E-Commerce_Customer_Support_Conversations`](https://huggingface.co/datasets/NebulaByte/E-Commerce_Customer_Support_Conversations) | 50 | 23,979 → 11,538 (**-51.9%**) | 24,629 → 11,912 (**-51.6%**) | 124/124 |
| EN: [`bitext/Bitext-customer-support-llm-chatbot-training-dataset`](https://huggingface.co/datasets/bitext/Bitext-customer-support-llm-chatbot-training-dataset) | 50 | 11,431 → 3,390 (**-70.3%**) | 11,639 → 3,406 (**-70.7%**) | 281/281 |
| EN: [`FunDialogues/customer-service-robot-support`](https://huggingface.co/datasets/FunDialogues/customer-service-robot-support) | 50 | 3,146 → 1,548 (**-50.8%**) | 3,226 → 1,590 (**-50.7%**) | 0/0 (100%) |
| EN: [`Kaludi/Customer-Support-Responses`](https://huggingface.co/datasets/Kaludi/Customer-Support-Responses) | 50 | 1,837 → 893 (**-51.4%**) | 1,883 → 919 (**-51.2%**) | 1/1 |
| EN: [`nwirandx/Korean-OSCE-history-taking-dialogues`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) (`sessions`, 영문) | 50 | 1,462 → 608 (**-58.4%**) | 1,467 → 610 (**-58.4%**) | 11/11 |
| **영문 소계 (5개 데이터셋)** | **250** | **41,855 → 17,977 (-57.0%)** | **42,844 → 18,437 (-57.0%)** | **417/417 (100%)** |
| KO: [`iamjoon/ecommerce-function-calling-datasets-korean`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) | 50 | 19,243 → 7,774 (**-59.6%**) | 29,172 → 12,468 (**-57.3%**) | 220/220 |
| KO: [`jjun123/delivery-app-function-calling-datasets-korean`](https://huggingface.co/datasets/jjun123/delivery-app-function-calling-datasets-korean) | 30 | 42,129 → 18,182 (**-56.8%**) | 52,592 → 24,433 (**-53.5%**) | 325/325 |
| KO: [`jihye-moon/klac_legal_aid_counseling`](https://huggingface.co/datasets/jihye-moon/klac_legal_aid_counseling) | 50 | 27,325 → 12,357 (**-54.8%**) | 42,753 → 18,872 (**-55.9%**) | 465/465 |
| KO: [`seventy4/labor-counsel-qa`](https://huggingface.co/datasets/seventy4/labor-counsel-qa) | 50 | 5,788 → 2,498 (**-56.8%**) | 9,588 → 4,041 (**-57.9%**) | 38/38 |
| KO: [`schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1`](https://huggingface.co/datasets/schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1) | 50 | 7,437 → 3,119 (**-58.1%**) | 9,405 → 4,478 (**-52.4%**) | 176/176 |
| KO: [`nwirandx/Korean-OSCE-history-taking-dialogues`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) (`sessions`, 한국어) | 50 | 1,854 → 882 (**-52.4%**) | 2,791 → 1,304 (**-53.3%**) | 13/13 |
| KO: [`nayohan/141_korean_multi_session_dialogue`](https://huggingface.co/datasets/nayohan/141_korean_multi_session_dialogue) | 50 | 18,307 → 8,914 (**-51.3%**) | 28,868 → 14,252 (**-50.6%**) | 77/77 |
| **한국어 소계 (7개 데이터셋)** | **330** | **122,083 → 53,726 (-56.0%)** | **175,169 → 79,848 (-54.4%)** | **1,314/1,314 (100%)** |
| **전체 합계 (12개 데이터셋 벤치마크, 580개 채팅·상담 사례)** | **580** | **163,938 → 71,703 (-56.3%)** | **218,013 → 98,285 (-54.9%)** | **1,731/1,731 (100%)** |

## 규칙

- **한 번에 한 가지만 변경합니다.** 변경을 함께 적용하면 어떤 변경의 효과인지 구분할 수 없습니다.
- **되돌리기 조건**을 미리 정의합니다(예: 성공률이 노이즈 임계값보다 많이 떨어지는 경우).
- 조용한 동작 변경보다 **되돌릴 수 있고 눈에 보이는** 수정(설정/구성)을 선호합니다.
- 블랙박스 우선: 에이전트 자체를 수정할 수 없다면 구성, 지침, 도구 설정을 통해 대응합니다.

## 원인별 관련 연구

각 개선 방법과 관련된 발표 연구를 소개합니다. 학회 등급 순, 같은 행 안에서는 인용 수 순으로 정렬했습니다(전체 목록: [논문](../../papers/)). 각 연구는 *자신의* 에이전트에 대한 가설로 취급하고 [3단계](../03-prove/)에서 검증합니다.

| 원인 | 논문 |
|------|------|
| 저장소 사실 재학습 | [Agentic Context Engineering](https://arxiv.org/abs/2510.04618) (ICLR 2025); [Agent Workflow Memory](https://arxiv.org/abs/2409.07429) (ICML 2024); [TokenEconomizer](https://arxiv.org/abs/2610.22732) (프리프린트 2026) |
| 사용하지 않는 도구 / 도구 스키마 크기 | [MCP-Zero](https://arxiv.org/abs/2506.01056) (프리프린트 2025); [RAG-MCP](https://arxiv.org/abs/2505.03275) (프리프린트 2025); [Less is More (function calling)](https://arxiv.org/abs/2411.15399) (프리프린트 2024) |
| 실패 루프, 에이전트 설계 | [SWE-agent](https://arxiv.org/abs/2405.15793) (NeurIPS 2024); [Agentless](https://arxiv.org/abs/2407.01489) (프리프린트 2024); [Efficient Agents](https://arxiv.org/abs/2508.02694) (프리프린트 2025) |
| 비대한 도구 출력 / 궤적 | [SWE-agent](https://arxiv.org/abs/2405.15793) (NeurIPS 2024); [Trajectory Reduction](https://arxiv.org/abs/2509.23586) (PACMSE 2025); [The Complexity Trap (observation masking)](https://arxiv.org/abs/2508.21433) (프리프린트 2025) |
| 기록 증가 / 메모리 | [Mem0](https://arxiv.org/abs/2504.19413) (ECAI 2025); [Recursively Summarizing](https://arxiv.org/abs/2308.15022) (Neurocomputing 2023); [MemGPT](https://arxiv.org/abs/2310.08560) (프리프린트 2023); [The Complexity Trap (observation masking)](https://arxiv.org/abs/2508.21433) (프리프린트 2025) |
| 컨텍스트 열화 | [Lost in the Middle](https://arxiv.org/abs/2307.03172) (TACL 2023); [LLMs Get Lost in Multi-Turn Conversation](https://arxiv.org/abs/2505.06120) (ICLR 2025); [Same Task, More Tokens](https://arxiv.org/abs/2402.14848) (ACL 2024) |
| 접두부 변동 / 캐시 미스 | [SGLang](https://arxiv.org/abs/2312.07104) (NeurIPS 2023); [Prompt Cache](https://arxiv.org/abs/2311.04934) (MLSys 2023); [Don't Break the Cache](https://arxiv.org/abs/2601.06007) (프리프린트 2026) |
| 프롬프트 및 컨텍스트 압축 | [LongLLMLingua](https://arxiv.org/abs/2310.06839) (ACL 2023); [LLMLingua-2](https://arxiv.org/abs/2403.12968) (ACL 2024); [LLMLingua](https://arxiv.org/abs/2310.05736) (EMNLP 2023) |
| 과도하게 큰 모델 | [FrugalGPT](https://arxiv.org/abs/2305.05176) (TMLR 2023); [Hybrid LLM](https://arxiv.org/abs/2404.14618) (ICLR 2024); [RouteLLM](https://arxiv.org/abs/2406.18665) (프리프린트 2024) |
| 추론 길이 / 강도 | [Token-Budget-Aware Reasoning](https://arxiv.org/abs/2412.18547) (ACL 2024); [L1 (length-controlled reasoning)](https://arxiv.org/abs/2503.04697) (프리프린트 2025); [Chain of Draft](https://arxiv.org/abs/2502.18600) (프리프린트 2025) |
| 중복된 파이프라인 단계 / 불필요한 모델 크기 | [FrugalGPT](https://arxiv.org/abs/2305.05176) (TMLR 2023); [AutoMix](https://arxiv.org/abs/2310.12963) (NeurIPS 2023); [Agentless](https://arxiv.org/abs/2407.01489) (프리프린트 2024) |
| 대량 호출 | [Batch Prompting](https://arxiv.org/abs/2301.08721) (EMNLP 2023) |

## 다음 단계

[5. 결과](../05-result/)에서 다시 측정합니다.
