---
title: "6. 골든 데이터셋과 루브릭 평가"
description: "단일 턴·멀티턴 에이전트의 토큰 최적화 전후 품질을 검증하기 위한 엑셀(CSV) 골든 데이터셋 설계, 키워드·의도 하이브리드 채점, 루브릭 평가 방법입니다."
track: agent
verified: 2026-11-06
sidebar:
  order: 6
---

**목표:** 토큰을 줄인 뒤에도 에이전트의 답변 정확도와 멀티턴 문맥 유지력이 그대로인지 검증하는 **반복 가능한 품질 기준선**을 만듭니다.

---

## 1. 정답을 어떻게 판정할 것인가: 키워드 일치 vs 의도(의미) 파악

에이전트 응답을 채점할 때 단일 방식만 사용하면 오류가 발생합니다. 질문과 작업의 성격에 따라 **4가지 판정 기준**을 조합하는 **하이브리드(Hybrid) 채점**이 가장 정확하고 비용도 적게 듭니다.

| 판정 기준 | 언제 사용하는가? | 작동 방식 (예시) | 평가 토큰 비용 |
|-----------|------------------|------------------|----------------|
| **1. 필수 키워드·슬롯 일치 (`must_include_all`)** | 특정 식별자, 수치, 경계 조건, 파일 경로가 빠지면 오답인 경우 | `checkout.py;>= 100` (모두 포함되어야 통과) | **0 토큰** (문자열·정규식) |
| **2. 동의어 그룹 일치 (`must_include_any`)** | 같은 뜻을 여러 표현으로 말할 수 있지만 핵심 단어군 중 하나는 꼭 들어가야 하는 경우 | `평일\|영업일\|월요일;주말 제외\|토·일 제외` (그룹별 1개 이상 포함 시 통과) | **0 토큰** (문자열·정규식) |
| **3. 금지 키워드·금지 행동 (`must_not_include` / `expected_tools`)** | 환각 문구, 규정 위반 약속, 잘못된 명령어, 필수 도구 미호출을 차단할 때 | 금지어: `전액 환불해 드리겠습니다`, 필수 도구: `update_shipping_address` | **0 토큰** (로그·문자열) |
| **4. 의도 및 의미 루브릭 평가 (`expected_intent` + Rubric)** | 문장 표현이 완전히 달라도 질문 의도에 맞는 결론을 내렸는지 판단해야 할 때 | `"개봉 후 7일이 지나 단순 변심 환불은 불가함을 안내하고 대안(점검 접수)을 제시함"` | **소량 소비** (LLM-as-a-Judge) |

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row">
      <span>1단계 (결정적)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-hit" style="width:100%">필수 키워드(all) · 동의어(any) · 금지어(not) · 호출 도구(tools) 검사 (0 토큰)</div>
      </div>
      <span>즉시 판정</span>
    </div>
    <div class="at-diag-row">
      <span>2단계 (루브릭)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-prefix" style="width:72%">1단계 통과 건만 5대 루브릭 축(의도·사실/슬롯·멀티턴 문맥·도구/정책·간결성, 10점 만점) 채점</div>
        <div class="at-diag-seg seg-user" style="width:28%">길이 편향 차단</div>
      </div>
      <span>정밀 검증</span>
    </div>
  </div>
  <figcaption>
    그림 1. 비용이 들지 않는 결정적 키워드·도구 검사를 먼저 실행하고, 통과한 응답에 대해서만 의도 기반 루브릭(LLM-as-a-Judge)을 실행하는 2단계 하이브리드 평가 파이프라인.
  </figcaption>
</figure>

---

## 2. 실제 멀티턴(Multi-turn) 대화는 어떻게 평가해야 하는가?

멀티턴 대화(`Turn 1 → Turn 2 → Turn 3`)에서 토큰 최적화(컨텍스트 압축, 관찰 마스킹, 도구 가지치기 등)를 검증할 때 가장 흔히 겪는 문제는 **"1턴의 답변이 달라지면 2턴의 질문 맥락이 어긋나 전후 비교가 불가능해진다"**는 점입니다. 이를 해결하려면 다음 두 가지 방식을 구분해 사용합니다.

### 방식 A: 고정 문맥 주입 평가 (Pinned-History Slicing — 전후 비교 시 기본 권장)

- **방법**: `Turn 1 ~ Turn k-1`까지의 대화 기록과 도구 결과를 **고정된 텍스트(`prior_turns_context`)**로 넣어 주고, **측정 대상인 `Turn k`의 응답과 토큰 사용량만 비교**합니다.
- **멀티턴 기억 유지(Planted-Fact Retention) 검증**:
  - **1턴(Setup)**에 중요한 제약 조건을 심어 둡니다. 예: *"주문번호 `ORD-901` 배송지를 바꾸고 싶은데, **주말에는 택배를 절대 못 받아요**."*
  - **2턴(Memory Check)**에서는 그 제약을 다시 언급하지 않고 질문합니다. 예: *"서울시 강남구 테헤란로 123으로 변경해 주시고, 언제 도착하는지도 알려주세요."*
  - 컨텍스트를 압축하거나 프롬프트를 줄인 뒤에도 에이전트가 1턴의 **`ORD-901`**과 **`주말 수령 불가(평일/영업일 배송)`** 제약을 잃어버리지 않고 반영하는지 검사합니다.

### 방식 B: 엔드투엔드 시나리오 시뮬레이션 (`pass^k`)

- 전체 루프 단축이나 에이전트 자율 실행을 평가할 때는 사용자 역할을 하는 고정 시나리오가 최대 `N`턴 동안 에이전트와 대화하며 최종 목표 달성 여부와 총 세션 토큰을 측정합니다([tau-bench, arXiv:2406.12045](https://arxiv.org/abs/2406.12045)).

---

## 3. 엑셀(`.xlsx` / `.csv`)로 골든 데이터셋 정리·관리하기

엔지니어와 도메인 담당자(기획·QA·현업)가 함께 골든 데이터셋을 관리할 때는 중첩된 JSON보다 **엑셀 통합 문서(`.xlsx`)의 표(1행 = 평가 턴 1건, 같은 대화는 동일한 `session_id`에 `turn_index` 1~5로 연결)** 형태가 훨씬 효율적입니다.

- **실제 엑셀 파일 다운로드 (시트 2개: `Golden_Dataset` 16행 + `Rubric_Criteria` 루브릭 기준표)**:
  - [📥 `sample-golden-dataset.xlsx` 다운로드 (Excel 통합 문서)](https://github.com/dotnetpower/agent-tokenomics/raw/main/examples/sample-golden-dataset.xlsx)
  - [📥 `sample-golden-dataset.csv` 다운로드 (UTF-8 BOM CSV)](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-golden-dataset.csv) · [생성·채점 스크립트 `golden_dataset_tool.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/golden_dataset_tool.py)

| 열 이름 (Column) | 입력 형식 | 설명 및 작성 요령 |
|------------------|-----------|-------------------|
| `case_id` | `HF-ECOM5-T2` | 행 고유 ID (`세션ID-턴번호`) |
| `session_id` | `HF-ECOM5` | 멀티턴 대화를 묶는 세션 ID |
| `turn_index` | `1` ~ `5` | 해당 세션 내의 턴 순서 |
| `hf_dataset` / `hf_row` | `iamjoon/...` / `train[5]` | 원본 Hugging Face 데이터셋 이름 및 행 번호 |
| `slice` | `multi_turn_tool` | 평가 유형 (`multi_turn_tool`, `multi_turn_memory`, `policy_guardrail`, `legal_statute_qa`) |
| `match_mode` | `hybrid` | `keyword_only` / `intent_only` / `hybrid` |
| `prior_turns_context` | `[T1 User] ... [T1 Tool] ...` | 멀티턴인 경우 이전 턴까지의 고정 대화·도구 결과 요약 (1턴이면 빈칸) |
| `user_input` | 텍스트 | 현재 턴에 사용자가 입력하는 질문 또는 지시 |
| `must_include_all` | `;`로 구분 | 반드시 포함되어야 하는 키워드·숫자·식별자 (예: `5월 10일;무선 이어폰;4567891230`) |
| `must_include_any` | `;`로 그룹, `\|`로 동의어 | 그룹별 동의어 중 1개 이상 포함 (예: `희망 날짜\|희망하는 날짜;모의 서비스\|가상`) |
| `must_not_include` | `;`로 구분 | 포함되면 즉시 오답 처리할 금지 표현 (예: `제가 바로 결제 취소해 드렸습니다`) |
| `expected_tools` | `;`로 구분 | 해당 턴에 호출해야 하는 도구 이름 (예: `view_order_history;view_order_details`) |
| `expected_intent` | 한 줄 문장 | **의도 기반 채점 기준**: 표현이 달라도 어떤 뜻과 결론이 담겨야 정답인지 명시 |
| `baseline_response` / `baseline_tokens` | 텍스트 / 정수 | 최적화 전 에이전트 응답과 소비 토큰 수 |
| `candidate_response` / `candidate_tokens` | 텍스트 / 정수 | 최적화 후 에이전트 응답과 소비 토큰 수 |

### Hugging Face 공개 데이터셋 기반 16개 골든 데이터셋 예시 (`sample-golden-dataset.xlsx` 시트 1)

공개된 한국어 Hugging Face 데이터셋 6종에서 추출한 **실제 멀티턴(5턴 연속 세션·4턴 연속 세션·10턴 문진 요약) 및 단일 턴 16개 평가 행**입니다.

| 케이스 (턴) | Hugging Face 데이터셋 (`hf_row`) | 방식 / 필수 도구 | `must_include_all` (필수 슬롯) | `must_include_any` (동의어 그룹) | `expected_intent` (의도 채점 기준) |
|---|---|---|---|---|---|
| **`HF-ECOM5-T1`** (1/5) | [`iamjoon/ecommerce-...`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) (`train[5]`) | `hybrid`<br>`view_order_history` | `5월 10일`<br>`무선 이어폰`<br>`4567891230` | `한진택배`<br>`배송 중\|배송중` | `U002`의 5월 10일 무선 이어폰 주문이 한진택배(`4567891230`)로 배송 중임을 안내 |
| **`HF-ECOM5-T2`** (2/5) | `iamjoon/ecommerce-...` (`train[5]`) | `hybrid`<br>`view_user_profile` | `Gold`<br>`10%` | `할인 쿠폰\|쿠폰` | 1턴의 `U002` 문맥을 유지해 `Gold` 등급 및 `10%` 할인 쿠폰 보유 사실을 안내 |
| **`HF-ECOM5-T3`** (3/5) | `iamjoon/ecommerce-...` (`train[5]`) | `hybrid`<br>`search_policy_info` | `7일`<br>`왕복 배송비` | `수령 후\|수령일`<br>`고객센터\|신청` | 상품 수령 후 `7일` 이내 환불 신청 가능 조건과 왕복 배송비 규정을 안내 |
| **`HF-ECOM5-T4`** (4/5) | `iamjoon/ecommerce-...` (`train[5]`) | `hybrid`<br>`search_policy_info` | `반품` | `출고되기 전\|출고 전`<br>`출고 이후\|출고 후`<br>`주문취소 버튼\|주문내역` | 출고 전에는 주문내역에서 즉시 취소 가능하고 출고 후에는 반품으로 처리됨을 구분 설명 |
| **`HF-ECOM5-T5`** (5/5) | `iamjoon/ecommerce-...` (`train[5]`) | `intent_only`<br>*(도구 없음)* | `담당자` | `직접 처리할 수 없\|직접 처리 불가`<br>`전달` | AI 상담사가 직접 결제 취소할 권한이 없음을 밝히고 담당자에게 전달해 처리하겠다고 안내 |
| **`HF-ECOM2-T1`** (1/4) | `iamjoon/ecommerce-...` (`train[2]`) | `hybrid`<br>`show_cart` | `블루투스 이어폰`<br>`무선 마우스` | `1개;2개` | `U001` 장바구니에 블루투스 이어폰 1개와 무선 마우스 2개가 담겨 있음을 안내 |
| **`HF-ECOM2-T2`** (2/4) | `iamjoon/ecommerce-...` (`train[2]`) | `hybrid`<br>`view_order_history`<br>`view_order_details` | `고성능 노트북`<br>`1,150,000원` | `1대;구매` | 1턴의 `U001`을 기억해 주문 이력·상세(`O102`)를 연쇄 호출하고 고성능 노트북(`1,150,000원`) 확인 |
| **`HF-ECOM2-T3`** (3/4) | `iamjoon/ecommerce-...` (`train[2]`) | `intent_only`<br>*(도구 없음)* | `담당자` | `기술적인 지원\|기술 지원`<br>`전달` | 노트북 전원 꺼짐 증상이 기술 지원 대상임을 안내하고 담당 부서 전달 약속 |
| **`HF-ECOM2-T4`** (4/4) | `iamjoon/ecommerce-...` (`train[2]`) | `hybrid`<br>*(도구 없음)* | `카드 영수증`<br>`담당자` | `직접 처리할 수 없\|직접 처리 불가`<br>`전달` | 카드 영수증 발급은 직접 처리 불가함을 밝히고 담당자에게 전달하겠다고 안내 |
| **`HF-DELIV-T1`** (1/1) | [`jjun123/delivery-app-...`](https://huggingface.co/datasets/jjun123/delivery-app-function-calling-datasets-korean) (`train[0]`) | `hybrid`<br>`create_delivery_order` | `14,900원;7,000원`<br>`21,900원;이서준`<br>`e2a4b1c9-...` | `관악구 관악로 1`<br>`준비 중\|준비중`<br>`담당자` | 품목·총액(`21,900원`), 수령인(`이서준`), 주문 UUID, 매장 준비 중 안내 및 도착 시간 담당자 전달 |
| **`HF-DFM14-T1`** (1/1) | [`schneiderkamplab/dfm14-...`](https://huggingface.co/datasets/schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1) (`train[2]`) | `keyword_only`<br>`lookup_parcel`<br>`redirect_parcel` | `parcel-610`<br>`locker-16`<br>`redirect-610` | `변경 처리가 완료\|변경 완료` | `tracking-610` 조회로 `parcel-610` 확인 후 `locker-16` 보관함 변경 및 `redirect-610` 반환 |
| **`HF-CLINIC-T1`** (1/1) | `schneiderkamplab/dfm14-...` (`train[11]`) | `hybrid`<br>*(도구 없음)* | `병원 이름`<br>`서비스 종류` | `희망 날짜\|희망하는 날짜`<br>`모의 서비스\|가상`<br>`예약 가능 시간대` | 실제 예약 확정·의료 상담을 하지 않는 가상 모의 서비스임을 밝히고 시간대 조회만 가능함을 안내 |
| **`HF-KLAC-T1`** (1/1) | [`jihye-moon/klac_legal_aid...`](https://huggingface.co/datasets/jihye-moon/klac_legal_aid_counseling) (`train[1]`) | `hybrid`<br>*(도구 없음)* | `제34조;제4조`<br>`1년;4주;15시간` | `근로기준법`<br>`근로자퇴직급여 보장법`<br>`퇴직연금` | 근로기준법 제34조·퇴직급여법 제4조 및 `1년 미만`·`4주 평균 주 15시간 미만` 제외 요건 제시 |
| **`HF-KLAC-T2`** (1/1) | `jihye-moon/klac_legal_aid...` (`train[0]`) | `hybrid`<br>*(도구 없음)* | `최저임금법`<br>`제6조;무효;90%` | `3월\|3개월`<br>`수습` | 최저임금 미달 합의 무효(제6조) 및 수습 `3개월` 이내 최저임금 `90%` 지급 가능 요건 설명 |
| **`HF-LABOR-T1`** (1/1) | [`seventy4/labor-counsel-qa`](https://huggingface.co/datasets/seventy4/labor-counsel-qa) (`train[34]`) | `hybrid`<br>*(도구 없음)* | `30일;30일분`<br>`통상임금;3개월` | `천재·사변\|부득이한 사유`<br>`예외` | `30일` 전 해고예고 또는 `30일분` 이상 통상임금 지급 의무와 `3개월` 미만·`천재·사변` 예외 안내 |
| **`HF-OSCE-T1`** (2/2) | [`nwirandx/Korean-OSCE-...`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) (`sessions[0..9]`) | `hybrid`<br>*(도구 없음)* | `김서현`<br>`1984년 5월 23일`<br>`20일;15분;20분;30분` | `과거력 없음\|과거력은 없`<br>`어지럽고\|어지러움`<br>`숨이 차\|호흡곤란` | 1~9턴 문진 내역(환자 정보, 20일 전 발병, 15~20분→30분 이상 악화, 과거력 없음, 동반 증상) 요약 |

---

## 4. 루브릭(Rubric) 평가 항목 재검토 및 설계 (5개 축 × 0·1·2점 = 10점 만점)

단일 0~10점 자유 척도는 채점 모델(LLM-as-a-Judge)이 실행할 때마다 점수가 흔들립니다. 반면 에이전트의 실패 원인을 **5개의 독립된(Orthogonal) 평가 축 × 각 0·1·2점 = 총 10점 만점**으로 분리하면, **현재 턴의 수치/슬롯 왜곡(축 2)**과 **이전 턴의 멀티턴 문맥 망각(축 3)**, 그리고 **도구 호출 및 정책 위반(축 4)**을 섞이지 않게 정확히 진단할 수 있습니다.

| 평가 축 (5 Axes × 2점 = 10점 만점) | 0점 (Fail — 치명적 결함) | 1점 (Partial — 부분 충족) | 2점 (Pass — 완전 충족) | 왜 이 항목이 필요한가? |
|---|---|---|---|---|
| **1. 질문 의도 및 핵심 결론 정확성**<br>(`intent_and_resolution`) | 질문 의도를 오해했거나 `expected_intent`와 반대·무관한 결론을 냄 | 의도는 파악했으나 핵심 결론이나 필요한 후속 조치 안내가 일부 빠짐 | 표현 방식과 무관하게 `expected_intent`의 핵심 결론과 해결책을 정확히 제시함 | 단어 선택이 달라도 의미·결론이 맞는지 평가 (`intent_only` / `hybrid`) |
| **2. 필수 사실·슬롯 및 수치 정확성**<br>(`fact_and_slot_accuracy`) | 필수 식별자·수치·법조문을 누락·왜곡했거나 없는 사실(환각)을 지어냄 | 주요 사실은 맞으나 부차적 수치·조건 표현이 모호하거나 일부 빠짐 | 모든 필수 키워드·수치·식별자·법조문을 왜곡이나 환각 없이 정확히 반영함 | 프롬프트/출력 압축 시 숫자·ID·법조문 손실 감시 (`keyword` / `hybrid`) |
| **3. 멀티턴 문맥 및 상태 유지력**<br>(`multiturn_context_retention`) | 이전 턴(`prior_turns_context`)의 제약·식별자를 잊어 모순되거나 재질문함 | 이전 턴 정보를 일부만 반영하거나 문맥 연결이 불완전함 | 이전 턴에서 누적된 고객 ID·제약 조건·대화 상태를 누락 없이 일관되게 유지함 | 컨텍스트 압축(`/compact`)·관찰 마스킹 후 멀티턴 기억 유지 검증 |
| **4. 도구 선택 및 정책·가드레일 준수**<br>(`tool_and_policy_compliance`) | 필수 도구를 호출하지 않고 결과를 지어내거나 금지어(`must_not_include`)·권한 밖 약속 수행 | 안전·정책 한계는 지켰으나 불필요한 도구를 중복 호출했거나 형식 제약을 어김 | 필요한 도구만 정확히 호출(권한 밖 작업은 담당자 전달)하고 모든 정책·금지 조건 준수 | 도구 가지치기 및 정책 프롬프트 축소 후 안전성·함수 호출 검증 |
| **5. 간결한 완결성 (길이 편향 방지)**<br>(`concise_completeness`) | 필수 설명이 지나치게 생략되어 사용자가 재질문해야 하거나 문장이 깨짐 | 필요한 정보는 담겼으나 불필요한 인사말·사과·질문 되풀이로 토큰을 낭비함 | **필수 사실과 의도를 모두 충족하면서 군더더기 없이 간결하고 명확함 (짧은 답변 만점)** | **LLM Judge의 긴 답변 선호 편향(Verbosity Bias) 차단** |

> **권장 통과 기준**: 1단계 결정적 검사(`must_include_all`, `must_include_any`, `must_not_include`, `expected_tools`)를 모두 통과하고, 2단계 루브릭 5개 축 합계가 **8점 이상(10점 만점)**이며 **어느 한 축도 `0점`이 아닐 때** 최종 `PASS`로 판정합니다.

---

## 5. 엑셀(`.xlsx` / `.csv`) 생성 및 하이브리드 채점 파이썬 코드

[`examples/golden_dataset_tool.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/golden_dataset_tool.py)는 외부 패키지 설치 없이 표준 라이브러리만으로 **2개 시트(`Golden_Dataset`, `Rubric_Criteria` 10점 만점 기준표)와 헤더 서식·열 너비가 적용된 실제 엑셀 파일([`examples/sample-golden-dataset.xlsx`](https://github.com/dotnetpower/agent-tokenomics/raw/main/examples/sample-golden-dataset.xlsx))** 및 [`examples/sample-golden-dataset.csv`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-golden-dataset.csv)를 생성하고, `.xlsx` 또는 `.csv`를 직접 읽어 16개 Hugging Face 케이스를 평가합니다.

```bash
# 1. 2개 시트(Golden_Dataset + Rubric_Criteria 10점 만점 기준표)가 포함된 실제 .xlsx 및 .csv 생성
python3 examples/golden_dataset_tool.py --write-sample

# 2. 생성된 엑셀(.xlsx) 파일을 직접 읽어 Baseline vs Candidate 하이브리드 채점 실행
python3 examples/golden_dataset_tool.py --eval examples/sample-golden-dataset.xlsx

# 3. 특정 멀티턴 케이스(HF-ECOM5-T5)의 10점 만점 LLM-as-a-Judge 루브릭 채점 프롬프트 출력
python3 examples/golden_dataset_tool.py --print-judge-prompt HF-ECOM5-T5
```

### 실제 실행 결과 (`examples/sample-golden-dataset.xlsx`, 16개 행)

```text
case_id      turn hf_dataset                         base_ok cand_ok base_tok cand_tok   delta rubric
--------------------------------------------------------------------------------------------------------
HF-ECOM5-T1     1 iamjoon/ecommerce-function-calli   PASS    PASS       4,200    2,350  -44.0%  10/10
HF-ECOM5-T2     2 iamjoon/ecommerce-function-calli   PASS    PASS       5,800    3,100  -46.6%  10/10
HF-ECOM5-T3     3 iamjoon/ecommerce-function-calli   PASS    PASS       7,400    3,850  -48.0%  10/10
HF-ECOM5-T4     4 iamjoon/ecommerce-function-calli   PASS    PASS       9,100    4,500  -50.5%  10/10
HF-ECOM5-T5     5 iamjoon/ecommerce-function-calli   PASS    PASS      10,800    4,950  -54.2%  10/10
HF-ECOM2-T1     1 iamjoon/ecommerce-function-calli   PASS    PASS       4,100    2,200  -46.3%  10/10
HF-ECOM2-T2     2 iamjoon/ecommerce-function-calli   PASS    PASS       6,900    3,600  -47.8%  10/10
HF-ECOM2-T3     3 iamjoon/ecommerce-function-calli   PASS    PASS       8,300    4,100  -50.6%  10/10
HF-ECOM2-T4     4 iamjoon/ecommerce-function-calli   PASS    PASS       9,600    4,450  -53.6%  10/10
HF-DELIV-T1     1 jjun123/delivery-app-function-ca   PASS    PASS      12,400    6,800  -45.2%  10/10
HF-DFM14-T1     1 schneiderkamplab/dfm14-ko-synthe   PASS    PASS       5,400    2,900  -46.3%  10/10
HF-CLINIC-T1    1 schneiderkamplab/dfm14-ko-synthe   PASS    PASS       4,900    2,700  -44.9%  10/10
HF-KLAC-T1      1 jihye-moon/klac_legal_aid_counse   PASS    PASS       6,400    3,300  -48.4%  10/10
HF-KLAC-T2      1 jihye-moon/klac_legal_aid_counse   PASS    PASS       5,900    3,100  -47.5%  10/10
HF-LABOR-T1     1 seventy4/labor-counsel-qa          PASS    PASS       5,200    2,850  -45.2%  10/10
HF-OSCE-T1      2 nwirandx/Korean-OSCE-history-tak   PASS    PASS       9,400    4,600  -51.1%  10/10
--------------------------------------------------------------------------------------------------------
Pass rate          : Baseline 16/16 (100.0%) -> Candidate 16/16 (100.0%)
Mean rubric score  : Baseline 9.62/10 -> Candidate 10.00/10
Total tokens       : Baseline 115,800 -> Candidate 59,350 (-48.7%)
Cost-of-Pass (tok) : Baseline 7,238 tok/pass -> Candidate 3,709 tok/pass
Decision           : KEEP (Quality held & tokens reduced)
```

---

## 6. 예시: `tokprof`로 추출한 프롬프트로 골든 데이터셋 초안 만들기

`user_input`, `prior_turns_context`, `baseline_response`를 손으로 채우는 일은 느리고 오류가 잘 납니다. 에이전트가 이미 동작하고 있다면 [`tokprof`](../tokprof/)가 실제 실행의 프롬프트와 응답(비밀값은 마스킹)을 그대로 기록하므로, 사람은 "정답이라면 무엇이 들어 있어야 하는가"라는 판단만 추가하면 됩니다.

| 단계 | 담당 | 작업 | 산출물 |
|------|------|------|--------|
| 1 | 엔지니어 | **baseline** 에이전트로 대표 시나리오 1개를 `tokprof` 아래에서 실행하고 프롬프트를 CSV로 내보냅니다 | `baseline-run.csv` |
| 2 | 스크립트 | 사용자 턴당 1행으로 변환합니다 (이전 대화, 사용자 입력, baseline 응답, baseline 토큰) | `draft.csv` |
| 3 | 도메인 담당자 | 행마다 `must_include_all` 후보를 확정·수정하고, `must_include_any` / `must_not_include` / `expected_tools`를 추가하고, `expected_intent`를 작성합니다 | 검수된 데이터셋 |
| 4 | 엔지니어 | 4단계 개선안을 **하나만** 적용하고 **같은** 사용자 입력으로 다시 실행해 내보낸 뒤, `case_id` 기준으로 새 응답을 병합합니다 | `golden.csv` |
| 5 | 채점 | 4장의 10점 루브릭으로 두 응답을 채점하고 `--eval`을 실행합니다 | PASS / FAIL 및 토큰 증감 |

```bash
# 1. baseline 실행을 캡처하고 프롬프트를 CSV로 내보내기 (OS·언어 무관)
tokprof -- python my_agent.py
tokprof report --csv            # .tokprof/latest.csv 생성 (또는: --format csv --out baseline-run.csv)

# 2. 초안 데이터셋 생성 (사용자 턴당 1행)
python3 examples/tokprof_to_golden.py baseline-run.csv -o draft.csv

# 3. draft.csv를 편집(엑셀로 열어도 됨)한 뒤, 개선안 1개를 적용하고 같은 시나리오를 다시 캡처
tokprof -- python my_agent.py
python3 examples/tokprof_to_golden.py candidate-run.csv --merge draft.csv -o golden.csv

# 4. 특정 행의 Judge 프롬프트를 출력해 두 응답을 채점한 뒤 평가
python3 examples/golden_dataset_tool.py --print-judge-prompt S1-T2
python3 examples/golden_dataset_tool.py --eval golden.csv
```

[`examples/tokprof_to_golden.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/tokprof_to_golden.py)가 `tokprof` CSV(`row_type=call` 행)를 변환하는 규칙입니다.

| `tokprof` 열 | 골든 데이터셋 열 | 비고 |
|--------------|------------------|------|
| `session` + `turn` | `case_id`(`S1-T2`), `session_id`, `turn_index` | `tokprof`는 유휴 시간 간격으로 호출을 턴으로 묶으므로 시나리오 단계마다 턴이 나뉘도록 실행해야 합니다 |
| `conversation_history` | `prior_turns_context` | 고정 이전 대화 (방식 A: 이전 대화 고정 슬라이싱) |
| `user_prompt` (턴의 첫 호출) | `user_input` | |
| `assistant_response` (턴의 마지막 호출) | `baseline_response` / `candidate_response` | 도구 호출 중간 단계가 아닌 최종 답변 |
| 턴 내 모든 호출의 `input_total` + `output_tokens` 합계 | `baseline_tokens` / `candidate_tokens` | 에이전트 루프의 모든 호출이 과금되므로 턴 합계가 비용입니다 |
| 응답 속 숫자형 토큰 (예: `4567891230`) | `must_include_all` (**제안값**) | 하나씩 확인하고, 실행마다 달라지는 값은 삭제합니다 |

**캡처 데이터 사용 시 규칙**

- 캡처된 응답은 baseline의 *동작*일 뿐 *정답*이 아닙니다. baseline이 틀렸다면 그대로 정답이 되어 버리므로, 실제 요구사항을 기준으로 사람이 `expected_intent`와 `must_*`를 검증한 뒤에 사용합니다.
- `tokprof`는 비밀값을 기본으로 마스킹하지만 이름·주문번호 같은 개인정보는 제거하지 않습니다. CSV를 외부로 내보내기 전에 치환하거나 마스킹하고, 필요하면 `tokprof`의 프롬프트 제외 모드도 검토합니다.
- 생성된 `rubric_score` 열은 비어 있어 `0`으로 계산되므로, 채점자가 두 점수를 채우기 전까지 모든 행이 `FAIL`로 표시됩니다. `Decision` 줄도 그 이후에만 의미가 있습니다.
- candidate 실행에서도 같은 입력을 재생해야 합니다. 에이전트가 후속 질문을 스스로 결정하는 경우에는 2장의 방식 B(`pass^k`)를 사용합니다.

---

## 관련 연구(논문)

- [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) — 프리프린트 2025. 단일 호출 비용이 아니라 성공한 작업 1건당 기대 비용(`Cost-of-Pass`)으로 모델을 평가합니다.
- [$\tau$-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045) — 프리프린트 2024. 멀티턴 도구 호출과 정책 준수를 반복 실행 일관성(`pass^k`)으로 평가합니다.
- [AI Agents That Matter](https://arxiv.org/abs/2407.01502) — 프리프린트 2024. 에이전트 벤치마크에서 비용과 정확도를 함께 보고하고 데이터 오염을 방지하는 평가 원칙을 정리합니다.
- [Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations](https://arxiv.org/abs/2411.00640) — 프리프린트 2024. 대응 표본 평가에서 신뢰구간과 표본 크기를 계산하는 방법을 다룹니다.
- [LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory](https://arxiv.org/abs/2410.10813) — ICLR 2025. 멀티턴 대화에서 이전 턴에 심어 둔 사실 회상과 갱신 능력을 측정합니다.
