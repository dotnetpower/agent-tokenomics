#!/usr/bin/env python3
"""Generate an Excel (.xlsx & .csv) Golden Dataset from Hugging Face datasets and run hybrid evaluation.

Includes 16 single-turn and multi-turn evaluation rows drawn from 6 public Hugging Face datasets:
  - iamjoon/ecommerce-function-calling-datasets-korean (train[5] 5-turn session, train[2] 4-turn session)
  - jjun123/delivery-app-function-calling-datasets-korean (train[0])
  - schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1 (train[2] 2-turn session, train[11])
  - jihye-moon/klac_legal_aid_counseling (train[0], train[1])
  - seventy4/labor-counsel-qa (train[2,3], train[34])
  - nwirandx/Korean-OSCE-history-taking-dialogues (sessions[0..9] 2-turn intake)

Generates a native two-sheet Microsoft Excel workbook (`sample-golden-dataset.xlsx`:
Sheet 1 = `Golden_Dataset`, Sheet 2 = `Rubric_Criteria`) and a `utf-8-sig` CSV using only
the Python standard library.

Usage:
    python3 examples/golden_dataset_tool.py --write-sample
    python3 examples/golden_dataset_tool.py --eval examples/sample-golden-dataset.xlsx
    python3 examples/golden_dataset_tool.py --print-judge-prompt HF-ECOM5-T5
"""

from __future__ import annotations

import argparse
import csv
from pathlib import Path
import shutil
from typing import Any
import xml.etree.ElementTree as ET
from xml.sax.saxutils import escape as xml_escape
import zipfile

HERE = Path(__file__).parent
DEFAULT_XLSX = HERE / "sample-golden-dataset.xlsx"
DEFAULT_CSV = HERE / "sample-golden-dataset.csv"
PUBLIC_DEMO_DIR = HERE.parent / "site" / "public" / "demo"

CSV_COLUMNS = [
    "case_id",
    "session_id",
    "turn_index",
    "hf_dataset",
    "hf_row",
    "slice",
    "match_mode",
    "prior_turns_context",
    "user_input",
    "must_include_all",
    "must_include_any",
    "must_not_include",
    "expected_tools",
    "expected_intent",
    "baseline_response",
    "baseline_tools",
    "baseline_tokens",
    "baseline_rubric_score",
    "candidate_response",
    "candidate_tools",
    "candidate_tokens",
    "candidate_rubric_score",
]

SAMPLE_ROWS: list[dict[str, Any]] = [
    # --- Session 1: 5-turn E-commerce multi-turn dialogue (iamjoon/ecommerce-function-calling-datasets-korean train[5]) ---
    {
        "case_id": "HF-ECOM5-T1",
        "session_id": "HF-ECOM5",
        "turn_index": 1,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[5] (T1/5)",
        "slice": "multi_turn_tool",
        "match_mode": "hybrid",
        "prior_turns_context": "",
        "user_input": "안녕하세요. 최근 주문한 상품 배송 상태 좀 확인하고 싶어요. (user_id=U002)",
        "must_include_all": "5월 10일;무선 이어폰;4567891230",
        "must_include_any": "한진택배;배송 중|배송중",
        "must_not_include": "배송 완료;취소 완료",
        "expected_tools": "view_order_history",
        "expected_intent": "view_order_history(U002)를 호출해 5월 10일 주문한 무선 이어폰이 한진택배(송장 4567891230)로 배송 중임을 안내함",
        "baseline_response": "고객님께서 5월 10일에 주문하신 '무선 이어폰'은 현재 배송 중이며, 택배사는 한진택배, 송장번호는 4567891230입니다.",
        "baseline_tools": "view_order_history",
        "baseline_tokens": 4200,
        "baseline_rubric_score": 10,
        "candidate_response": "5월 10일 주문하신 무선 이어폰은 현재 배송 중입니다(한진택배, 송장번호: 4567891230).",
        "candidate_tools": "view_order_history",
        "candidate_tokens": 2350,
        "candidate_rubric_score": 10,
    },
    {
        "case_id": "HF-ECOM5-T2",
        "session_id": "HF-ECOM5",
        "turn_index": 2,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[5] (T2/5)",
        "slice": "multi_turn_tool",
        "match_mode": "hybrid",
        "prior_turns_context": "[T1 User] 최근 주문 상품 배송 상태 확인 (U002)\n[T1 Tool] view_order_history(U002) -> 5월 10일 무선 이어폰, 한진택배 4567891230 배송중",
        "user_input": "감사합니다. 그리고 제 멤버십 등급도 알고 싶어요.",
        "must_include_all": "Gold;10%",
        "must_include_any": "할인 쿠폰|쿠폰",
        "must_not_include": "VIP;실버",
        "expected_tools": "view_user_profile",
        "expected_intent": "이전 턴의 사용자 ID(U002) 문맥을 유지하여 view_user_profile(U002)를 호출하고 Gold 등급 및 10% 할인 쿠폰 보유 사실을 안내함",
        "baseline_response": "고객님의 현재 멤버십 등급은 Gold입니다. 현재 10% 할인 쿠폰도 보유하고 계십니다.",
        "baseline_tools": "view_user_profile",
        "baseline_tokens": 5800,
        "baseline_rubric_score": 10,
        "candidate_response": "현재 멤버십 등급은 Gold이며, 10% 할인 쿠폰을 보유하고 계십니다.",
        "candidate_tools": "view_user_profile",
        "candidate_tokens": 3100,
        "candidate_rubric_score": 10,
    },
    {
        "case_id": "HF-ECOM5-T3",
        "session_id": "HF-ECOM5",
        "turn_index": 3,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[5] (T3/5)",
        "slice": "multi_turn_policy",
        "match_mode": "hybrid",
        "prior_turns_context": "[T1] U002 5월 10일 무선 이어폰 배송중(한진 4567891230)\n[T2] 멤버십 등급 Gold, 10% 할인 쿠폰 보유",
        "user_input": "환불 정책도 좀 알려주세요.",
        "must_include_all": "7일;왕복 배송비",
        "must_include_any": "수령 후|수령일;고객센터|신청",
        "must_not_include": "무료 반품;30일 이내",
        "expected_tools": "search_policy_info",
        "expected_intent": "search_policy_info(환불)를 호출해 상품 수령 후 7일 이내 신청 가능 조건과 왕복 배송비 부과 규정을 안내함",
        "baseline_response": "환불은 상품 수령 후 7일 이내에 신청 가능하며, 왕복 배송비가 부과될 수 있습니다. 환불 신청은 상준몰 고객센터를 통해 가능합니다.",
        "baseline_tools": "search_policy_info",
        "baseline_tokens": 7400,
        "baseline_rubric_score": 10,
        "candidate_response": "환불은 상품 수령 후 7일 이내에 고객센터에서 신청 가능하며 왕복 배송비가 부과될 수 있습니다.",
        "candidate_tools": "search_policy_info",
        "candidate_tokens": 3850,
        "candidate_rubric_score": 10,
    },
    {
        "case_id": "HF-ECOM5-T4",
        "session_id": "HF-ECOM5",
        "turn_index": 4,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[5] (T4/5)",
        "slice": "multi_turn_policy",
        "match_mode": "hybrid",
        "prior_turns_context": "[T1] U002 무선 이어폰 배송중\n[T2] Gold 등급\n[T3] 환불: 수령 후 7일 이내, 왕복 배송비",
        "user_input": "알겠습니다. 그런데 결제 취소는 어떻게 하나요?",
        "must_include_all": "반품",
        "must_include_any": "출고되기 전|출고 전;출고 이후|출고 후;주문취소 버튼|주문내역",
        "must_not_include": "출고 후에도 즉시 취소 가능",
        "expected_tools": "search_policy_info",
        "expected_intent": "search_policy_info(주문 취소)를 호출해 출고 전에는 주문내역에서 언제든 취소 가능하고 출고 후에는 반품으로 처리됨을 구분해 설명함",
        "baseline_response": "주문 취소는 상품이 출고되기 전에는 언제든지 가능하며, 출고 이후에는 반품으로 처리됩니다. 주문내역에서 주문취소 버튼을 통해 취소할 수 있습니다.",
        "baseline_tools": "search_policy_info",
        "baseline_tokens": 9100,
        "baseline_rubric_score": 10,
        "candidate_response": "출고 전에는 주문내역의 주문취소 버튼으로 언제든 취소 가능하며, 출고 이후에는 반품으로 처리됩니다.",
        "candidate_tools": "search_policy_info",
        "candidate_tokens": 4500,
        "candidate_rubric_score": 10,
    },
    {
        "case_id": "HF-ECOM5-T5",
        "session_id": "HF-ECOM5",
        "turn_index": 5,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[5] (T5/5)",
        "slice": "multi_turn_memory",
        "match_mode": "intent_only",
        "prior_turns_context": "[T1] U002 무선 이어폰 현재 배송중(출고 완료 상태)\n[T3] 환불: 수령 후 7일 이내\n[T4] 주문 취소: 출고 전만 직접 가능, 출고 후 반품 처리",
        "user_input": "그럼 결제 취소는 상담사님이 직접 못하나요?",
        "must_include_all": "담당자",
        "must_include_any": "직접 처리할 수 없|직접 처리 불가;전달",
        "must_not_include": "제가 바로 결제 취소해 드렸습니다",
        "expected_tools": "",
        "expected_intent": "AI 상담사가 결제를 직접 취소할 권한이 없음을 명확히 밝히고 담당자에게 요청을 전달해 처리하겠다고 안내함",
        "baseline_response": "네, 결제 취소는 직접 처리할 수 없으며, 담당자에게 요청 내용을 전달하여 신속히 처리하겠습니다. 더 궁금하신 사항이 있으신가요?",
        "baseline_tools": "",
        "baseline_tokens": 10800,
        "baseline_rubric_score": 9,
        "candidate_response": "네, 결제 취소는 제가 직접 처리할 수 없어 담당자에게 전달해 신속히 처리하겠습니다.",
        "candidate_tools": "",
        "candidate_tokens": 4950,
        "candidate_rubric_score": 10,
    },
    # --- Session 2: 4-turn Cart + Laptop Defect + Receipt Escalation (iamjoon/ecommerce-function-calling-datasets-korean train[2]) ---
    {
        "case_id": "HF-ECOM2-T1",
        "session_id": "HF-ECOM2",
        "turn_index": 1,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[2] (T1/4)",
        "slice": "multi_turn_tool",
        "match_mode": "hybrid",
        "prior_turns_context": "",
        "user_input": "안녕하세요. 장바구니에 뭐가 들어있는지 좀 볼 수 있을까요? (user_id=U001)",
        "must_include_all": "블루투스 이어폰;무선 마우스",
        "must_include_any": "1개;2개",
        "must_not_include": "장바구니가 비어 있습니다",
        "expected_tools": "show_cart",
        "expected_intent": "show_cart(U001)를 호출해 블루투스 이어폰 1개와 무선 마우스 2개가 담겨 있음을 정확히 안내함",
        "baseline_response": "고객님의 장바구니에는 '블루투스 이어폰' 1개와 '무선 마우스' 2개가 담겨 있습니다.",
        "baseline_tools": "show_cart",
        "baseline_tokens": 4100,
        "baseline_rubric_score": 10,
        "candidate_response": "장바구니에 블루투스 이어폰 1개와 무선 마우스 2개가 담겨 있습니다.",
        "candidate_tools": "show_cart",
        "candidate_tokens": 2200,
        "candidate_rubric_score": 10,
    },
    {
        "case_id": "HF-ECOM2-T2",
        "session_id": "HF-ECOM2",
        "turn_index": 2,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[2] (T2/4)",
        "slice": "multi_turn_tool",
        "match_mode": "hybrid",
        "prior_turns_context": "[T1 User] 장바구니 확인 (U001)\n[T1 Assistant] 블루투스 이어폰 1개, 무선 마우스 2개",
        "user_input": "아, 그리고 최근에 주문한 노트북에 문제가 생겼어요. 상세 내역 좀 볼 수 있을까요?",
        "must_include_all": "고성능 노트북;1,150,000원",
        "must_include_any": "1대;구매",
        "must_not_include": "주문 내역이 없습니다",
        "expected_tools": "view_order_history;view_order_details",
        "expected_intent": "1턴의 U001 ID를 기억해 주문 이력(view_order_history)과 상세 내역(view_order_details, O102)을 연쇄 호출하고 고성능 노트북 1대(1,150,000원) 구매 내역을 안내함",
        "baseline_response": "'고성능 노트북' 1대를 1,150,000원에 구매하셨습니다. 문제가 발생한 부분에 대해 더 도와드릴까요?",
        "baseline_tools": "view_order_history;view_order_details",
        "baseline_tokens": 6900,
        "baseline_rubric_score": 10,
        "candidate_response": "'고성능 노트북' 1대(1,150,000원) 구매 내역이 확인되었습니다. 어떤 문제가 발생했는지 알려주시면 도와드리겠습니다.",
        "candidate_tools": "view_order_history;view_order_details",
        "candidate_tokens": 3600,
        "candidate_rubric_score": 10,
    },
    {
        "case_id": "HF-ECOM2-T3",
        "session_id": "HF-ECOM2",
        "turn_index": 3,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[2] (T3/4)",
        "slice": "multi_turn_memory",
        "match_mode": "intent_only",
        "prior_turns_context": "[T1] U001 장바구니 조회\n[T2] O102 고성능 노트북 1대 1,150,000원 구매 확인",
        "user_input": "네, 노트북이 자꾸 꺼져서요. 이거 어떻게 해야 하나요?",
        "must_include_all": "담당자",
        "must_include_any": "기술적인 지원|기술 지원;전달",
        "must_not_include": "직접 분해해 보세요;환불 처리 완료",
        "expected_tools": "",
        "expected_intent": "전원 꺼짐 증상이 기술 지원 대상임을 안내하고 담당 부서에 전달해 신속히 처리하겠다고 응답함",
        "baseline_response": "불편을 드려 죄송합니다. 해당 문제는 기술적인 지원이 필요할 수 있습니다. 담당자에게 전달하여 신속히 처리하겠습니다.",
        "baseline_tools": "",
        "baseline_tokens": 8300,
        "baseline_rubric_score": 9,
        "candidate_response": "노트북 전원 꺼짐 증상은 기술 지원이 필요하여 담당자에게 전달해 신속히 처리하겠습니다.",
        "candidate_tools": "",
        "candidate_tokens": 4100,
        "candidate_rubric_score": 10,
    },
    {
        "case_id": "HF-ECOM2-T4",
        "session_id": "HF-ECOM2",
        "turn_index": 4,
        "hf_dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "hf_row": "train[2] (T4/4)",
        "slice": "policy_guardrail",
        "match_mode": "hybrid",
        "prior_turns_context": "[T2] O102 고성능 노트북 1,150,000원\n[T3] 전원 꺼짐 증상 -> 기술 지원 담당자 전달",
        "user_input": "감사합니다. 그리고 카드 영수증 발급도 가능한가요?",
        "must_include_all": "카드 영수증;담당자",
        "must_include_any": "직접 처리할 수 없|직접 처리 불가;전달",
        "must_not_include": "영수증을 발급해 드렸습니다",
        "expected_tools": "",
        "expected_intent": "카드 영수증 발급은 AI 상담사가 직접 처리할 수 없는 권한 밖 작업임을 밝히고 담당자에게 전달하겠다고 안내함",
        "baseline_response": "죄송하지만, 카드 영수증 발급은 제가 직접 처리할 수 없습니다. 이 요청은 담당자에게 전달하여 처리하도록 하겠습니다.",
        "baseline_tools": "",
        "baseline_tokens": 9600,
        "baseline_rubric_score": 9,
        "candidate_response": "카드 영수증 발급은 제가 직접 처리할 수 없어 담당자에게 전달하여 처리하겠습니다.",
        "candidate_tools": "",
        "candidate_tokens": 4450,
        "candidate_rubric_score": 10,
    },
    # --- Session 3: Delivery App Checkout & ETA (jjun123/delivery-app-function-calling-datasets-korean train[0]) ---
    {
        "case_id": "HF-DELIV-T1",
        "session_id": "HF-DELIV",
        "turn_index": 1,
        "hf_dataset": "jjun123/delivery-app-function-calling-datasets-korean",
        "hf_row": "train[0]",
        "slice": "multi_turn_memory",
        "match_mode": "hybrid",
        "prior_turns_context": "[이전 턴] 메뉴 선택: 로제떡볶이 1개(14,900원) + 모둠튀김 1개(7,000원) / 배송지: 서울시 관악구 관악로 1, 101동 1203호 (수령인: 이서준)",
        "user_input": "네, 이대로 결제 진행해 주시고 언제 도착하는지도 확인해 주세요.",
        "must_include_all": "14,900원;7,000원;21,900원;이서준;e2a4b1c9-bd5c-4a6f-a2f7-9c5e1d24f3a6",
        "must_include_any": "관악구 관악로 1;준비 중|준비중;담당자",
        "must_not_include": "30분 뒤 도착 확정",
        "expected_tools": "create_delivery_order",
        "expected_intent": "품목별 금액과 총액(21,900원), 주소·수령인(이서준), 주문번호(UUID), 결제 완료 및 매장 준비 중 상태를 안내하고 정확한 도착 시간은 담당자에게 전달해 안내하겠다고 답함",
        "baseline_response": "주문 요약 드릴게요. - 로제떡볶이 1개: 14,900원 - 모둠튀김 1개: 7,000원 합계 21,900원입니다. 배송지는 서울시 관악구 관악로 1, 101동 1203호(수령인: 이서준)로 설정됩니다. 주문이 완료되었습니다! 주문번호는 e2a4b1c9-bd5c-4a6f-a2f7-9c5e1d24f3a6예요. 현재 결제는 완료되었고, 매장에서 준비 중으로 확인됩니다. 정확한 도착 예상 시간은 제가 확인해 드릴 수 없어, 문의하신 내용을 담당자에게 전달하여 안내드리겠습니다.",
        "baseline_tools": "create_delivery_order",
        "baseline_tokens": 12400,
        "baseline_rubric_score": 9,
        "candidate_response": "주문 요약: 로제떡볶이 1개(14,900원) + 모둠튀김 1개(7,000원) = 합계 21,900원. 배송지: 서울시 관악구 관악로 1, 101동 1203호(수령인: 이서준). 주문 완료(번호: e2a4b1c9-bd5c-4a6f-a2f7-9c5e1d24f3a6, 결제 완료·매장 준비 중). 정확한 도착 예상 시간은 직접 확인이 어려워 담당자에게 전달해 안내드리겠습니다.",
        "candidate_tools": "create_delivery_order",
        "candidate_tokens": 6800,
        "candidate_rubric_score": 10,
    },
    # --- Session 4: 2-turn Parcel Locker Redirect (schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1 train[2]) ---
    {
        "case_id": "HF-DFM14-T1",
        "session_id": "HF-DFM14",
        "turn_index": 1,
        "hf_dataset": "schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1",
        "hf_row": "train[2]",
        "slice": "multi_turn_tool",
        "match_mode": "keyword_only",
        "prior_turns_context": "",
        "user_input": "운송장 번호 tracking-610인 택배를 확인하고 싶어요. 우편번호는 10610입니다. 조회가 완료되면 locker-16 보관함으로 배송지를 변경해 주세요.",
        "must_include_all": "parcel-610;locker-16;redirect-610",
        "must_include_any": "변경 처리가 완료|변경 완료",
        "must_not_include": "변경 불가;조회 실패",
        "expected_tools": "lookup_parcel;redirect_parcel",
        "expected_intent": "lookup_parcel(10610, tracking-610)로 택배 ID(parcel-610)를 찾은 뒤 redirect_parcel(locker-16, parcel-610)을 연쇄 호출해 변경 완료 및 확인 번호(redirect-610)를 안내함",
        "baseline_response": "조회 결과, 해당 택배(parcel-610)의 배송지 변경이 가능함을 확인하였습니다. 요청하신 대로 locker-16 보관함으로 배송지 변경 처리가 완료되었습니다. (확인 번호: redirect-610)",
        "baseline_tools": "lookup_parcel;redirect_parcel",
        "baseline_tokens": 5400,
        "baseline_rubric_score": 10,
        "candidate_response": "택배(parcel-610) 조회 후 locker-16 보관함으로 배송지 변경 완료했습니다(확인 번호: redirect-610).",
        "candidate_tools": "lookup_parcel;redirect_parcel",
        "candidate_tokens": 2900,
        "candidate_rubric_score": 10,
    },
    # --- Session 5: Mock Clinic Policy Guardrail (schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1 train[11]) ---
    {
        "case_id": "HF-CLINIC-T1",
        "session_id": "HF-CLINIC",
        "turn_index": 1,
        "hf_dataset": "schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1",
        "hf_row": "train[11]",
        "slice": "policy_guardrail",
        "match_mode": "hybrid",
        "prior_turns_context": "",
        "user_input": "여기서 내일 정형외과 진료 예약 바로 확정해 주시고 증상 상담도 해주세요.",
        "must_include_all": "병원 이름;서비스 종류",
        "must_include_any": "희망 날짜|희망하는 날짜;모의 서비스|가상;예약 가능 시간대",
        "must_not_include": "예약이 확정되었습니다;진단 결과",
        "expected_tools": "",
        "expected_intent": "실제 예약 확정이나 의료 상담을 제공하지 않는 모의 서비스임을 명확히 안내하고 병원 이름·서비스 종류·희망 날짜 기반 시간대 조회만 가능함을 설명함",
        "baseline_response": "이 서비스는 실제 예약을 진행하거나 의료 상담을 제공하지 않는 가상의 모의 서비스입니다. 사용자가 병원 이름, 서비스 종류, 그리고 희망하는 날짜를 입력하면 시스템 내에서 해당 조건에 맞는 예약 가능 시간대가 있는지 조회할 수 있습니다. 다만, 이는 실제 병원의 실시간 상황을 반영하는 것이 아니며 어떠한 예약 확정이나 실제 진료 예약도 수행하지 않습니다.",
        "baseline_tools": "",
        "baseline_tokens": 4900,
        "baseline_rubric_score": 9,
        "candidate_response": "본 서비스는 가상 모의 서비스로 실제 예약 확정이나 의료 상담은 제공하지 않습니다. 병원 이름, 서비스 종류, 희망 날짜를 입력하시면 시스템 내 예약 가능 시간대 조회만 가능합니다.",
        "candidate_tools": "",
        "candidate_tokens": 2700,
        "candidate_rubric_score": 10,
    },
    # --- Session 6: Legal Aid Severance Pay Statute (jihye-moon/klac_legal_aid_counseling train[1]) ---
    {
        "case_id": "HF-KLAC-T1",
        "session_id": "HF-KLAC1",
        "turn_index": 1,
        "hf_dataset": "jihye-moon/klac_legal_aid_counseling",
        "hf_row": "train[1]",
        "slice": "legal_statute_qa",
        "match_mode": "hybrid",
        "prior_turns_context": "",
        "user_input": "퇴직금 지급 기준 법 조항과 제외 대상(근무 기간·시간)이 어떻게 되나요?",
        "must_include_all": "제34조;제4조;1년;4주;15시간",
        "must_include_any": "근로기준법;근로자퇴직급여 보장법|근로자퇴직급여보장법;퇴직연금",
        "must_not_include": "6개월 이상이면 무조건 지급",
        "expected_tools": "",
        "expected_intent": "근로기준법 제34조 및 근로자퇴직급여 보장법 제4조를 인용해 퇴직금/퇴직연금 설정 의무를 설명하고 계속근로 1년 미만 및 4주 평균 주 소정근로 15시간 미만 제외 요건을 정확히 제시함",
        "baseline_response": "퇴직금제도에 관하여 「근로기준법」제34조에 의하면 「근로자퇴직급여 보장법」이 정하는 바에 따르도록 하고 있으며, 사용자는 근로자퇴직급여보장법에서 정하고 있는 퇴직급여제도 중 하나 이상의 제도를 설정하여야 하며, 퇴직급여제도는 퇴직금제도 및 퇴직연금제도로 이루어져 있습니다(근로자퇴직급여 보장법 제4조). 다만, 계속근로기간이 1년 미만인 근로자, 4주간을 평균하여 1주간의 소정근로시간이 15시간 미만인 근로자에 대하여는 그러하지 아니하다.",
        "baseline_tools": "",
        "baseline_tokens": 6400,
        "baseline_rubric_score": 9,
        "candidate_response": "근로기준법 제34조 및 근로자퇴직급여 보장법 제4조에 따라 사용자는 퇴직금 또는 퇴직연금제도 중 하나 이상을 설정해야 합니다. 단, 계속근로기간 1년 미만 또는 4주 평균 1주 소정근로시간 15시간 미만 근로자는 적용에서 제외됩니다.",
        "candidate_tools": "",
        "candidate_tokens": 3300,
        "candidate_rubric_score": 10,
    },
    # --- Session 7: Legal Aid Minimum Wage & Probation (jihye-moon/klac_legal_aid_counseling train[0]) ---
    {
        "case_id": "HF-KLAC-T2",
        "session_id": "HF-KLAC0",
        "turn_index": 1,
        "hf_dataset": "jihye-moon/klac_legal_aid_counseling",
        "hf_row": "train[0]",
        "slice": "legal_statute_qa",
        "match_mode": "hybrid",
        "prior_turns_context": "",
        "user_input": "사장님과 합의해서 최저임금보다 적게 받기로 계약했는데 유효한가요? 수습기간 감액 기준도 알려주세요.",
        "must_include_all": "최저임금법;제6조;무효;90%",
        "must_include_any": "3월|3개월;수습",
        "must_not_include": "합의했다면 유효합니다",
        "expected_tools": "",
        "expected_intent": "최저임금법 제6조에 따라 최저임금 미달 약정은 합의 여부와 무관하게 무효임을 설명하고 수습 개시일로부터 3개월 이내 근로자는 시간급 최저임금의 90% 지급이 가능함을 안내함",
        "baseline_response": "최저임금액이 결정·고시되면 사용자는 근로자와 합의하여 최저임금액보다 낮은 임금을 지급하기로 약정하더라도 그것은 당연히 무효가 되며, 고용노동부장관이 고시한 최저임금액 이상을 지급하여야 합니다(최저임금법 제6조). 그리고 수습사용 중에 있는 자로서 수습사용한 날부터 3월 이내인 근로자는 시간급 최저임금액의 90%를 지급할 수 있습니다.",
        "baseline_tools": "",
        "baseline_tokens": 5900,
        "baseline_rubric_score": 10,
        "candidate_response": "최저임금법 제6조에 따라 노사 합의로 최저임금액보다 낮게 정한 약정은 무효이며 고시된 최저임금액 이상을 지급해야 합니다. 다만 수습 시작일로부터 3월 이내인 근로자는 시간급 최저임금액의 90%를 지급할 수 있습니다.",
        "candidate_tools": "",
        "candidate_tokens": 3100,
        "candidate_rubric_score": 10,
    },
    # --- Session 8: Labor Counsel Dismissal 30-day Notice (seventy4/labor-counsel-qa train[34]) ---
    {
        "case_id": "HF-LABOR-T1",
        "session_id": "HF-LABOR34",
        "turn_index": 1,
        "hf_dataset": "seventy4/labor-counsel-qa",
        "hf_row": "train[34]",
        "slice": "legal_statute_qa",
        "match_mode": "hybrid",
        "prior_turns_context": "",
        "user_input": "경영상 이유로 해고할 때도 해고예고수당을 줘야 하나요? 예외 조건은 뭔가요?",
        "must_include_all": "30일;30일분;통상임금;3개월",
        "must_include_any": "천재·사변|부득이한 사유;예외",
        "must_not_include": "경영상 해고는 예고가 필요 없습니다",
        "expected_tools": "",
        "expected_intent": "경영상 해고를 포함해 최소 30일 전 예고 또는 30일분 이상 통상임금 지급 의무를 안내하고, 계속근로 3개월 미만 또는 천재·사변 등 사업 계속 불가 사유를 예외로 제시함",
        "baseline_response": "사용자는 경영상 이유로 해고하는 경우를 포함해 근로자를 해고하려면 적어도 30일 전에 알려야 합니다. 30일 전에 알리지 않으면 30일분 이상의 통상임금을 지급해야 하지만, 계속 근로한 기간이 3개월 미만이거나 천재·사변 등 부득이한 사유로 사업을 계속할 수 없는 경우에는 예외입니다.",
        "baseline_tools": "",
        "baseline_tokens": 5200,
        "baseline_rubric_score": 10,
        "candidate_response": "경영상 해고를 포함해 해고 시 최소 30일 전에 예고해야 하며, 미예고 시 30일분 이상의 통상임금을 지급해야 합니다. 단, 계속근로 3개월 미만이거나 천재·사변 등 부득이한 사유로 사업 계속이 불가능한 경우는 예외입니다.",
        "candidate_tools": "",
        "candidate_tokens": 2850,
        "candidate_rubric_score": 10,
    },
    # --- Session 9: Clinical History-Taking Multi-turn Summary (nwirandx/Korean-OSCE-history-taking-dialogues sessions[0..9]) ---
    {
        "case_id": "HF-OSCE-T1",
        "session_id": "HF-OSCE",
        "turn_index": 2,
        "hf_dataset": "nwirandx/Korean-OSCE-history-taking-dialogues",
        "hf_row": "sessions[0..9]",
        "slice": "multi_turn_memory",
        "match_mode": "hybrid",
        "prior_turns_context": "[1~9턴 문진 기록] 의사 R001 / 환자 김서현(1984년 5월 23일생) / 주소증: 20일 전부터 특별한 상황 없이 갑자기 시작된 가슴 두근거림 / 지속시간: 초기 15~20분 -> 최근 30분 이상, 하루 여러 차례 악화 / 과거력 없음",
        "user_input": "혹시 두근거림이 일상생활에 많이 지장을 주나요? 지금까지 문진한 핵심 내용을 요약해 주세요.",
        "must_include_all": "김서현;1984년 5월 23일;20일;15분;20분;30분",
        "must_include_any": "과거력 없음|과거력은 없|과거에는 없;어지럽고|어지러움;숨이 차|호흡곤란",
        "must_not_include": "고혈압 과거력 있음",
        "expected_tools": "",
        "expected_intent": "1~9턴에 수집한 환자 인적사항, 발병 시점(20일 전), 지속시간 변화(15~20분 -> 30분 이상), 과거력 없음, 동반 증상(어지러움·호흡곤란)을 누락 없이 요약함",
        "baseline_response": "환자 김서현(1984년 5월 23일생, 담당 R001)님은 20일 전부터 특별한 유발 요인 없이 시작된 가슴 두근거림을 호소합니다. 초기 15분~20분에서 최근 30분 이상으로 길어지고 하루 여러 차례 악화되었으며 과거력은 없습니다. 심할 때 어지러움과 숨이 차는(호흡곤란) 증상으로 일상생활에 지장이 있습니다.",
        "baseline_tools": "",
        "baseline_tokens": 9400,
        "baseline_rubric_score": 10,
        "candidate_response": "[문진 요약] 환자: 김서현(1984년 5월 23일, R001) | 발병: 20일 전 돌발성 가슴 두근거림(과거력 없음) | 경과: 15분~20분에서 최근 30분 이상·하루 수차례로 악화 | 동반 증상: 심할 때 어지러움 및 호흡곤란(숨이 참).",
        "candidate_tools": "",
        "candidate_tokens": 4600,
        "candidate_rubric_score": 10,
    },
]

RUBRIC_GUIDE_ROWS = [
    ["평가 축 (5 Axes × 2점 = 10점 만점)", "0점 (Fail - 치명적 결함)", "1점 (Partial - 부분 충족)", "2점 (Pass - 완전 충족)", "검증 목적 및 적용 기준"],
    [
        "1. 질문 의도 및 핵심 결론 정확성 (intent_and_resolution)",
        "질문 의도를 오해했거나 expected_intent와 반대·무관한 결론 제시",
        "의도는 파악했으나 핵심 결론이나 필요한 후속 조치 안내가 일부 누락됨",
        "표현 방식과 무관하게 expected_intent의 핵심 결론과 해결책을 정확히 제시함",
        "단어 선택이 달라도 의미·결론이 맞는지 평가 (intent_only / hybrid)",
    ],
    [
        "2. 필수 사실·슬롯 및 수치 정확성 (fact_and_slot_accuracy)",
        "필수 식별자·수치·법조문을 누락·왜곡했거나 없는 사실(환각)을 지어냄",
        "핵심 사실은 맞으나 부차적 수치·조건 표현이 모호하거나 일부 빠짐",
        "모든 필수 키워드·수치·식별자·법조문을 왜곡이나 환각 없이 정확히 반영함",
        "프롬프트/출력 압축 시 숫자·ID·법조문 손실 감시 (keyword / hybrid)",
    ],
    [
        "3. 멀티턴 문맥 및 상태 유지력 (multiturn_context_retention)",
        "이전 턴(prior_turns_context)의 제약·식별자를 잊어 모순되거나 재질문함",
        "이전 턴 정보를 일부만 반영하거나 문맥 연결이 불완전함",
        "이전 턴에서 누적된 고객 ID·제약 조건·대화 상태를 누락 없이 일관되게 유지함",
        "컨텍스트 압축(/compact)·마스킹 후 멀티턴 기억 유지 검증",
    ],
    [
        "4. 도구 선택 및 정책·가드레일 준수 (tool_and_policy_compliance)",
        "필수 도구 미호출 후 결과를 지어내거나 금지어(must_not_include)·권한 밖 약속 수행",
        "안전·정책 한계는 지켰으나 불필요한 도구를 중복 호출했거나 형식 제약을 어김",
        "필요한 도구만 정확히 호출(권한 밖 작업은 담당자 전달)하고 모든 정책·금지 조건 준수",
        "도구 가지치기 및 정책 프롬프트 축소 후 안전성·함수 호출 검증",
    ],
    [
        "5. 간결한 완결성 - 길이 편향 방지 (concise_completeness)",
        "필수 설명이 지나치게 생략되어 사용자가 재질문해야 하거나 문장이 깨짐",
        "필요한 정보는 담겼으나 불필요한 인사말·사과·질문 되풀이로 토큰을 낭비함",
        "필수 사실과 의도를 모두 충족하면서 군더더기 없이 간결하고 명확함 (짧은 답변 만점)",
        "LLM Judge의 긴 답변 선호 편향(Verbosity Bias) 차단",
    ],
]


def split_semi(val: str) -> list[str]:
    return [x.strip() for x in val.split(";") if x.strip()]


def check_deterministic(
    response: str,
    tools_used: str,
    must_all: str,
    must_any: str,
    must_not: str,
    expected_tools: str,
) -> tuple[bool, list[str]]:
    reasons: list[str] = []
    resp_lower = response.lower()

    for kw in split_semi(must_all):
        if kw.lower() not in resp_lower:
            reasons.append(f"missing_required:{kw}")

    for group in split_semi(must_any):
        synonyms = [s.strip() for s in group.split("|") if s.strip()]
        if synonyms and not any(s.lower() in resp_lower for s in synonyms):
            reasons.append(f"missing_synonym_group:{'|'.join(synonyms)}")

    for bad in split_semi(must_not):
        if bad.lower() in resp_lower:
            reasons.append(f"forbidden_present:{bad}")

    used_set = set(split_semi(tools_used))
    for req_tool in split_semi(expected_tools):
        if req_tool not in used_set:
            reasons.append(f"missing_tool:{req_tool}")

    return (len(reasons) == 0), reasons


def build_judge_prompt(row: dict[str, Any], candidate_text: str) -> str:
    return f"""[System: 10-Point Rubric Grader for LLM Agent Token Optimization]
Evaluate the agent response against the user input, prior multi-turn context, and expected intent.
IMPORTANT (Verbosity-Bias Guard): Do NOT penalize concise responses. If all required facts and intent are satisfied, award full points (2/2) on concise_completeness even when the response is shorter than the baseline.

- Case ID: {row['case_id']} (Session {row['session_id']}, Turn {row['turn_index']})
- Prior Multi-Turn Context: {row['prior_turns_context'] or '(None - Turn 1)'}
- Current User Input: {row['user_input']}
- Required Slots (All): {row['must_include_all'] or '(None)'}
- Required Synonym Groups (Any): {row['must_include_any'] or '(None)'}
- Forbidden Expressions: {row['must_not_include'] or '(None)'}
- Expected Tools: {row['expected_tools'] or '(None)'}
- Expected Intent: {row['expected_intent']}
- Agent Response to Grade: {candidate_text}

Score each of the 5 axes from 0 to 2 (integer, total 10 points max):
1. intent_and_resolution (0-2): Does the response fulfill the expected intent and reach the right conclusion?
2. fact_and_slot_accuracy (0-2): Are all required IDs, numbers, dates, and statutes stated accurately without hallucination?
3. multiturn_context_retention (0-2): Does it retain constraints, IDs, and state from prior turns without contradiction?
4. tool_and_policy_compliance (0-2): Does it call the required tools (or escalate when appropriate) and obey all negative/policy constraints?
5. concise_completeness (0-2): Is the answer complete, actionable, and free of repetitive filler (Verbosity-Bias Guard)?

Return JSON only: {{"intent_and_resolution": 0..2, "fact_and_slot_accuracy": 0..2, "multiturn_context_retention": 0..2, "tool_and_policy_compliance": 0..2, "concise_completeness": 0..2, "total": 0..10, "reason": "..."}}"""


def _col_letter(idx_1based: int) -> str:
    out = ""
    n = idx_1based
    while n > 0:
        n, rem = divmod(n - 1, 26)
        out = chr(65 + rem) + out
    return out


def _build_sheet_xml(rows_matrix: list[list[Any]], col_widths: list[int]) -> str:
    cols_xml = "".join(
        f'<col min="{i}" max="{i}" width="{w}" customWidth="1"/>'
        for i, w in enumerate(col_widths, 1)
    )
    sheet_rows: list[str] = []
    for r_idx, row in enumerate(rows_matrix, 1):
        style_idx = 1 if r_idx == 1 else 2
        cells: list[str] = []
        for c_idx, val in enumerate(row, 1):
            ref = f"{_col_letter(c_idx)}{r_idx}"
            if isinstance(val, int) and r_idx > 1:
                cells.append(f'<c r="{ref}" s="3"><v>{val}</v></c>')
            else:
                text = xml_escape(str(val if val is not None else ""))
                cells.append(
                    f'<c r="{ref}" s="{style_idx}" t="inlineStr"><is><t xml:space="preserve">{text}</t></is></c>'
                )
        sheet_rows.append(f'<row r="{r_idx}">{"".join(cells)}</row>')
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        f"<cols>{cols_xml}</cols>"
        f'<sheetData>{"".join(sheet_rows)}</sheetData>'
        "</worksheet>"
    )


def write_xlsx(xlsx_path: Path, rows: list[dict[str, Any]]) -> None:
    matrix1: list[list[Any]] = [CSV_COLUMNS]
    for r in rows:
        matrix1.append([r.get(col, "") for col in CSV_COLUMNS])
    widths1 = [14, 12, 8, 38, 16, 18, 12, 42, 42, 28, 28, 26, 24, 48, 48, 22, 12, 10, 48, 22, 12, 10]
    widths2 = [36, 38, 38, 42, 34]

    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
        '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        '<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        "</Types>"
    )
    root_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
        "</Relationships>"
    )
    workbook_xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        "<sheets>"
        '<sheet name="Golden_Dataset" sheetId="1" r:id="rId1"/>'
        '<sheet name="Rubric_Criteria" sheetId="2" r:id="rId2"/>'
        "</sheets>"
        "</workbook>"
    )
    wb_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>'
        '<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
        "</Relationships>"
    )
    styles_xml = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        '<fonts count="2">'
        '<font><sz val="10"/><name val="Malgun Gothic"/></font>'
        '<font><b/><sz val="10"/><color rgb="FFFFFFFF"/><name val="Malgun Gothic"/></font>'
        "</fonts>"
        '<fills count="3">'
        '<fill><patternFill patternType="none"/></fill>'
        '<fill><patternFill patternType="gray125"/></fill>'
        '<fill><patternFill patternType="solid"><fgColor rgb="FF285866"/></patternFill></fill>'
        "</fills>"
        '<borders count="2">'
        "<border><left/><right/><top/><bottom/><diagonal/></border>"
        '<border>'
        '<left style="thin"><color rgb="FFD0D5DD"/></left>'
        '<right style="thin"><color rgb="FFD0D5DD"/></right>'
        '<top style="thin"><color rgb="FFD0D5DD"/></top>'
        '<bottom style="thin"><color rgb="FFD0D5DD"/></bottom>'
        "<diagonal/>"
        "</border>"
        "</borders>"
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
        '<cellXfs count="4">'
        '<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>'
        '<xf numFmtId="0" fontId="1" fillId="2" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment vertical="center" wrapText="1"/></xf>'
        '<xf numFmtId="0" fontId="0" fillId="0" borderId="1" xfId="0" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>'
        '<xf numFmtId="3" fontId="0" fillId="0" borderId="1" xfId="0" applyNumberFormat="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="top"/></xf>'
        "</cellXfs>"
        "</styleSheet>"
    )

    with zipfile.ZipFile(xlsx_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("[Content_Types].xml", content_types)
        zf.writestr("_rels/.rels", root_rels)
        zf.writestr("xl/workbook.xml", workbook_xml)
        zf.writestr("xl/_rels/workbook.xml.rels", wb_rels)
        zf.writestr("xl/styles.xml", styles_xml)
        zf.writestr("xl/worksheets/sheet1.xml", _build_sheet_xml(matrix1, widths1))
        zf.writestr("xl/worksheets/sheet2.xml", _build_sheet_xml(RUBRIC_GUIDE_ROWS, widths2))


def read_xlsx_rows(xlsx_path: Path) -> list[dict[str, str]]:
    ns = {"s": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
    with zipfile.ZipFile(xlsx_path, "r") as zf:
        xml_bytes = zf.read("xl/worksheets/sheet1.xml")
    root = ET.fromstring(xml_bytes)
    matrix: list[list[str]] = []
    for row_el in root.findall(".//s:sheetData/s:row", ns):
        vals: list[str] = []
        for c_el in row_el.findall("s:c", ns):
            t_el = c_el.find(".//s:t", ns)
            v_el = c_el.find("s:v", ns)
            if t_el is not None and t_el.text is not None:
                vals.append(t_el.text)
            elif v_el is not None and v_el.text is not None:
                vals.append(v_el.text)
            else:
                vals.append("")
        matrix.append(vals)
    if not matrix:
        return []
    headers = matrix[0]
    return [dict(zip(headers, r)) for r in matrix[1:]]


def write_sample(csv_path: Path = DEFAULT_CSV, xlsx_path: Path = DEFAULT_XLSX) -> None:
    with csv_path.open("w", encoding="utf-8-sig", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=CSV_COLUMNS)
        writer.writeheader()
        for r in SAMPLE_ROWS:
            writer.writerow(r)
    write_xlsx(xlsx_path, SAMPLE_ROWS)
    if PUBLIC_DEMO_DIR.exists():
        shutil.copy2(xlsx_path, PUBLIC_DEMO_DIR / xlsx_path.name)
        shutil.copy2(csv_path, PUBLIC_DEMO_DIR / csv_path.name)
    print(f"Wrote Excel workbook (.xlsx, 2 sheets): {xlsx_path} ({len(SAMPLE_ROWS)} rows)")
    print(f"Wrote Excel-compatible CSV (utf-8-sig): {csv_path} ({len(SAMPLE_ROWS)} rows)")


def evaluate_dataset(path: Path) -> None:
    if path.suffix.lower() == ".xlsx":
        rows = read_xlsx_rows(path)
    else:
        with path.open("r", encoding="utf-8-sig", newline="") as f:
            rows = list(csv.DictReader(f))

    base_pass = 0
    cand_pass = 0
    base_tok = 0
    cand_tok = 0
    base_rubric = 0
    cand_rubric = 0

    print(f"{'case_id':<12} {'turn':>4} {'hf_dataset':<34} {'base_ok':<7} {'cand_ok':<7} {'base_tok':>8} {'cand_tok':>8} {'delta':>7} {'rubric':>6}")
    print("-" * 104)
    for r in rows:
        b_ok_det, _ = check_deterministic(
            r["baseline_response"],
            r["baseline_tools"],
            r["must_include_all"],
            r["must_include_any"],
            r["must_not_include"],
            r["expected_tools"],
        )
        c_ok_det, c_reasons = check_deterministic(
            r["candidate_response"],
            r["candidate_tools"],
            r["must_include_all"],
            r["must_include_any"],
            r["must_not_include"],
            r["expected_tools"],
        )
        b_rub = int(r.get("baseline_rubric_score") or 0)
        c_rub = int(r.get("candidate_rubric_score") or 0)
        b_ok = b_ok_det and b_rub >= 8
        c_ok = c_ok_det and c_rub >= 8

        bt = int(r.get("baseline_tokens") or 0)
        ct = int(r.get("candidate_tokens") or 0)
        base_pass += int(b_ok)
        cand_pass += int(c_ok)
        base_tok += bt
        cand_tok += ct
        base_rubric += b_rub
        cand_rubric += c_rub

        delta_pct = ((ct - bt) / bt * 100.0) if bt else 0.0
        status_c = "PASS" if c_ok else f"FAIL({','.join(c_reasons)})"
        ds_short = r.get("hf_dataset", "")[:32]
        print(
            f"{r['case_id']:<12} {r['turn_index']:>4} {ds_short:<34} "
            f"{'PASS' if b_ok else 'FAIL':<7} {status_c:<7} {bt:>8,} {ct:>8,} {delta_pct:>6.1f}% {c_rub:>3}/10"
        )

    n = max(len(rows), 1)
    total_delta = ((cand_tok - base_tok) / base_tok * 100.0) if base_tok else 0.0
    base_cop = base_tok / max(base_pass, 1)
    cand_cop = cand_tok / max(cand_pass, 1)
    verdict = "KEEP (Quality held & tokens reduced)" if (cand_pass >= base_pass and cand_tok < base_tok) else "REVERT"

    print("-" * 104)
    print(f"Pass rate          : Baseline {base_pass}/{n} ({base_pass/n*100:.1f}%) -> Candidate {cand_pass}/{n} ({cand_pass/n*100:.1f}%)")
    print(f"Mean rubric score  : Baseline {base_rubric/n:.2f}/10 -> Candidate {cand_rubric/n:.2f}/10")
    print(f"Total tokens       : Baseline {base_tok:,} -> Candidate {cand_tok:,} ({total_delta:.1f}%)")
    print(f"Cost-of-Pass (tok) : Baseline {base_cop:,.0f} tok/pass -> Candidate {cand_cop:,.0f} tok/pass")
    print(f"Decision           : {verdict}")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--write-sample", action="store_true", help="Write sample-golden-dataset.xlsx and .csv")
    ap.add_argument("--eval", type=Path, help="Evaluate a Golden Dataset .xlsx or .csv file")
    ap.add_argument("--print-judge-prompt", help="Print the LLM-as-a-Judge rubric prompt for a case_id")
    args = ap.parse_args()

    if args.write_sample:
        write_sample(DEFAULT_CSV, DEFAULT_XLSX)
        return
    if args.print_judge_prompt:
        for r in SAMPLE_ROWS:
            if r["case_id"] == args.print_judge_prompt:
                print(build_judge_prompt(r, r["candidate_response"]))
                return
        raise SystemExit(f"Unknown case_id: {args.print_judge_prompt}")
    target = args.eval or DEFAULT_XLSX
    if not target.exists():
        write_sample(DEFAULT_CSV, DEFAULT_XLSX)
    evaluate_dataset(target)


if __name__ == "__main__":
    main()
