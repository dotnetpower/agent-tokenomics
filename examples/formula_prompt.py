#!/usr/bin/env python3
"""Compare prose, Unicode math, and BPE-aligned formula prompts across tokenizers.

Reproduces the BPE-aligned formula tables in:
- copilot-tokenomics/04-instructions-and-tools
- copilot-tokenomics/06-prompt-and-output-style
- copilot-tokenomics/07-language-and-tokenization
- agent-tokenomics/04-improve

Includes real customer-support and counseling benchmark cases drawn from public
Hugging Face datasets (`iamjoon/ecommerce-function-calling-datasets-korean`,
`jjun123/delivery-app-function-calling-datasets-korean`,
`jihye-moon/klac_legal_aid_counseling`, `seventy4/labor-counsel-qa`,
`NebulaByte/E-Commerce_Customer_Support_Conversations`, and
`bitext/Bitext-customer-support-llm-chatbot-training-dataset`).

Also provides:
- `--compress` to convert an English or Korean prompt/counselor turn into a BPE-aligned formula
- `--compress-dialogue` to compact a multi-turn counselor transcript into a BPE state/action chain
- `--hf-live` to fetch live rows from Hugging Face Datasets Server and benchmark compression

Setup:
    python3 -m venv .venv && .venv/bin/pip install tiktoken
Usage:
    .venv/bin/python examples/formula_prompt.py
    .venv/bin/python examples/formula_prompt.py --compress "이슈 1945을 완료할수 있도록 남은 작업에 대한 계획을 세워서 모두 조치하고 완료되면 푸시 머지까지 해서 이슈를 닫을수 있도록 하자"
    .venv/bin/python examples/formula_prompt.py --hf-live --hf-rows 5
"""

from __future__ import annotations

import argparse
import json
import re
import urllib.parse
import urllib.request
from typing import Any, Sequence

import tiktoken

STYLE_COMPARISON = {
    "Prose (with backticks)": (
        "When you need to find a symbol or literal and read its surrounding lines, "
        "use `code_context` in one call instead of searching first and reading later."
    ),
    "Unicode math (unspaced)": (
        "∀x∈Sym∪Lit: need(x+ctx) ⇒ code_context(x) ≻ (grep(x) ∘ read(x))"
    ),
    "ASCII formula (unspaced)": (
        "δ(symbol|literal+surrounding_lines)=>code_context(root,pattern,radius<=20)"
    ),
    "BPE-aligned formula (spaced)": (
        "symbol + context => code_context (1 call > grep + read)"
    ),
}

KOREAN_WORKFLOW_CASES = [
    (
        "Issue completion pipeline",
        "이슈 1945을 완료할수 있도록 남은 작업에 대한 계획을 세워서 모두 조치하고 완료되면 푸시 머지까지 해서 이슈를 닫을수 있도록 하자",
        "[ko] :: #1945 남은작업 계획수립 -> 모두조치 -> push & merge -> 이슈닫기",
    ),
    (
        "Bulk gap discovery & loop",
        "더 보강이 필요한것 100개 이상 찾아서 모두 조치하는 라운드를 10회 진행하고 결과를 수치화된 표로 보고한 다음 커밋 푸시 머지까지 진행해",
        "[ko] :: 보강점 >=100 발굴 -> 모두조치 -> loop 10x -> 수치표 보고 -> commit & push & merge",
    ),
    (
        "Bounded hardening loop",
        "모두 적용하고 비평 10개이상 하드닝 라운드를 심각도가 Low 이하만 남을때 까지 반복하고 완료되면 개선점을 100개 이상 찾아서 모두 보강한 다음 푸시 머지 후 보고해",
        "[ko] :: 모두적용 -> loop(비평 >=10 하드닝, until 심각도 <= Low) -> 개선점 >=100 발굴 -> 모두보강 -> push & merge -> 보고",
    ),
    (
        "Optimization audit & list",
        "다음은 절감액이다 더 최적화 할수 있는 부분을 30가지 이상 찾아서 계획을 세워서 목록으로 보여줘",
        "[ko] :: 절감액 기반 최적화 >=30 발굴 -> 계획수립 -> 목록 보고",
    ),
]

HF_COUNSELING_TURN_CASES: list[dict[str, Any]] = [
    {
        "name": "E-commerce shipping & refund",
        "dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "row_ref": "train[5]",
        "prose": (
            "고객님께서 5월 10일에 주문하신 ‘무선 이어폰’은 현재 배송 중이며, 택배사는 한진택배, "
            "송장번호는 4567891230입니다. 환불은 상품 수령 후 7일 이내에 신청 가능하며, "
            "왕복 배송비가 부과될 수 있습니다. 주문 취소는 상품이 출고되기 전에는 언제든지 가능하며, "
            "출고 이후에는 반품으로 처리됩니다."
        ),
        "formula": (
            "[ko] :: 5월 10일 주문 무선 이어폰 => 배송중 & 한진택배 4567891230 | "
            "환불: 수령 <= 7일 신청 & 왕복 배송비 | 주문 취소: 출고전 => 언제든 가능 | 출고후 => 반품 처리"
        ),
        "required_slots": [
            "5월 10일",
            "무선 이어폰",
            "배송중",
            "한진택배",
            "4567891230",
            "환불",
            "7일",
            "왕복 배송비",
            "출고전",
            "출고후",
            "반품 처리",
        ],
    },
    {
        "name": "Defect & receipt escalation",
        "dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "row_ref": "train[2]",
        "prose": (
            "'고성능 노트북' 1대를 1,150,000원에 구매하셨습니다. 불편을 드려 죄송합니다. "
            "해당 문제는 기술적인 지원이 필요할 수 있습니다. 담당자에게 전달하여 신속히 처리하겠습니다. "
            "죄송하지만, 카드 영수증 발급은 제가 직접 처리할 수 없습니다. "
            "이 요청은 담당자에게 전달하여 처리하도록 하겠습니다."
        ),
        "formula": (
            "[ko] :: 고성능 노트북 1대 1,150,000원 구매 | "
            "기술 지원 필요 -> 담당 전달 처리 | 카드 영수증 발급: 직접 처리 불가 -> 담당 전달 처리"
        ),
        "required_slots": [
            "고성능 노트북",
            "1대",
            "1,150,000원",
            "구매",
            "기술 지원",
            "카드 영수증 발급",
            "직접 처리 불가",
            "담당 전달 처리",
        ],
    },
    {
        "name": "Delivery checkout & ETA",
        "dataset": "jjun123/delivery-app-function-calling-datasets-korean",
        "row_ref": "train[0]",
        "prose": (
            "주문 요약 드릴게요. - 로제떡볶이 1개: 14,900원 - 모둠튀김 1개: 7,000원 합계 21,900원입니다. "
            "배송지는 서울시 관악구 관악로 1, 101동 1203호(수령인: 이서준)로 설정됩니다. "
            "주문이 완료되었습니다! 주문번호는 e2a4b1c9-bd5c-4a6f-a2f7-9c5e1d24f3a6예요. "
            "현재 결제는 완료되었고, 매장에서 준비 중으로 확인됩니다. "
            "정확한 도착 예상 시간은 제가 확인해 드릴 수 없어, 문의하신 내용을 담당자에게 전달하여 안내드리겠습니다."
        ),
        "formula": (
            "[ko] :: 주문 요약: 로제떡볶이 1개 14,900원 + 모둠튀김 1개 7,000원 = 21,900원 | "
            "배송지: 서울시 관악구 관악로 1, 101동 1203호 이서준 -> "
            "주문 완료 e2a4b1c9-bd5c-4a6f-a2f7-9c5e1d24f3a6 => 결제 완료 & 매장 준비중 | "
            "도착 예상 시간: 직접 확인 불가 -> 담당 전달 안내"
        ),
        "required_slots": [
            "로제떡볶이",
            "14,900원",
            "모둠튀김",
            "7,000원",
            "21,900원",
            "서울시 관악구 관악로 1, 101동 1203호",
            "이서준",
            "e2a4b1c9-bd5c-4a6f-a2f7-9c5e1d24f3a6",
            "결제 완료",
            "매장 준비중",
            "도착 예상 시간",
        ],
    },
    {
        "name": "Severance pay statute",
        "dataset": "jihye-moon/klac_legal_aid_counseling",
        "row_ref": "train[1]",
        "prose": (
            "퇴직금제도에 관하여 「근로기준법」제34조에 의하면 「근로자퇴직급여 보장법」이 정하는 바에 "
            "따르도록 하고 있으며, 사용자는 근로자퇴직급여보장법에서 정하고 있는 퇴직급여제도 중 하나 이상의 "
            "제도를 설정하여야 하며, 퇴직급여제도는 퇴직금제도 및 퇴직연금제도로 이루어져 있습니다"
            "(근로자퇴직급여 보장법 제4조). 다만, 계속근로기간이 1년 미만인 근로자, 4주간을 평균하여 "
            "1주간의 소정근로시간이 15시간 미만인 근로자에 대하여는 그러하지 아니하다."
        ),
        "formula": (
            "[ko] :: 근로기준법 제34조 & 근로자퇴직급여 보장법 제4조 => "
            "사용자 퇴직급여 (퇴직금 | 퇴직연금) >= 1개 설정 | "
            "제외: 계속 근로 < 1년 | 4주 평균 주 소정근로 < 15시간"
        ),
        "required_slots": [
            "근로기준법 제34조",
            "근로자퇴직급여 보장법 제4조",
            "퇴직금",
            "퇴직연금",
            "1년",
            "4주",
            "15시간",
        ],
    },
    {
        "name": "Average wage & floor rule",
        "dataset": "seventy4/labor-counsel-qa",
        "row_ref": "train[2,3]",
        "prose": (
            "근로기준법 제2조에 따르면 산정 사유가 발생한 날 이전 3개월 동안 지급된 임금 총액을 그 기간의 "
            "총일수로 나누며, 취업 후 3개월 미만인 경우에도 같은 방식으로 봅니다. 근로기준법 제3조에 따라 "
            "법에서 정한 근로조건은 최저기준이므로, 근로관계 당사자는 그 기준을 이유로 근로조건을 낮출 수 없습니다."
        ),
        "formula": (
            "[ko] :: 근로기준법 제2조 평균 임금 = 사유 발생 이전 3개월 지급 임금 / 총일수 & "
            "취업 < 3개월 동일 방식 | 근로기준법 제3조 근로조건 = 최저 기준 => 기준 이유 근로조건 저하 불가"
        ),
        "required_slots": [
            "근로기준법 제2조",
            "3개월",
            "총일수",
            "근로기준법 제3조",
            "최저 기준",
            "근로조건",
        ],
    },
    {
        "name": "Order cancel SOP (English)",
        "dataset": "bitext/Bitext-customer-support-llm-chatbot-training-dataset",
        "row_ref": "train[2]",
        "prose": (
            "I can sense that you're seeking assistance with canceling your purchase with the purchase number "
            "{{Order Number}}. I apologize for any inconvenience caused, and I'm here to guide you through the "
            "process. To cancel your purchase, please follow these steps: 1. Log into your account: Visit our "
            "{{Online Company Portal Info}} and sign in using your credentials. 2. Locate your order: Once logged "
            "in, navigate to the '{{Online Order Interaction}}' section to find the purchase with the order number "
            "{{Order Number}}. 3. Initiate the cancellation: Upon finding your purchase, look for the option "
            "labeled '{{Online Order Interaction}}' and click on it to initiate the cancellation process. "
            "4. Confirm the cancellation: Our system might ask for confirmation or feedback regarding the "
            "cancellation. Kindly provide the necessary information to complete the process. 5. Seek additional "
            "support: If you encounter any difficulties or require further assistance, our dedicated customer "
            "support team is available during {{Customer Support Hours}}. You can contact us at "
            "{{Customer Support Phone Number}} or use the Live Chat feature on our {{Website URL}}."
        ),
        "formula": (
            "cancel_order {{Order Number}} :: login {{Online Company Portal Info}} -> "
            "open {{Online Order Interaction}} -> select {{Order Number}} -> click cancel -> confirm | "
            "help => support {{Customer Support Hours}} @ {{Customer Support Phone Number}} | Live Chat {{Website URL}}"
        ),
        "required_slots": [
            "cancel",
            "{{Order Number}}",
            "{{Online Company Portal Info}}",
            "{{Online Order Interaction}}",
            "confirm",
            "{{Customer Support Hours}}",
            "{{Customer Support Phone Number}}",
            "Live Chat",
            "{{Website URL}}",
        ],
    },
]

HF_COUNSELING_DIALOGUE_CASES: list[dict[str, Any]] = [
    {
        "name": "Address + shipping + spam report (14 turns)",
        "dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "row_ref": "train[0]",
        "prose": (
            "user: 안녕하세요.\n"
            "assistant: 안녕하세요! 상준몰 AI 상담사입니다. 무엇을 도와드릴까요?\n"
            "user: 제 주소가 어떻게 되어 있는지 확인하고 싶어요.\n"
            'assistant: <tool_call>\n{"name": "view_user_profile", "arguments": {"user_id": "U002"}}\n</tool_call>\n'
            "assistant: 고객님의 주소는 '서울특별시 강남구 테헤란로 123'으로 등록되어 있습니다. 다른 정보가 필요하신가요?\n"
            "user: 최근에 주문한 상품의 배송 상태도 알고 싶어요.\n"
            'assistant: <tool_call>\n{"name": "view_order_history", "arguments": {"user_id": "U002"}}\n</tool_call>\n'
            "assistant: 고객님께서 7월 10일에 주문하신 '무선 이어폰'은 현재 배송중이며, 택배사는 한진택배, 송장번호는 123456789012입니다.\n"
            "user: 아, 그리고 상준몰에 리뷰 스팸이 너무 많던데요.\n"
            "assistant: 불편을 드려 죄송합니다. 리뷰 스팸 관련 문제는 담당자에게 전달하여 검토하도록 하겠습니다.\n"
            "user: 네, 감사합니다.\n"
            "assistant: 더 궁금하신 사항이 있으신가요?\n"
            "user: 아니요, 괜찮습니다.\n"
            "assistant: 네, 문의해 주셔서 감사합니다. 상준몰 고객센터 AI 상담사였습니다. 좋은 하루 되세요!"
        ),
        "formula": (
            "[ko] :: 주소 확인 -> view_user_profile(U002) => 주소 = 서울특별시 강남구 테헤란로 123 | "
            "배송 상태 확인 -> view_order_history(U002) => 7월 10일 주문 무선 이어폰 => 배송중 & 한진택배 123456789012 | "
            "상준몰 리뷰 스팸 신고 => 리뷰 스팸 -> 담당 전달 검토"
        ),
        "required_slots": [
            "view_user_profile(U002)",
            "서울특별시 강남구 테헤란로 123",
            "view_order_history(U002)",
            "7월 10일",
            "무선 이어폰",
            "한진택배",
            "123456789012",
            "리뷰 스팸",
            "담당 전달 검토",
        ],
    },
    {
        "name": "Cart + laptop defect + receipt (15 turns)",
        "dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "row_ref": "train[2]",
        "prose": (
            "user: 안녕하세요. 장바구니에 뭐가 들어있는지 좀 볼 수 있을까요?\n"
            'assistant: <tool_call>\n{"name": "show_cart", "arguments": {"user_id": "U001"}}\n</tool_call>\n'
            "assistant: 고객님의 장바구니에는 '블루투스 이어폰' 1개와 '무선 마우스' 2개가 담겨 있습니다.\n"
            "user: 아, 그리고 최근에 주문한 노트북에 문제가 생겼어요. 상세 내역 좀 볼 수 있을까요?\n"
            'assistant: <tool_call>\n{"name": "view_order_history", "arguments": {"user_id": "U001"}}\n</tool_call>\n'
            'assistant: <tool_call>\n{"name": "view_order_details", "arguments": {"user_id": "U001", "order_id": "O102"}}\n</tool_call>\n'
            "assistant: '고성능 노트북' 1대를 1,150,000원에 구매하셨습니다. 문제가 발생한 부분에 대해 더 도와드릴까요?\n"
            "user: 네, 노트북이 자꾸 꺼져서요. 이거 어떻게 해야 하나요?\n"
            "assistant: 불편을 드려 죄송합니다. 해당 문제는 기술적인 지원이 필요할 수 있습니다. 담당자에게 전달하여 신속히 처리하겠습니다.\n"
            "user: 감사합니다. 그리고 카드 영수증 발급도 가능한가요?\n"
            "assistant: 죄송하지만, 카드 영수증 발급은 제가 직접 처리할 수 없습니다. 이 요청은 담당자에게 전달하여 처리하도록 하겠습니다.\n"
            "user: 네, 알겠습니다.\n"
            "assistant: 더 궁금하신 사항이 있으신가요?\n"
            "user: 아니요, 괜찮습니다.\n"
            "assistant: 네, 문의해 주셔서 감사합니다. 상준몰 고객센터 AI 상담사였습니다. 좋은 하루 되세요!"
        ),
        "formula": (
            "[ko] :: 장바구니 확인 -> show_cart(U001) => 장바구니 = 블루투스 이어폰 1개와 무선 마우스 2개 | "
            "주문 노트북 문제 -> 상세 내역 확인 -> view_order_history(U001) -> view_order_details(U001, O102) => "
            "고성능 노트북 1대 1,150,000원 구매 | 노트북 꺼짐 증상 문의 => 기술 지원 필요 -> 담당 전달 처리 | "
            "카드 영수증 발급 문의 => 카드 영수증 발급: 직접 처리 불가 -> 담당 전달 처리"
        ),
        "required_slots": [
            "show_cart(U001)",
            "블루투스 이어폰",
            "무선 마우스",
            "view_order_history(U001)",
            "view_order_details(U001, O102)",
            "고성능 노트북",
            "1,150,000원",
            "기술 지원",
            "카드 영수증 발급",
        ],
    },
    {
        "name": "Shipping + Gold tier + refund/cancel (16 turns)",
        "dataset": "iamjoon/ecommerce-function-calling-datasets-korean",
        "row_ref": "train[5]",
        "prose": (
            "user: 안녕하세요. 최근 주문한 상품 배송 상태 좀 확인하고 싶어요.\n"
            'assistant: <tool_call>\n{"name": "view_order_history", "arguments": {"user_id": "U002"}}\n</tool_call>\n'
            "assistant: 고객님께서 5월 10일에 주문하신 ‘무선 이어폰’은 현재 배송 중이며, 택배사는 한진택배, 송장번호는 4567891230입니다.\n"
            "user: 감사합니다. 그리고 제 멤버십 등급도 알고 싶어요.\n"
            'assistant: <tool_call>\n{"name": "view_user_profile", "arguments": {"user_id": "U002"}}\n</tool_call>\n'
            "assistant: 고객님의 현재 멤버십 등급은 Gold입니다. 현재 10% 할인 쿠폰도 보유하고 계십니다.\n"
            "user: 환불 정책도 좀 알려주세요.\n"
            'assistant: <tool_call>\n{"name": "search_policy_info", "arguments": {"keyword": "환불"}}\n</tool_call>\n'
            "assistant: 환불은 상품 수령 후 7일 이내에 신청 가능하며, 왕복 배송비가 부과될 수 있습니다. 환불 신청은 상준몰 고객센터를 통해 가능합니다.\n"
            "user: 알겠습니다. 그런데 결제 취소는 어떻게 하나요?\n"
            'assistant: <tool_call>\n{"name": "search_policy_info", "arguments": {"keyword": "주문 취소"}}\n</tool_call>\n'
            "assistant: 주문 취소는 상품이 출고되기 전에는 언제든지 가능하며, 출고 이후에는 반품으로 처리됩니다. 주문내역에서 주문취소 버튼을 통해 취소할 수 있습니다.\n"
            "user: 그럼 결제 취소는 직접 못하나요?\n"
            "assistant: 네, 결제 취소는 직접 처리할 수 없으며, 담당자에게 요청 내용을 전달하여 신속히 처리하겠습니다. 더 궁금하신 사항이 있으신가요?\n"
            "user: 아니요, 괜찮아요.\n"
            "assistant: 네, 문의해 주셔서 감사합니다. 상준몰 AI 상담사였습니다. 좋은 하루 보내세요!"
        ),
        "formula": (
            "[ko] :: 배송 상태 확인 -> view_order_history(U002) => 5월 10일 주문 무선 이어폰 => 배송중 & 한진택배 4567891230 | "
            "멤버십 등급 확인 -> view_user_profile(U002) => 멤버십 = Gold & 10% 할인 쿠폰 보유 | "
            "환불 정책 문의 -> search_policy_info(환불) => 환불: 수령 <= 7일 신청 & 왕복 배송비 | "
            "결제 취소 방법 문의 -> search_policy_info(주문 취소) => 주문 취소: 출고전 => 언제든 가능 | 출고후 => 반품 처리 | "
            "결제 취소 직접 가능 여부 => 결제 취소: 직접 처리 불가 -> 담당 전달 처리"
        ),
        "required_slots": [
            "view_order_history(U002)",
            "5월 10일",
            "무선 이어폰",
            "4567891230",
            "view_user_profile(U002)",
            "Gold",
            "10% 할인 쿠폰",
            "search_policy_info(환불)",
            "7일",
            "search_policy_info(주문 취소)",
            "결제 취소",
        ],
    },
    {
        "name": "Login OTP fallback call (13 turns, English)",
        "dataset": "NebulaByte/E-Commerce_Customer_Support_Conversations",
        "row_ref": "train[0]",
        "prose": (
            "Agent: Thank you for calling BrownBox Customer Support. My name is Tom. How may I assist you today?\n\n"
            "Customer: Hi Tom, I'm trying to log in to my account to purchase an Oven Toaster Grill (OTG), "
            "but I'm unable to proceed as it's asking for mobile number or email verification. Can you help me with that?\n\n"
            "Agent: Sure, I can assist you with that. May I know your registered mobile number or email address, please?\n\n"
            "Customer: My registered mobile number is +1 123-456-7890.\n\n"
            "Agent: Thank you. Let me check that for you. I'm sorry to inform you that we don't have this number on our records. "
            "Can you please confirm if this is the correct number?\n\n"
            "Customer: Oh, I'm sorry. I might have registered with a different number. "
            "Can you please check with my email address instead? It's johndoe@email.com.\n\n"
            "Agent: Sure, let me check that for you. (After a few moments) I see that we have your email address on our records. "
            "We'll be sending you a verification code shortly. Please check your email and let me know once you receive it.\n\n"
            "Customer: Okay, I received the code. What do I do with it?\n\n"
            "Agent: Please enter the verification code in the field provided and click on 'Verify'. "
            "Once your email address is verified, you'll be able to proceed with your purchase.\n\n"
            "Customer: Okay, I entered the code, and it's verified now. Thank you for your help.\n\n"
            "Agent: You're welcome. Is there anything else I can assist you with?\n\n"
            "Customer: No, that's all. Thank you.\n\n"
            "Agent: You're welcome. Have a great day!"
        ),
        "formula": (
            "login blocked by mobile | email verification for Oven Toaster Grill (OTG) -> "
            "check mobile +1 123-456-7890 => not on records -> "
            "check email johndoe@email.com => found & send verification code -> "
            "enter code & click Verify => verified & purchase unlocked"
        ),
        "required_slots": [
            "Oven Toaster Grill (OTG)",
            "+1 123-456-7890",
            "not on records",
            "johndoe@email.com",
            "verification code",
            "Verify",
            "verified",
        ],
    },
]


def compress_prompt(text: str) -> str:
    """Convert a prose English or Korean task prompt into a BPE-aligned formula."""
    s = text.strip()
    if not s:
        return ""
    has_ko = bool(re.search(r"[가-힣]", s))
    s = re.sub(r"이슈\s*#?(\d+)\s*(?:을|를|의|이|가|은|는)?", r"#\1 ", s)
    s = re.sub(
        r"비평\s*(\d+)\s*개\s*이상\s*하드닝\s*라운드를\s*심각도가?\s*Low\s*이하만?\s*남을\s*때\s*까지\s*반복하고",
        r"loop(비평 >=\1 하드닝, until 심각도 <= Low) ->",
        s,
    )
    s = re.sub(r"(\d+)\s*(?:개|가지)\s*이상\s*(?:찾아서|발굴해서)", r">=\1 발굴 ->", s)
    s = re.sub(
        r"(?:라운드를\s*)?(\d+)\s*(?:회|라운드)\s*(?:더\s*)?(?:진행하고|진행해서|반복하고)",
        r"loop \1x ->",
        s,
    )
    s = re.sub(r"(\d+)\s*축\s*루브릭\s*평가(?:를\s*해서)?", r"\1축 루브릭 평가 ->", s)

    replacements = [
        ("Hey, could you please help me ", ""),
        ("Could you please ", ""),
        ("Please ", ""),
        ("Thanks!", ""),
        ("남은 작업을 모두 조치하기위한 계획을 세워서", "남은작업 계획수립 ->"),
        ("남은 작업에 대한 계획을 세워서", "남은작업 계획수립 ->"),
        ("완료할수 있도록", ""),
        ("완료할 수 있도록", ""),
        ("완료되면", "->"),
        ("이슈를 닫을수 있도록 하자", "-> 이슈닫기"),
        ("다음은 절감액이다", "절감액 기반"),
        ("더 보강이 필요한것", "보강점"),
        ("보강이 필요한것", "보강점"),
        ("더 최적화 할수 있는 부분을", "최적화"),
        ("개선점을", "개선점"),
        ("모두 목록으로 나열한 다음", "-> 목록 나열 ->"),
        ("목록으로 보여줘", "-> 목록 보고"),
        ("결과를 수치화된 표로 보고한 다음", "-> 수치표 보고 ->"),
        ("수치화된 표 형태로 보여줄래?", "-> 수치표 보고"),
        ("수치화된 표로 보고해", "-> 수치표 보고"),
        ("커밋 푸시 머지까지 진행해", "-> commit & push & merge"),
        ("커밋 푸시 머지까지 완료해", "-> commit & push & merge"),
        ("푸시 머지까지 해서", "-> push & merge"),
        ("푸시 머지 후", "-> push & merge ->"),
        ("모두 조치하는", "-> 모두조치 ->"),
        ("모두 조치하고", "-> 모두조치 ->"),
        ("모두 조치하자", "-> 모두조치"),
        ("모두 보강한 다음", "-> 모두보강 ->"),
        ("모두 적용하고", "모두적용 ->"),
        ("계획을 세워서", "-> 계획수립 ->"),
        ("보고해", "보고"),
    ]
    for old, new in replacements:
        s = s.replace(old, new)

    s = re.sub(r"\s+", " ", s).strip().rstrip(".?")
    s = re.sub(r"(?:->\s*){2,}", "-> ", s).strip(" ->")
    return f"[ko] :: {s}" if has_ko else s


def print_tables() -> None:
    enc_o200k = tiktoken.get_encoding("o200k_base")
    enc_cl100k = tiktoken.get_encoding("cl100k_base")
    print(f"tiktoken {tiktoken.__version__}\n")

    print("=== 1. Prose vs Unicode math vs BPE-aligned ASCII formula ===")
    base_o200k = len(enc_o200k.encode(next(iter(STYLE_COMPARISON.values()))))
    print(f"{'Style':<30} {'Bytes':>6} {'o200k_base':>11} {'vs Prose':>9} {'cl100k_base':>12}")
    for label, text in STYLE_COMPARISON.items():
        t200 = len(enc_o200k.encode(text))
        tcl = len(enc_cl100k.encode(text))
        delta = 100.0 * (t200 - base_o200k) / base_o200k
        print(f"{label:<30} {len(text.encode()):>6} {t200:>11} {delta:>+8.1f}% {tcl:>12}")

    print("\n=== 2. Korean prose vs Korean core-noun + BPE formula ===")
    print(f"{'Case':<28} {'o200k (Prose->Formula)':>24} {'cl100k (Prose->Formula)':>25}")
    tot_p200 = tot_f200 = tot_pcl = tot_fcl = 0
    for name, prose, formula in KOREAN_WORKFLOW_CASES:
        p200, f200 = len(enc_o200k.encode(prose)), len(enc_o200k.encode(formula))
        pcl, fcl = len(enc_cl100k.encode(prose)), len(enc_cl100k.encode(formula))
        tot_p200 += p200
        tot_f200 += f200
        tot_pcl += pcl
        tot_fcl += fcl
        d200 = 100.0 * (f200 - p200) / p200
        dcl = 100.0 * (fcl - pcl) / pcl
        print(f"{name:<28} {p200:>3} -> {f200:<3} ({d200:>+5.1f}%)   {pcl:>3} -> {fcl:<3} ({dcl:>+5.1f}%)")
    print(
        f"{'Total':<28} {tot_p200:>3} -> {tot_f200:<3} ({100.0*(tot_f200-tot_p200)/tot_p200:>+5.1f}%)   "
        f"{tot_pcl:>3} -> {tot_fcl:<3} ({100.0*(tot_fcl-tot_pcl)/tot_pcl:>+5.1f}%)"
    )


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--compress", help="convert a prose prompt into a BPE-aligned formula and count tokens")
    args = ap.parse_args()

    if args.compress:
        enc_o200k = tiktoken.get_encoding("o200k_base")
        enc_cl100k = tiktoken.get_encoding("cl100k_base")
        formula = compress_prompt(args.compress)
        in_200, out_200 = len(enc_o200k.encode(args.compress)), len(enc_o200k.encode(formula))
        in_cl, out_cl = len(enc_cl100k.encode(args.compress)), len(enc_cl100k.encode(formula))
        print(f"Input  (o200k={in_200}, cl100k={in_cl}): {args.compress}")
        print(f"Output (o200k={out_200} [{100*(out_200-in_200)/max(1,in_200):+.1f}%], cl100k={out_cl} [{100*(out_cl-in_cl)/max(1,in_cl):+.1f}%]): {formula}")
        return

    print_tables()


if __name__ == "__main__":
    main()
