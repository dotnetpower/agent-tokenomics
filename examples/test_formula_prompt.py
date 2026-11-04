"""Automated tests for BPE-aligned formula prompt and counselor dialogue compression."""

from __future__ import annotations

from pathlib import Path
import sys
import urllib.error

import pytest
import tiktoken

sys.path.insert(0, str(Path(__file__).resolve().parent))
import formula_prompt as fp  # noqa: E402
import formula_prompt_en as fp_en  # noqa: E402
import formula_prompt_ko as fp_ko  # noqa: E402


@pytest.fixture(scope="module")
def enc_o200k() -> tiktoken.Encoding:
    return tiktoken.get_encoding("o200k_base")


@pytest.fixture(scope="module")
def enc_cl100k() -> tiktoken.Encoding:
    return tiktoken.get_encoding("cl100k_base")


def test_style_comparison_token_counts(
    enc_o200k: tiktoken.Encoding,
    enc_cl100k: tiktoken.Encoding,
) -> None:
    expected = {
        "Prose (with backticks)": (150, 31, 31),
        "Unicode math (unspaced)": (75, 30, 31),
        "ASCII formula (unspaced)": (75, 20, 20),
        "BPE-aligned formula (spaced)": (55, 14, 14),
    }
    for label, (exp_bytes, exp_200, exp_cl) in expected.items():
        text = fp.STYLE_COMPARISON[label]
        assert len(text.encode("utf-8")) == exp_bytes
        assert len(enc_o200k.encode(text)) == exp_200
        assert len(enc_cl100k.encode(text)) == exp_cl


def test_english_workflow_cases_and_compressor(
    enc_o200k: tiktoken.Encoding,
    enc_cl100k: tiktoken.Encoding,
) -> None:
    tot_p200 = tot_f200 = tot_pcl = tot_fcl = 0
    for name, prose, expected_formula in fp_en.ENGLISH_WORKFLOW_CASES:
        actual = fp_en.compress_prompt_en(prose)
        assert actual == expected_formula, f"Mismatch in {name}"
        assert fp.compress_prompt(prose) == expected_formula
        p200, f200 = len(enc_o200k.encode(prose)), len(enc_o200k.encode(actual))
        pcl, fcl = len(enc_cl100k.encode(prose)), len(enc_cl100k.encode(actual))
        assert f200 < p200, f"No o200k token reduction for {name}"
        assert fcl < pcl, f"No cl100k token reduction for {name}"
        tot_p200 += p200
        tot_f200 += f200
        tot_pcl += pcl
        tot_fcl += fcl

    assert (tot_p200, tot_f200) == (149, 69)
    assert (tot_pcl, tot_fcl) == (149, 69)


def test_korean_workflow_cases_and_compressor(
    enc_o200k: tiktoken.Encoding,
    enc_cl100k: tiktoken.Encoding,
) -> None:
    tot_p200 = tot_f200 = tot_pcl = tot_fcl = 0
    for name, prose, expected_formula in fp_ko.KOREAN_WORKFLOW_CASES:
        actual = fp_ko.compress_prompt_ko(prose)
        assert actual == expected_formula, f"Mismatch in {name}"
        assert fp.compress_prompt(prose) == expected_formula
        p200, f200 = len(enc_o200k.encode(prose)), len(enc_o200k.encode(actual))
        pcl, fcl = len(enc_cl100k.encode(prose)), len(enc_cl100k.encode(actual))
        assert f200 < p200, f"No o200k token reduction for {name}"
        assert fcl < pcl, f"No cl100k token reduction for {name}"
        tot_p200 += p200
        tot_f200 += f200
        tot_pcl += pcl
        tot_fcl += fcl

    assert (tot_p200, tot_f200) == (175, 122)
    assert (tot_pcl, tot_fcl) == (288, 167)


def test_korean_leading_space_bpe_merge_invariant(enc_o200k: tiktoken.Encoding) -> None:
    """Core 2-syllable Korean counseling nouns merge into 1 token only with a leading space."""
    single_token_with_leading_space = [
        "고객",
        "담당",
        "주문",
        "완료",
        "직접",
        "처리",
        "확인",
        "전달",
        "안내",
        "기준",
    ]
    for word in single_token_with_leading_space:
        assert len(enc_o200k.encode(" " + word)) == 1, f"' {word}' should be 1 token in o200k_base"
        assert len(enc_o200k.encode(word)) == 2, f"'{word}' without leading space should split into 2 tokens"

    # Spaced Korean noun pairs use fewer tokens than unspaced compound nouns
    for unspaced, spaced in [
        (" 직접처리", " 직접 처리"),
        (" 주문완료", " 주문 완료"),
        (" 반품처리", " 반품 처리"),
        (" 최저기준", " 최저 기준"),
    ]:
        assert len(enc_o200k.encode(spaced)) < len(enc_o200k.encode(unspaced))


def test_hf_english_turn_and_dialogue_cases(
    enc_o200k: tiktoken.Encoding,
    enc_cl100k: tiktoken.Encoding,
) -> None:
    assert len(fp_en.HF_ENGLISH_TURN_CASES) == 7
    tot_p200 = tot_f200 = tot_pcl = tot_fcl = tot_slots = 0
    for item in fp_en.HF_ENGLISH_TURN_CASES:
        actual = fp_en.compress_prompt_en(item["prose"])
        assert actual == item["formula"], f"Formula mismatch in {item['name']}"
        assert fp.compress_prompt(item["prose"]) == item["formula"]
        kept, total, missing = fp.check_slot_retention(actual, item["required_slots"])
        assert kept == total and not missing, f"Missing slots in {item['name']}: {missing}"
        strict_slots = fp.extract_numeric_and_id_tokens(item["prose"])
        s_kept, s_tot, s_miss = fp.check_slot_retention(actual, strict_slots)
        assert s_kept == s_tot and not s_miss, f"Missing strict slots in {item['name']}: {s_miss}"

        p200, f200 = len(enc_o200k.encode(item["prose"])), len(enc_o200k.encode(actual))
        pcl, fcl = len(enc_cl100k.encode(item["prose"])), len(enc_cl100k.encode(actual))
        assert f200 < p200
        assert fcl < pcl
        tot_p200 += p200
        tot_f200 += f200
        tot_pcl += pcl
        tot_fcl += fcl
        tot_slots += total

    assert (tot_p200, tot_f200) == (986, 238)
    assert (tot_pcl, tot_fcl) == (1002, 239)
    assert tot_slots == 43

    assert len(fp_en.HF_ENGLISH_DIALOGUE_CASES) == 6
    d_p200 = d_f200 = d_pcl = d_fcl = d_slots = 0
    for item in fp_en.HF_ENGLISH_DIALOGUE_CASES:
        actual = fp_en.compress_dialogue_en(item["prose"])
        assert actual == item["formula"], f"Dialogue formula mismatch in {item['name']}"
        assert fp.compress_dialogue(item["prose"]) == item["formula"]
        kept, total, missing = fp.check_slot_retention(actual, item["required_slots"])
        assert kept == total and not missing, f"Missing slots in {item['name']}: {missing}"
        strict_slots = fp.extract_numeric_and_id_tokens(item["prose"])
        s_kept, s_tot, s_miss = fp.check_slot_retention(actual, strict_slots)
        assert s_kept == s_tot and not s_miss, f"Missing strict slots in {item['name']}: {s_miss}"

        p200, f200 = len(enc_o200k.encode(item["prose"])), len(enc_o200k.encode(actual))
        pcl, fcl = len(enc_cl100k.encode(item["prose"])), len(enc_cl100k.encode(actual))
        assert f200 < p200
        assert fcl < pcl
        d_p200 += p200
        d_f200 += f200
        d_pcl += pcl
        d_fcl += fcl
        d_slots += total

    assert (d_p200, d_f200) == (2094, 349)
    assert (d_pcl, d_fcl) == (2139, 350)
    assert d_slots == 45


def test_hf_korean_turn_and_dialogue_cases(
    enc_o200k: tiktoken.Encoding,
    enc_cl100k: tiktoken.Encoding,
) -> None:
    assert len(fp_ko.HF_KOREAN_TURN_CASES) == 10
    tot_p200 = tot_f200 = tot_pcl = tot_fcl = tot_slots = 0
    for item in fp_ko.HF_KOREAN_TURN_CASES:
        actual = fp_ko.compress_prompt_ko(item["prose"])
        assert actual == item["formula"], f"Formula mismatch in {item['name']}"
        assert fp.compress_prompt(item["prose"]) == item["formula"]
        kept, total, missing = fp.check_slot_retention(actual, item["required_slots"])
        assert kept == total and not missing, f"Missing slots in {item['name']}: {missing}"
        strict_slots = fp.extract_numeric_and_id_tokens(item["prose"])
        s_kept, s_tot, s_miss = fp.check_slot_retention(actual, strict_slots)
        assert s_kept == s_tot and not s_miss, f"Missing strict slots in {item['name']}: {s_miss}"

        p200, f200 = len(enc_o200k.encode(item["prose"])), len(enc_o200k.encode(actual))
        pcl, fcl = len(enc_cl100k.encode(item["prose"])), len(enc_cl100k.encode(actual))
        assert f200 < p200
        assert fcl < pcl
        tot_p200 += p200
        tot_f200 += f200
        tot_pcl += pcl
        tot_fcl += fcl
        tot_slots += total

    assert (tot_p200, tot_f200) == (1100, 621)
    assert (tot_pcl, tot_fcl) == (1703, 929)
    assert tot_slots == 77

    assert len(fp_ko.HF_KOREAN_DIALOGUE_CASES) == 7
    d_p200 = d_f200 = d_pcl = d_fcl = d_slots = 0
    for item in fp_ko.HF_KOREAN_DIALOGUE_CASES:
        actual = fp_ko.compress_dialogue_ko(item["prose"])
        assert actual == item["formula"], f"Dialogue formula mismatch in {item['name']}"
        assert fp.compress_dialogue(item["prose"]) == item["formula"]
        kept, total, missing = fp.check_slot_retention(actual, item["required_slots"])
        assert kept == total and not missing, f"Missing slots in {item['name']}: {missing}"
        strict_slots = fp.extract_numeric_and_id_tokens(item["prose"])
        s_kept, s_tot, s_miss = fp.check_slot_retention(actual, strict_slots)
        assert s_kept == s_tot and not s_miss, f"Missing strict slots in {item['name']}: {s_miss}"

        p200, f200 = len(enc_o200k.encode(item["prose"])), len(enc_o200k.encode(actual))
        pcl, fcl = len(enc_cl100k.encode(item["prose"])), len(enc_cl100k.encode(actual))
        assert f200 < p200
        assert fcl < pcl
        d_p200 += p200
        d_f200 += f200
        d_pcl += pcl
        d_fcl += fcl
        d_slots += total

    assert (d_p200, d_f200) == (2450, 727)
    assert (d_pcl, d_fcl) == (3614, 1046)
    assert d_slots == 66


def test_dialogue_structured_messages_equivalence() -> None:
    messages = [
        {"role": "user", "content": "안녕하세요."},
        {"role": "assistant", "content": "안녕하세요! 상준몰 AI 상담사입니다. 무엇을 도와드릴까요?"},
        {"role": "user", "content": "제 주소가 어떻게 되어 있는지 확인하고 싶어요."},
        {
            "role": "assistant",
            "content": '<tool_call>\n{"name": "view_user_profile", "arguments": {"user_id": "U002"}}\n</tool_call>',
        },
        {
            "role": "user",
            "content": '<tool_response>\n[{"id": "U002", "address": "서울특별시 강남구 테헤란로 123"}]\n</tool_response>',
        },
        {
            "role": "assistant",
            "content": "고객님의 주소는 '서울특별시 강남구 테헤란로 123'으로 등록되어 있습니다. 다른 정보가 필요하신가요?",
        },
    ]
    compressed = fp.compress_dialogue(messages)
    assert compressed == "[ko] :: view_user_profile(U002) => 주소 = 서울특별시 강남구 테헤란로 123"


def test_generalized_unseen_korean_and_english_compression(
    enc_o200k: tiktoken.Encoding,
    enc_cl100k: tiktoken.Encoding,
) -> None:
    """Verify general morphological & discourse rules on unseen Korean and English counseling texts."""
    unseen_cases = [
        (
            "근로기준법 제11조는 상시 5명 이상의 근로자를 사용하는 모든 사업 또는 사업장에 적용됩니다. "
            "다만 동거하는 친족만을 사용하는 사업 또는 사업장과 가사 사용인에게는 적용되지 않습니다."
        ),
        (
            "근로기준법 제23조에 따라 사용자는 업무상 부상이나 질병을 치료하기 위해 휴업한 기간과 그 후 30일 동안, "
            "또는 산전·산후 여성이 법에 따라 휴업한 기간과 그 후 30일 동안 해고할 수 없습니다. "
            "다만 사용자가 법에 따른 일시보상을 했거나 사업을 계속할 수 없게 된 경우에는 예외입니다."
        ),
        (
            "I appreciate that you're seeking assistance with canceling your order tied to the order number "
            "{{Order Number}}, and I'm here to guide you through the cancellation process. Please follow these steps:\n"
            "1. Access Your Account: Log in to your {{Online Company Portal Info}} using your username and password.\n"
            "2. Navigate to Your Orders: Once you're logged in, locate the '{{Online Order Interaction}}' section.\n"
            "3. Select the Order: Find order number {{Order Number}} and click on it.\n"
            "4. Confirm Cancellation: Follow the system prompts to finalize the cancellation.\n"
            "If you have any further questions, reach out during {{Customer Support Hours}} at "
            "{{Customer Support Phone Number}} or on {{Website URL}}."
        ),
    ]
    for raw in unseen_cases:
        has_ko = any("가" <= ch <= "힣" for ch in raw)
        comp = fp.compress_prompt(raw, include_lang_tag=has_ko)
        slots = fp.extract_numeric_and_id_tokens(raw)
        kept, tot, miss = fp.check_slot_retention(comp, slots)
        assert kept == tot and not miss, f"Lost slots {miss} in unseen case"
        assert len(enc_o200k.encode(comp)) < int(len(enc_o200k.encode(raw)) * 0.72)
        assert len(enc_cl100k.encode(comp)) < int(len(enc_cl100k.encode(raw)) * 0.72)


def test_slot_preservation_guardrail() -> None:
    sample = (
        "근로기준법 제34조 및 같은 법 제5조 제2항(대법원 1999. 12. 6. 선고 98다46198 판결), "
        "문의 이메일 jane.doe@email.com, 주문번호 O102, 보관함 locker-16, 템플릿 {{Order Number}}"
    )
    slots = fp.extract_numeric_and_id_tokens(sample)
    for expected_slot in [
        "제34조",
        "제5조",
        "제2항",
        "98다46198",
        "jane.doe@email.com",
        "O102",
        "locker-16",
        "{{Order Number}}",
    ]:
        assert expected_slot in slots
    restored = fp.ensure_slot_preservation("요약본", slots)
    kept, tot, miss = fp.check_slot_retention(restored, slots)
    assert kept == tot and not miss


def test_non_expansion_guard_and_edge_cases(enc_o200k: tiktoken.Encoding) -> None:
    assert fp.compress_prompt("") == ""
    assert fp.compress_dialogue("") == ""
    short = "코드만 출력"
    assert fp.compress_prompt(short) == short
    assert len(enc_o200k.encode(fp.compress_prompt(short))) <= len(enc_o200k.encode(short))


def test_hf_live_datasets_server_integration() -> None:
    try:
        results = fp.run_hf_live_benchmark(rows_per_dataset=50)
    except (RuntimeError, urllib.error.URLError, TimeoutError) as exc:
        pytest.skip(f"Hugging Face Datasets Server unreachable: {exc}")

    assert len(results) == 12
    total_rows = sum(r["rows"] for r in results)
    total_p200 = sum(r["o200k_prose"] for r in results)
    total_f200 = sum(r["o200k_formula"] for r in results)
    total_kept = sum(r["slots_kept"] for r in results)
    total_slots = sum(r["slots_total"] for r in results)

    assert total_rows == 580
    assert total_kept == total_slots == 1731, f"Slot mismatch: {total_kept}/{total_slots}"
    assert total_f200 < int(total_p200 * 0.45), f"Expected >55% overall reduction, got {total_f200}/{total_p200}"
    for res in results:
        assert res["o200k_formula"] < int(res["o200k_prose"] * 0.51), f"Insufficient savings on {res['dataset']}"
        assert res["cl100k_formula"] < int(res["cl100k_prose"] * 0.51), f"Insufficient cl100k savings on {res['dataset']}"
        assert res["slots_kept"] == res["slots_total"], f"Lost slots on {res['dataset']}"
