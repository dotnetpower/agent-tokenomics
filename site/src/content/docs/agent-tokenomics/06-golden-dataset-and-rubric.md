---
title: "6. Golden dataset and rubric evaluation"
description: "Design an Excel/CSV Golden Dataset for single-turn and multi-turn LLM agents, combine keyword and intent matching, and score quality with a rubric."
track: agent
verified: 2026-11-06
sidebar:
  order: 6
---

**Goal:** build a **repeatable quality guardrail** that verifies your agent preserves both answer accuracy and multi-turn memory after you cut tokens.

---

## 1. How to define a "correct answer": Keyword match vs. Intent match

Using a single grading method across all agent tasks breaks down in practice. Combine **four complementary checks** in a **hybrid grading pipeline**:

| Check type | When to use it | How it works (example) | Evaluation token cost |
|------------|----------------|------------------------|-----------------------|
| **1. Required keywords & slots (`must_include_all`)** | Missing an exact identifier, number, boundary operator, or file path makes the answer wrong | `checkout.py;>= 100` (all items must appear) | **0 tokens** (string / regex) |
| **2. Synonym groups (`must_include_any`)** | Phrasing can vary, but at least one term from each concept group must appear | `weekday\|business day\|Monday;exclude weekends` | **0 tokens** (string / regex) |
| **3. Forbidden phrases & tool checks (`must_not_include` / `expected_tools`)** | Blocking hallucinated promises, policy violations, unsafe commands, or skipped tool calls | Forbidden: `full refund approved`, Required tool: `update_shipping_address` | **0 tokens** (log / string) |
| **4. Intent & semantic rubric (`expected_intent` + Rubric)** | Judging whether differently worded prose reaches the right conclusion and respects constraints | `"Politely decline full refund past the 7-day window and offer inspection instead"` | **Low** (LLM-as-a-Judge) |

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row">
      <span>Stage 1 (0 tok)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-hit" style="width:100%">Deterministic check: must_include_all · must_include_any · must_not_include · expected_tools</div>
      </div>
      <span>Fast filter</span>
    </div>
    <div class="at-diag-row">
      <span>Stage 2 (Judge)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-prefix" style="width:72%">5-Axis Rubric (Intent · Facts/Slots · Multi-Turn · Tools/Policy · Conciseness, 10 pts)</div>
        <div class="at-diag-seg seg-user" style="width:28%">Verbosity Guard</div>
      </div>
      <span>Semantic check</span>
    </div>
  </div>
  <figcaption>
    Figure 1. Two-stage hybrid evaluation pipeline: zero-cost deterministic keyword and tool checks run first, followed by a 5-axis, 10-point intent rubric with a verbosity-bias guard.
  </figcaption>
</figure>

---

## 2. How to evaluate multi-turn conversations

When testing token optimizations (such as `/compact`, observation masking, or tool pruning) across multi-turn dialogues (`Turn 1 → Turn 2 → Turn 3`), live free-form rollouts often diverge on Turn 1 — making it impossible to compare Turn 2 on identical inputs. Use two complementary modes:

### Mode A: Pinned-History Slicing (recommended default for before/after comparisons)

- **How it works:** Freeze `Turn 1 .. Turn k-1` (including prior tool outputs or summaries) in `prior_turns_context`, and compare the baseline and candidate only on **Turn `k`**.
- **Testing multi-turn memory retention (Planted Facts):**
  - **Turn 1 (Setup):** Plant a critical constraint or identifier (`"I need to change the address for order ORD-901, and I cannot receive packages on weekends"`).
  - **Turn 2 (Memory Check):** Ask the follow-up without repeating the constraint (`"Change it to 123 Teheran-ro, Gangnam-gu, and tell me when it will arrive"`).
  - Verify that after context compaction or prompt compression, the agent still remembers both `ORD-901` and the **no-weekend delivery** constraint on Turn 2.

### Mode B: End-to-End Trajectory Simulation (`pass^k`)

- When evaluating loop reduction or autonomous multi-step completion, run a scripted user scenario across up to `N` turns and measure end-to-end task completion consistency (`pass^k`, [tau-bench, arXiv:2406.12045](https://arxiv.org/abs/2406.12045)) alongside total session tokens.

---

## 3. Organizing the Golden Dataset in Excel (`.xlsx` / `.csv`)

To let engineers, PMs, and domain experts edit and review test cases together in Microsoft Excel without wrestling with nested JSON, use **one row per evaluated turn** in an `.xlsx` workbook and group multi-turn dialogues by `session_id` and `turn_index` (`1..5`):

- **Download the actual Excel workbook (2 sheets: `Golden_Dataset` 16 rows + `Rubric_Criteria` guide)**:
  - [📥 Download `sample-golden-dataset.xlsx` (Excel Workbook)](https://github.com/dotnetpower/agent-tokenomics/raw/main/examples/sample-golden-dataset.xlsx)
  - [📥 Download `sample-golden-dataset.csv` (UTF-8 BOM CSV)](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-golden-dataset.csv) · [Generator & grader `golden_dataset_tool.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/golden_dataset_tool.py)

| Column name | Format | Purpose & example |
|-------------|--------|-------------------|
| `case_id` | `HF-ECOM5-T2` | Unique row ID (`session_id` + turn number) |
| `session_id` | `HF-ECOM5` | Groups turns belonging to the same conversation |
| `turn_index` | `1` .. `5` | Turn order within the session |
| `hf_dataset` / `hf_row` | `iamjoon/...` / `train[5]` | Source Hugging Face dataset name and split/row index |
| `slice` | `multi_turn_tool` | Workload slice (`multi_turn_tool`, `multi_turn_memory`, `policy_guardrail`, `legal_statute_qa`) |
| `match_mode` | `hybrid` | `keyword_only`, `intent_only`, or `hybrid` |
| `prior_turns_context` | `[T1 User] ... [T1 Tool] ...` | Pinned conversation/tool history before this turn (empty for Turn 1) |
| `user_input` | Text | Current user prompt for this turn |
| `must_include_all` | `;`-separated | Mandatory keywords, numbers, or IDs (`5월 10일;무선 이어폰;4567891230`) |
| `must_include_any` | `;` groups, `\|` synonyms | Synonym groups where at least one per group must match (`희망 날짜\|희망하는 날짜;모의 서비스\|가상`) |
| `must_not_include` | `;`-separated | Forbidden strings that trigger immediate failure (`제가 바로 결제 취소해 드렸습니다`) |
| `expected_tools` | `;`-separated | Tools that must be called on this turn (`view_order_history;view_order_details`) |
| `expected_intent` | 1-sentence statement | **Intent grading reference:** the core meaning and conclusion required to pass |
| `baseline_response` / `baseline_tokens` | Text / integer | Pre-optimization response text and token count |
| `candidate_response` / `candidate_tokens` | Text / integer | Post-optimization response text and token count |

### 16 Golden Dataset rows drawn from 6 public Hugging Face datasets (`sample-golden-dataset.xlsx` Sheet 1)

| Case (Turn) | Hugging Face dataset (`hf_row`) | Mode / Tools | `must_include_all` (slots) | `must_include_any` (synonyms) | `expected_intent` (intent rubric target) |
|---|---|---|---|---|---|
| **`HF-ECOM5-T1`** (1/5) | [`iamjoon/ecommerce-...`](https://huggingface.co/datasets/iamjoon/ecommerce-function-calling-datasets-korean) (`train[5]`) | `hybrid`<br>`view_order_history` | `5월 10일`<br>`무선 이어폰`<br>`4567891230` | `한진택배`<br>`배송 중\|배송중` | Call `view_order_history(U002)` and report May 10 wireless earbuds shipping via Hanjin (`4567891230`) |
| **`HF-ECOM5-T2`** (2/5) | `iamjoon/ecommerce-...` (`train[5]`) | `hybrid`<br>`view_user_profile` | `Gold`<br>`10%` | `할인 쿠폰\|쿠폰` | Retain `U002` from Turn 1, call `view_user_profile(U002)`, and report `Gold` tier + `10%` coupon |
| **`HF-ECOM5-T3`** (3/5) | `iamjoon/ecommerce-...` (`train[5]`) | `hybrid`<br>`search_policy_info` | `7일`<br>`왕복 배송비` | `수령 후\|수령일`<br>`고객센터\|신청` | Call `search_policy_info(환불)` and state the 7-day post-receipt refund window and round-trip shipping fee |
| **`HF-ECOM5-T4`** (4/5) | `iamjoon/ecommerce-...` (`train[5]`) | `hybrid`<br>`search_policy_info` | `반품` | `출고되기 전\|출고 전`<br>`출고 이후\|출고 후`<br>`주문취소 버튼\|주문내역` | Distinguish pre-shipment cancellation (order history button) from post-shipment return |
| **`HF-ECOM5-T5`** (5/5) | `iamjoon/ecommerce-...` (`train[5]`) | `intent_only`<br>*(no tool)* | `담당자` | `직접 처리할 수 없\|직접 처리 불가`<br>`전달` | State that the AI agent cannot cancel payment directly and escalate to a human representative |
| **`HF-ECOM2-T1`** (1/4) | `iamjoon/ecommerce-...` (`train[2]`) | `hybrid`<br>`show_cart` | `블루투스 이어폰`<br>`무선 마우스` | `1개;2개` | Call `show_cart(U001)` and report 1 Bluetooth earbud + 2 wireless mice |
| **`HF-ECOM2-T2`** (2/4) | `iamjoon/ecommerce-...` (`train[2]`) | `hybrid`<br>`view_order_history`<br>`view_order_details` | `고성능 노트북`<br>`1,150,000원` | `1대;구매` | Retain `U001`, chain `view_order_history` → `view_order_details(O102)`, and confirm laptop purchase (`1,150,000원`) |
| **`HF-ECOM2-T3`** (3/4) | `iamjoon/ecommerce-...` (`train[2]`) | `intent_only`<br>*(no tool)* | `담당자` | `기술적인 지원\|기술 지원`<br>`전달` | Identify repeated power-off as requiring technical support and escalate to the specialist team |
| **`HF-ECOM2-T4`** (4/4) | `iamjoon/ecommerce-...` (`train[2]`) | `hybrid`<br>*(no tool)* | `카드 영수증`<br>`담당자` | `직접 처리할 수 없\|직접 처리 불가`<br>`전달` | State that card receipt issuance cannot be handled directly by the bot and forward to staff |
| **`HF-DELIV-T1`** (1/1) | [`jjun123/delivery-app-...`](https://huggingface.co/datasets/jjun123/delivery-app-function-calling-datasets-korean) (`train[0]`) | `hybrid`<br>`create_delivery_order` | `14,900원;7,000원`<br>`21,900원;이서준`<br>`e2a4b1c9-...` | `관악구 관악로 1`<br>`준비 중\|준비중`<br>`담당자` | Summarize items, total (`21,900원`), recipient (`이서준`), order UUID, preparation status, and escalate ETA check |
| **`HF-DFM14-T1`** (1/1) | [`schneiderkamplab/dfm14-...`](https://huggingface.co/datasets/schneiderkamplab/dfm14-ko-synthetic-tool-dialogue-r1) (`train[2]`) | `keyword_only`<br>`lookup_parcel`<br>`redirect_parcel` | `parcel-610`<br>`locker-16`<br>`redirect-610` | `변경 처리가 완료\|변경 완료` | Chain `lookup_parcel(10610, tracking-610)` → `redirect_parcel(locker-16, parcel-610)` and return `redirect-610` |
| **`HF-CLINIC-T1`** (1/1) | `schneiderkamplab/dfm14-...` (`train[11]`) | `hybrid`<br>*(no tool)* | `병원 이름`<br>`서비스 종류` | `희망 날짜\|희망하는 날짜`<br>`모의 서비스\|가상`<br>`예약 가능 시간대` | Clarify that the service is a mock slot lookup (no real booking or medical advice) |
| **`HF-KLAC-T1`** (1/1) | [`jihye-moon/klac_legal_aid...`](https://huggingface.co/datasets/jihye-moon/klac_legal_aid_counseling) (`train[1]`) | `hybrid`<br>*(no tool)* | `제34조;제4조`<br>`1년;4주;15시간` | `근로기준법`<br>`근로자퇴직급여 보장법`<br>`퇴직연금` | Cite Labor Standards Act Art. 34 & Retirement Benefit Act Art. 4 plus `< 1 year` and `< 15h/week` exclusions |
| **`HF-KLAC-T2`** (1/1) | `jihye-moon/klac_legal_aid...` (`train[0]`) | `hybrid`<br>*(no tool)* | `최저임금법`<br>`제6조;무효;90%` | `3월\|3개월`<br>`수습` | Cite Minimum Wage Act Art. 6 (sub-minimum agreements void) and `90%` probation rule (`≤ 3 months`) |
| **`HF-LABOR-T1`** (1/1) | [`seventy4/labor-counsel-qa`](https://huggingface.co/datasets/seventy4/labor-counsel-qa) (`train[34]`) | `hybrid`<br>*(no tool)* | `30일;30일분`<br>`통상임금;3개월` | `천재·사변\|부득이한 사유`<br>`예외` | State 30-day dismissal notice or `≥ 30 days` ordinary wage rule and `< 3 months` / force-majeure exceptions |
| **`HF-OSCE-T1`** (2/2) | [`nwirandx/Korean-OSCE-...`](https://huggingface.co/datasets/nwirandx/Korean-OSCE-history-taking-dialogues) (`sessions[0..9]`) | `hybrid`<br>*(no tool)* | `김서현`<br>`1984년 5월 23일`<br>`20일;15분;20분;30분` | `과거력 없음\|과거력은 없`<br>`어지럽고\|어지러움`<br>`숨이 차\|호흡곤란` | Summarize 9 turns of clinical intake (patient info, 20-day onset, 15–20m → >30m progression, no past history, dizziness/dyspnea) |

---

## 4. Reviewing & designing a 10-point Rubric (5 axes × 0–2 points = 10 points max)

Unanchored 1–10 scales drift between judge runs. Instead, decompose the 10-point score into **5 orthogonal axes × 0 / 1 / 2 points = 10 points max**. Separating **current-turn fact/slot accuracy (Axis 2)** from **multi-turn context retention (Axis 3)** and **tool/policy compliance (Axis 4)** pinpoints exactly which optimization caused a regression:

| Rubric axis (5 Axes × 2 pts = 10 pts) | 0 pts (Fail — critical defect) | 1 pt (Partial) | 2 pts (Pass — full credit) | What it guards against |
|---|---|---|---|---|
| **1. Intent & core resolution**<br>(`intent_and_resolution`) | Misreads user intent or reaches a conclusion contrary to `expected_intent` | Captures intent but omits part of the required resolution or next step | Fully satisfies `expected_intent` regardless of phrasing | Semantic correctness when wording differs (`intent_only` / `hybrid`) |
| **2. Fact, slot & numerical accuracy**<br>(`fact_and_slot_accuracy`) | Drops/distorts required IDs, numbers, dates, or statutes, or hallucinates facts | Core facts hold, but secondary numbers or conditions are vague | States all required keywords, numbers, IDs, and statutes accurately | Slot/number loss during BPE or prompt compression (`keyword` / `hybrid`) |
| **3. Multi-turn context & state retention**<br>(`multiturn_context_retention`) | Forgets constraints or IDs from `prior_turns_context`, contradicting or re-asking | Partially recalls prior turns or links context loosely | Consistently retains user IDs, planted constraints, and state across turns | Memory loss after `/compact` or observation masking |
| **4. Tool selection & policy guardrails**<br>(`tool_and_policy_compliance`) | Skips required tools and fabricates results, or violates `must_not_include` / policy | Obeys safety boundaries but makes redundant tool calls or minor format errors | Calls the right tools (or escalates out-of-scope actions) and obeys all policies | Over-pruned tool schemas or weakened guardrail prompts |
| **5. Concise completeness (Verbosity guard)**<br>(`concise_completeness`) | Omits essential details so the user must ask again, or produces broken text | Complete but padded with repetitive greetings, apologies, or filler | **Complete, actionable, and concise — awards full marks to shorter answers** | **Prevents LLM judges from favoring bloated replies** |

> **Recommended pass threshold:** A turn passes (`PASS`) only when all deterministic checks succeed **and** the 5-axis rubric total is **≥ 8 / 10** with no `0`-point axis.

---

## 5. Runnable Python `.xlsx` / `.csv` generator & evaluator (`examples/golden_dataset_tool.py`)

[`examples/golden_dataset_tool.py`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/golden_dataset_tool.py) uses only the Python standard library to generate a native two-sheet Microsoft Excel workbook ([`examples/sample-golden-dataset.xlsx`](https://github.com/dotnetpower/agent-tokenomics/raw/main/examples/sample-golden-dataset.xlsx): `Golden_Dataset` + `Rubric_Criteria` 10-point guide) and a `utf-8-sig` CSV ([`examples/sample-golden-dataset.csv`](https://github.com/dotnetpower/agent-tokenomics/blob/main/examples/sample-golden-dataset.csv)), evaluate all 16 Hugging Face single-turn and multi-turn rows directly from `.xlsx` or `.csv`, and print the 10-point LLM-as-a-Judge prompt:

```bash
# 1. Generate the two-sheet Excel workbook (.xlsx) and UTF-8 BOM CSV (.csv)
python3 examples/golden_dataset_tool.py --write-sample

# 2. Evaluate Baseline vs. Candidate directly from the .xlsx workbook
python3 examples/golden_dataset_tool.py --eval examples/sample-golden-dataset.xlsx

# 3. Print the 10-point LLM-as-a-Judge rubric prompt for a 5-turn Hugging Face case (HF-ECOM5-T5)
python3 examples/golden_dataset_tool.py --print-judge-prompt HF-ECOM5-T5
```

### Sample evaluation output (`examples/sample-golden-dataset.xlsx`, 16 rows)

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

## Further reading (papers)

- [Cost-of-Pass: An Economic Framework for Evaluating Language Models](https://arxiv.org/abs/2504.13359) — preprint 2025. Evaluates models by expected cost per successful task (`Cost-of-Pass`) rather than per-call price alone.
- [$\tau$-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains](https://arxiv.org/abs/2406.12045) — preprint 2024. Evaluates multi-turn tool calls and policy compliance across repeat trials (`pass^k`).
- [AI Agents That Matter](https://arxiv.org/abs/2407.01502) — preprint 2024. Establishes joint cost-and-accuracy reporting and anti-contamination practices for agent benchmarks.
- [Adding Error Bars to Evals: A Statistical Approach to Language Model Evaluations](https://arxiv.org/abs/2411.00640) — preprint 2024. Computes paired confidence intervals and sample-size requirements for evaluations.
- [LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory](https://arxiv.org/abs/2410.10813) — ICLR 2025. Benchmarks multi-turn fact retention and knowledge updates across long sessions.
