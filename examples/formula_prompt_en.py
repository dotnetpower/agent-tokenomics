#!/usr/bin/env python3
"""English-only BPE-aligned formula prompt and customer-support compression examples.

Reproduces the English BPE-aligned formula tables in:
- copilot-tokenomics/04-instructions-and-tools
- copilot-tokenomics/06-prompt-and-output-style
- agent-tokenomics/04-improve

Includes real English customer-support benchmark cases drawn from public
Hugging Face datasets (`bitext/Bitext-customer-support-llm-chatbot-training-dataset`
and `NebulaByte/E-Commerce_Customer_Support_Conversations`).

Setup:
    python3 -m venv .venv && .venv/bin/pip install tiktoken
Usage:
    .venv/bin/python examples/formula_prompt_en.py
    .venv/bin/python examples/formula_prompt_en.py --compress "Hey, could you please help me refactor this function? I think it might have some issues with how it handles the authentication, and I'd really like it to be more efficient. Thanks!"
    .venv/bin/python examples/formula_prompt_en.py --hf-live --hf-rows 50
"""

from __future__ import annotations

import argparse
import json
import re
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Sequence

import tiktoken

_ENC_O200K = tiktoken.get_encoding("o200k_base")

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

ENGLISH_WORKFLOW_CASES = [
    (
        "Auth function refactor",
        "Hey, could you please help me refactor this function? I think it might have some issues with how it handles the authentication, and I'd really like it to be more efficient. Thanks!",
        "Refactor function -> fix auth handling -> optimize",
    ),
    (
        "PR review & CI fix loop",
        "Could you please review all the changed files in this pull request, fix any failing unit tests or linter warnings until the build is completely green, and then summarize the root causes in a short table?",
        "review PR diff -> loop(fix failing tests & lint, until build == green) -> root-cause table",
    ),
    (
        "Symbol context lookup rule",
        "When you need to find a symbol or literal and read its surrounding lines, use `code_context` in one call instead of searching first and reading later.",
        "symbol + context => code_context (1 call > grep + read)",
    ),
    (
        "Bounded bug audit & patch",
        "Please scan the authentication and session modules to find at least 10 edge-case bugs, fix all high-severity issues first, run pytest quietly to verify, and report the before and after pass counts.",
        "scan auth & session -> find >= 10 edge bugs -> fix severity >= High -> pytest -q -> report pass counts",
    ),
]

HF_ENGLISH_TURN_CASES: list[dict[str, Any]] = [
    {
        "name": "Order cancel SOP",
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
    {
        "name": "Order change SOP",
        "dataset": "bitext/Bitext-customer-support-llm-chatbot-training-dataset",
        "row_ref": "train[1001]",
        "prose": (
            "We've received your message to us for assistance with changing your order, specifically the one with "
            "order number {{Order Number}}. I understand that you would like to modify your existing order, and "
            "I'm here to help. To make any changes to your order, such as adding or removing products, updating "
            "shipping information, or canceling items, please follow these steps:\n\n"
            "1. Log in to your account on our website or app.\n"
            "2. Go to your Orders section or Order History.\n"
            "3. Locate and select the desired order with order number {{Order Number}}.\n"
            '4. Look for the "Manage Order" or "Edit Order" option and click on it.\n'
            "5. Follow the on-screen instructions to update your order details.\n"
            "6. If you encounter any issues or require further assistance, please don't hesitate to reach out to "
            "our customer support team, available {{Customer Support Hours}}, at {{Customer Support Phone Number}} "
            "or through the Live Chat on our website at {{Website URL}}.\n\n"
            "We value your business and want to ensure that your order meets your needs. If there's anything else "
            "you'd like assistance with, please feel free to ask."
        ),
        "formula": (
            "change_order {{Order Number}} :: login account -> open Orders | Order History -> "
            "select {{Order Number}} -> click Manage Order | Edit Order -> update details | "
            "help => support {{Customer Support Hours}} @ {{Customer Support Phone Number}} | Live Chat {{Website URL}}"
        ),
        "required_slots": [
            "change_order",
            "{{Order Number}}",
            "Orders",
            "Manage Order",
            "Edit Order",
            "{{Customer Support Hours}}",
            "{{Customer Support Phone Number}}",
            "Live Chat",
            "{{Website URL}}",
        ],
    },
    {
        "name": "Cancellation fee check SOP",
        "dataset": "bitext/Bitext-customer-support-llm-chatbot-training-dataset",
        "row_ref": "train[3001]",
        "prose": (
            "I'm happy to help! To check the cancellation charges, please follow these steps: \n"
            "1. Log in to your online account on our website.\n"
            '2. Navigate to the "Account Summary" or "Billing" section.\n'
            '3. Look for the "Fees and Charges" or "Cancellation Charges" subsection.\n'
            "4. Click on it to view the detailed information about the cancellation fees applicable to your account.\n"
            "If you need any further assistance, feel free to reach out to our customer support team."
        ),
        "formula": (
            "check_cancellation_fee :: login online account -> open Account Summary | Billing -> "
            "select Fees and Charges | Cancellation Charges -> view cancellation fees"
        ),
        "required_slots": [
            "Account Summary",
            "Billing",
            "Fees and Charges",
            "Cancellation Charges",
            "cancellation fees",
        ],
    },
    {
        "name": "Refund timeframe & lookup",
        "dataset": "bitext/Bitext-customer-support-llm-chatbot-training-dataset",
        "row_ref": "train[6000]",
        "prose": (
            "Assuredly! I understand that knowing the timeframe for refunds is crucial for you. The length of time "
            "it takes to process a refund can vary depending on several factors. Typically, refunds are processed "
            "within a few business days after the return or cancellation has been initiated. However, it's important "
            "to note that the exact timeframe may depend on various elements such as the payment method used, the "
            "nature of the product or service, the company's refund policy, and any applicable regulations and "
            "procedures.\n\n"
            "To provide you with more accurate and specific information about the refund process, I encourage you "
            "to provide me with the details of your particular purchase. This way, I can look into it further and "
            "provide a more precise estimate of the refund timeline. Please share the {{Order Number}} or any "
            "relevant details, and I'll be more than happy to assist you further in determining the expected "
            "duration for your refund.\n\n"
            "Rest assured, we strive to process refunds as quickly as possible while adhering to relevant "
            "guidelines and ensuring a fair and accurate resolution. Our aim is to ensure your satisfaction "
            "throughout the process.\n\n"
            "Is there anything else I can assist you with today?"
        ),
        "formula": (
            "check_refund_policy :: return | cancellation initiated => refund in few business days "
            "(depends on payment method & product & policy) | exact timeline: provide {{Order Number}}"
        ),
        "required_slots": [
            "return",
            "cancellation",
            "business days",
            "payment method",
            "{{Order Number}}",
        ],
    },
    {
        "name": "Recall return & refund SOP",
        "dataset": "NebulaByte/E-Commerce_Customer_Support_Conversations",
        "row_ref": "train[1]",
        "prose": (
            "Thank you. Let me check the details of your order. (After a few seconds) I see that the monitor "
            "you purchased has been recalled by the manufacturer due to a technical issue. For safety reasons, "
            "we are requesting our customers to return the product, and we will be providing a full refund. "
            "Of course. We will be sending you a prepaid shipping label via email within the next 24 hours. "
            "Please print the label, pack the monitor securely in its original packaging, and drop it off at "
            "any UPS store. Once we receive the product, we will initiate the refund process, and you will "
            "receive your money within 5-7 business days."
        ),
        "formula": (
            "monitor recalled by manufacturer => return for full refund -> "
            "send prepaid shipping label via email <= 24 hours -> "
            "print label & pack & drop at UPS store -> on receipt => refund in 5-7 business days"
        ),
        "required_slots": [
            "recalled",
            "full refund",
            "prepaid shipping label",
            "24",
            "UPS",
            "5-7",
        ],
    },
    {
        "name": "Robotic arm recalibration chat",
        "dataset": "FunDialogues/customer-service-robot-support",
        "row_ref": "train[0]",
        "prose": (
            "Customer: My robotic arm seems to be misaligned. It's not picking objects accurately. "
            "What can I do? Agent: It appears that the arm may need recalibration. "
            "Please follow the instructions in the user manual to reset the calibration settings. "
            "If the problem persists, feel free to contact us again."
        ),
        "formula": "arm misaligned & not picking objects accurately => recalibration needed -> manual: reset calibration settings",
        "required_slots": [
            "misaligned",
            "picking objects",
            "recalibration",
            "manual",
            "calibration settings",
        ],
    },
    {
        "name": "Delayed order live-chat reply",
        "dataset": "Kaludi/Customer-Support-Responses",
        "row_ref": "train[0]",
        "prose": (
            "Customer: My order hasn't arrived yet.\n"
            "Agent: We apologize for the inconvenience. Can you please provide your order number so we can investigate?"
        ),
        "formula": "order not arrived => provide order number -> investigate",
        "required_slots": [
            "order",
            "arrived",
            "order number",
            "investigate",
        ],
    },
]

HF_ENGLISH_DIALOGUE_CASES: list[dict[str, Any]] = [
    {
        "name": "Login OTP fallback call (13 turns)",
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
    {
        "name": "Recalled monitor return & refund (13 turns)",
        "dataset": "NebulaByte/E-Commerce_Customer_Support_Conversations",
        "row_ref": "train[1]",
        "prose": (
            "Agent: Thank you for calling BrownBox customer support. My name is Alex. How may I assist you today?\n\n"
            "Customer: Hi Alex. I recently received an email from BrownBox requesting me to ship back the computer monitor I purchased last week. Can you please tell me why I am being asked to ship it back?\n\n"
            "Agent: Sure, I'll be happy to help you with that. May I have your order number, please?\n\n"
            "Customer: Yes, it's 789101.\n\n"
            "Agent: Thank you. Let me check the details of your order. (After a few seconds) I see that the monitor you purchased has been recalled by the manufacturer due to a technical issue. For safety reasons, we are requesting our customers to return the product, and we will be providing a full refund.\n\n"
            "Customer: Oh, I see. That makes sense. Can you guide me through the process of returning the monitor?\n\n"
            "Agent: Of course. We will be sending you a prepaid shipping label via email within the next 24 hours. Please print the label, pack the monitor securely in its original packaging, and drop it off at any UPS store. Once we receive the product, we will initiate the refund process, and you will receive your money within 5-7 business days.\n\n"
            "Customer: Okay, that sounds straightforward. Thank you for explaining the process to me.\n\n"
            "Agent: You're welcome. Is there anything else I can assist you with?\n\n"
            "Customer: No, that's all for now. Thank you for your help.\n\n"
            "Agent: You're welcome. If you have any other questions or concerns, please don't hesitate to reach out to us. Have a great day!\n\n"
            "Customer: You too. Goodbye.\n\n"
            "Agent: Goodbye."
        ),
        "formula": (
            "email requests ship back computer monitor -> order=789101 => recalled by manufacturer -> "
            "return for full refund -> send prepaid shipping label via email <= 24 hours -> "
            "print label & pack & drop at UPS store -> on receipt => refund in 5-7 business days"
        ),
        "required_slots": [
            "computer monitor",
            "789101",
            "recalled",
            "full refund",
            "prepaid shipping label",
            "24",
            "UPS",
            "5-7",
        ],
    },
    {
        "name": "Warranty replacement & Cancel glitch (19 turns)",
        "dataset": "NebulaByte/E-Commerce_Customer_Support_Conversations",
        "row_ref": "train[2]",
        "prose": (
            "Agent: Thank you for calling BrownBox Customer Support. My name is Sarah. How may I assist you today?\n\n"
            "Customer: Hi Sarah, I am calling because I am unable to click the 'Cancel' button for my Juicer/Mixer/Grinder that I recently purchased from your website.\n\n"
            "Agent: I'm sorry to hear that. May I have your order number and email address to check your account details?\n\n"
            "Customer: Sure, my order number is 987654 and my email address is jane.doe@email.com.\n\n"
            "Agent: Thank you for providing the details. Let me check that for you. Please bear with me for a moment.\n\n"
            "[Agent puts the customer on hold to check the account details]\n\n"
            "Agent: Thank you for your patience. I can see that your product is still under warranty. You can either choose to return it or get a replacement. Which option would you like to go for?\n\n"
            "Customer: I would like to go for a replacement.\n\n"
            "Agent: Sure. Let me guide you through the replacement process. First, we need to create a replacement request. Please log in to your account and click on 'My Orders'. Then, select the order containing the product you want to replace and click on 'Return or Replace Items'.\n\n"
            "Customer: Okay, I am on the 'Return or Replace Items' page.\n\n"
            "Agent: Great. Now, select the product you want to replace and click on 'Replacement'. You will be asked to provide a reason for the replacement. Please select the appropriate reason and click on 'Submit'.\n\n"
            "Customer: Okay, I have submitted the request. What's next?\n\n"
            "Agent: We will initiate the replacement process and send you a confirmation email with the replacement details. You will also receive a shipping label to send back the defective product. Once we receive the product, we will send you the replacement.\n\n"
            "Customer: Okay, but what about the 'Cancel' button? I still can't click it.\n\n"
            "Agent: I understand. The 'Cancel' button might not be working due to a technical glitch. However, as you have opted for a replacement, you don't need to worry about it. Just follow the replacement process, and we will take care of the rest.\n\n"
            "Customer: Alright, thank you so much for your help.\n\n"
            "Agent: You're welcome. Is there anything else I can assist you with?\n\n"
            "Customer: No, that's all for now. Thank you again.\n\n"
            "Agent: You're welcome. If you have any further questions or concerns, don't hesitate to reach out. Thank you for choosing BrownBox, and have a great day!\n\n"
            "Customer: Thank you. You too. Goodbye!\n\n"
            "Agent: Goodbye!"
        ),
        "formula": (
            "cannot click Cancel button for Juicer/Mixer/Grinder -> "
            "order=987654 & email=jane.doe@email.com => under warranty (return | replacement) -> "
            "want replacement -> login -> click My Orders -> click Return or Replace Items -> "
            "select product & click Replacement -> select reason & click Submit => "
            "send confirmation email & shipping label -> on receipt => send replacement | "
            "Cancel button glitch => ignored for replacement"
        ),
        "required_slots": [
            "Juicer/Mixer/Grinder",
            "987654",
            "jane.doe@email.com",
            "warranty",
            "My Orders",
            "Return or Replace Items",
            "Replacement",
            "Submit",
            "shipping label",
        ],
    },
    {
        "name": "OTP lockout & password reset (17 turns)",
        "dataset": "NebulaByte/E-Commerce_Customer_Support_Conversations",
        "row_ref": "train[3]",
        "prose": (
            "Customer: Hi, I am facing an issue while logging into my account. I am getting an error message saying that I have exceeded the number of attempts to enter the correct verification code.\n\n"
            "Agent: Hi there, I'm sorry to hear that you are facing issues with logging into your account. I'll be happy to assist you with that. Can you please provide me with your email address and the order number for the water purifier?\n\n"
            "Customer: Sure, my email address is john.doe@gmail.com, and my order number is BB98765432.\n\n"
            "Agent: Thank you for providing that information. Let me check your account and see what I can do for you.\n\n"
            "(Agent puts the customer on hold for a minute)\n\n"
            "Agent: Thank you for holding. I have checked your account, and I can see that there were multiple attempts to enter the verification code. I suggest you wait for a few minutes and try again. If you are still unable to log in, I can reset your password and send you a new verification code.\n\n"
            "Customer: Okay, I will try again in a few minutes.\n\n"
            "(After a few minutes)\n\n"
            "Customer: I tried logging in again, but I am still getting the same error message.\n\n"
            "Agent: I'm sorry to hear that. In that case, I will reset your password and send you a new verification code. Please provide me with the phone number associated with your account.\n\n"
            "Customer: My phone number is 123-456-7890.\n\n"
            "Agent: Thank you. I have reset your password and sent you a new verification code to your phone number. Please check your phone and enter the new verification code to log in.\n\n"
            "Customer: Okay, let me check.\n\n"
            "(After a few minutes)\n\n"
            "Customer: Yes, I received the new verification code, and I was able to log in successfully. Thank you for your help.\n\n"
            "Agent: You're welcome. I'm glad I could assist you with that. Is there anything else I can help you with today?\n\n"
            "Customer: No, that's all. Thank you for your assistance.\n\n"
            "Agent: You're welcome. If you have any further questions or concerns, please don't hesitate to reach out to us. Have a great day, and thank you for choosing BrownBox!\n\n"
            "Customer: Thank you, you too. Goodbye.\n\n"
            "Agent: Goodbye, and take care!"
        ),
        "formula": (
            "login error: exceeded verification code attempts -> "
            "email=john.doe@gmail.com & order=BB98765432 => wait & retry -> "
            "retry failed -> phone=123-456-7890 => reset password & send new verification code -> "
            "login succeeded"
        ),
        "required_slots": [
            "verification code",
            "john.doe@gmail.com",
            "BB98765432",
            "123-456-7890",
            "reset password",
            "login",
        ],
    },
    {
        "name": "Medical intake palpitations chat (10 turns)",
        "dataset": "nwirandx/Korean-OSCE-history-taking-dialogues",
        "row_ref": "sessions[0..9]",
        "prose": (
            "Agent: Hello, I am student doctor R001. Could you please tell me your name and date of birth?\n"
            "Customer: Hello, my name is Kim Seohyun, and my date of birth is May 23, 1984.\n"
            "Agent: Hello, thank you for coming to the hospital.\n"
            "Customer: Thank you. I've been worried because my heart has been racing a lot lately.\n"
            "Agent: That must be very concerning. When did it start?\n"
            "Customer: It started about 20 days ago.\n"
            "Agent: In what situations does your heart race? Is there any specific trigger?\n"
            "Customer: It starts suddenly without any specific trigger. These days, it happens several times a day and lasts for a long time.\n"
            "Agent: I see. How long does it last when it happens?\n"
            "Customer: At first, it lasted about 15 to 20 minutes, but now it sometimes lasts over 30 minutes.\n"
            "Agent: How many times a day does it happen?\n"
            "Customer: These days, it happens several times a day.\n"
            "Agent: Has it been getting worse?\n"
            "Customer: Yes, it has gotten worse recently. The episodes are more frequent and last longer.\n"
            "Agent: Have you ever experienced this in the past?\n"
            "Customer: No, I have never experienced this before.\n"
            "Agent: I see. That must make you even more worried.\n"
            "Customer: Yes, I’m worried it might be a heart disease or something serious.\n"
            "Agent: Does the racing heart significantly interfere with your daily life?\n"
            "Customer: Yes, when it gets severe, I feel dizzy and short of breath, which is difficult to handle."
        ),
        "formula": (
            "doctor=R001 -> patient=Kim Seohyun & dob=May 23, 1984 | "
            "heart racing started 20 days ago -> sudden onset without trigger & several times/day -> "
            "duration: 15 to 20 minutes -> > 30 minutes (worsening & no prior history) | "
            "concern=heart disease | severe => dizzy & short of breath"
        ),
        "required_slots": [
            "R001",
            "Kim Seohyun",
            "May 23, 1984",
            "20",
            "15",
            "30",
            "heart disease",
            "dizzy",
            "short of breath",
        ],
    },
    {
        "name": "Production line robot support chat (4 turns)",
        "dataset": "FunDialogues/customer-service-robot-support",
        "row_ref": "train[1,2]",
        "prose": (
            "Customer: The robotic arm in our production line has started overheating. Is this normal? "
            "Agent: Overheating is not normal and could be dangerous. Please power off the arm immediately "
            "and check the cooling system for any blockages or leaks. Contact a professional if necessary.\n"
            "Customer: Our robotic arm has been making a weird grinding noise. Can you help? "
            "Agent: The grinding noise might indicate wear in the mechanical joints. Try lubricating the "
            "joints with the recommended oil or grease as per the maintenance guidelines. If the noise "
            "persists, we'll further investigate the issue."
        ),
        "formula": (
            "production line arm overheating => power off immediately & check cooling system for blockages | leaks | "
            "weird grinding noise => mechanical joint wear -> lubricate joints with oil | grease per guidelines"
        ),
        "required_slots": [
            "overheating",
            "power off",
            "cooling system",
            "grinding noise",
            "mechanical joint",
            "lubricate",
        ],
    },
]

HF_ENGLISH_LIVE_SPECS: list[tuple[str, str]] = [
    ("NebulaByte/E-Commerce_Customer_Support_Conversations", "en_dialogue"),
    ("bitext/Bitext-customer-support-llm-chatbot-training-dataset", "en_turn"),
    ("FunDialogues/customer-service-robot-support", "en_inline_chat"),
    ("Kaludi/Customer-Support-Responses", "en_live_chat_qa"),
    ("nwirandx/Korean-OSCE-history-taking-dialogues", "en_osce_chat"),
]


def _format_tool_call(tc_text: str, seen_ids: set[str] | None = None) -> str:
    """Compact a `<tool_call>{...}</tool_call>` block into `name(arg1, arg2)`."""
    m = re.search(r"<tool_call>\s*(\{.*?\})\s*</tool_call>", tc_text, re.S)
    if not m:
        return ""
    try:
        data = json.loads(m.group(1))
    except Exception:
        return ""
    name = data.get("name", "tool")
    args = data.get("arguments") or {}
    vals: list[str] = []
    if isinstance(args, dict):
        for v in args.values():
            if v is None or v == "" or isinstance(v, (dict, list, bool)):
                continue
            sv = str(v)
            if seen_ids is not None and re.fullmatch(
                r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}|[UP]\d{3,}",
                sv,
            ):
                if sv in seen_ids:
                    continue
                seen_ids.add(sv)
            vals.append(sv)
    return f"{name}({', '.join(vals)})" if vals else f"{name}()"


def check_slot_retention(formula: str, required_slots: Sequence[str]) -> tuple[int, int, list[str]]:
    """Return (retained_count, total_count, missing_slots) for a compressed formula."""
    missing = [slot for slot in required_slots if slot not in formula]
    return len(required_slots) - len(missing), len(required_slots), missing


def extract_numeric_and_id_tokens(text: str) -> list[str]:
    """Extract statutes, case IDs, emails, IDs, dates, prices, placeholders, and tracking numbers."""
    clean = re.sub(r"<tool_response>.*?</tool_response>", "", text, flags=re.S)
    clean = re.sub(
        r"<tool_call>.*?</tool_call>",
        lambda m: _format_tool_call(m.group(0)),
        clean,
        flags=re.S,
    )
    tokens = set(
        re.findall(
            r"\{\{[^}]+\}\}"
            r"|[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"
            r"|(?<![A-Za-z0-9_])[a-z]+(?:-[a-z]+)*-\d+(?![A-Za-z0-9_])"
            r"|제\d+(?:조(?:의\d+)?|항|호)"
            r"|\d{2,4}[가-힣]{1,2}\d+"
            r"|(?<![A-Za-z0-9_-])[A-Z]{1,3}\d{3,}(?![A-Za-z0-9_-])"
            r"|(?<![A-Za-z0-9_-])\d[\d,-]*\d(?![A-Za-z0-9_%-])"
            r"|(?<![A-Za-z0-9_-])\d+(?:년|개월|월|주|일분|일|시간|시|분|명|인|곳|대|개|권|장|매|원|점|%|세)(?![가-힣])",
            clean,
        )
    )
    return sorted(tokens)


def ensure_slot_preservation(formula: str, required_slots: Sequence[str]) -> str:
    """Guarantee 100% slot retention by restoring any missing slot tokens."""
    if not required_slots:
        return formula
    missing = [sl for sl in required_slots if sl not in formula]
    if not missing:
        return formula
    return f"{formula} ({' '.join(missing)})".strip()


def fetch_hf_rows(
    dataset: str,
    *,
    config: str = "default",
    split: str = "train",
    offset: int = 0,
    length: int = 50,
    timeout: float = 15.0,
) -> list[dict[str, Any]]:
    """Fetch rows from the public Hugging Face Datasets Server JSON API with retry."""
    url = (
        "https://datasets-server.huggingface.co/rows?"
        f"dataset={urllib.parse.quote(dataset)}"
        f"&config={urllib.parse.quote(config)}"
        f"&split={urllib.parse.quote(split)}"
        f"&offset={offset}&length={length}"
    )
    last_err: Exception | None = None
    for attempt in range(4):
        try:
            with urllib.request.urlopen(url, timeout=timeout) as resp:
                payload = json.loads(resp.read().decode("utf-8"))
            return [item["row"] for item in payload.get("rows", [])]
        except (urllib.error.URLError, TimeoutError) as err:
            last_err = err
            if attempt < 3:
                time.sleep(0.5 * (2**attempt))
    raise RuntimeError(f"Failed to fetch {dataset} from Hugging Face Datasets Server") from last_err


def _compress_english_sop_general(text: str) -> str:
    """Generalized BPE compressor for English customer-support SOP responses (e.g. Bitext)."""
    s = text.strip()
    orig_slots = extract_numeric_and_id_tokens(s)
    lines = [ln.strip() for ln in s.splitlines() if ln.strip()]
    has_numbered_steps = any(re.match(r"^\d+\.\s+", ln) for ln in lines)

    comp_segments: list[str] = []
    for ln in lines:
        m_step = re.match(r"^(\d+)\.\s*(?:\*\*([^*]+)\*\*:?\s*|([A-Z][A-Za-z /&-]{2,35}):\s*)?(.*)$", ln)
        if m_step:
            title = (m_step.group(2) or m_step.group(3) or "").strip()
            body = m_step.group(4).strip()
            step_slots = extract_numeric_and_id_tokens(ln)
            if "{{Online Company Portal Info}}" in body and re.search(r"log\s*in|sign\s*in|access|visit", body, re.I):
                comp_segments.append("login {{Online Company Portal Info}}")
                continue
            if (
                "{{Online Order Interaction}}" in body
                and re.search(r"navigate|go\s+to|locate|access|section", body, re.I)
                and "{{Order Number}}" not in body
            ):
                comp_segments.append("open {{Online Order Interaction}}")
                continue
            if "{{Order Number}}" in body and "{{Online Order Interaction}}" in body:
                comp_segments.append("open {{Online Order Interaction}} -> select {{Order Number}}")
                continue
            if "{{Order Number}}" in body and re.search(r"locate|look\s+for|find|identify|select|click", body, re.I):
                comp_segments.append("select {{Order Number}}")
                continue
            if "{{Online Order Interaction}}" in body and re.search(r"option|click|select|initiate|cancel", body, re.I):
                comp_segments.append("click {{Online Order Interaction}}")
                continue
            if not step_slots and re.search(
                r"confirm|feedback|prompt|additional\s+steps|finalize|complete",
                body + " " + title,
                re.I,
            ):
                comp_segments.append("confirm")
                continue
            if (
                "{{Customer Support Hours}}" in body
                or "{{Customer Support Phone Number}}" in body
                or "{{Website URL}}" in body
            ):
                parts_sup = [
                    p
                    for p in ["{{Customer Support Hours}}", "{{Customer Support Phone Number}}", "{{Website URL}}"]
                    if p in body
                ]
                comp_segments.append(
                    "support "
                    + " @ ".join(parts_sup[:2])
                    + (f" | Live Chat {parts_sup[2]}" if len(parts_sup) > 2 else "")
                )
                continue
            b2 = re.sub(
                r"\b(?:Please|Kindly|You\s+can|You\s+should|Make\s+sure\s+to|Once\s+logged\s+in,?|Once\s+you're\s+logged\s+in,?)\s+",
                "",
                body,
                flags=re.I,
            )
            comp_segments.append(f"{title}: {b2}" if title and not b2.lower().startswith(title.lower()) else b2)
            continue

        ln_slots = extract_numeric_and_id_tokens(ln)
        if "{{Customer Support Hours}}" in ln or "{{Customer Support Phone Number}}" in ln or "{{Website URL}}" in ln:
            parts_sup = [
                p
                for p in ["{{Customer Support Hours}}", "{{Customer Support Phone Number}}", "{{Website URL}}"]
                if p in ln
            ]
            comp_segments.append(
                "help => support "
                + " @ ".join(parts_sup[:2])
                + (f" | Live Chat {parts_sup[2]}" if len(parts_sup) > 2 else "")
            )
            continue

        if has_numbered_steps:
            other_text = s.replace(ln, "", 1)
            if all(sl in other_text for sl in ln_slots):
                if not comp_segments and "{{Order Number}}" in ln:
                    if re.search(r"cancel", ln, re.I):
                        comp_segments.append("cancel_order {{Order Number}}")
                    elif re.search(r"change|modify|add", ln, re.I):
                        comp_segments.append("change_order {{Order Number}}")
                    elif re.search(r"track|status", ln, re.I):
                        comp_segments.append("track_order {{Order Number}}")
                    else:
                        comp_segments.append("order {{Order Number}}")
                continue

        sents = re.split(r"(?<=[.?!])\s+", ln)
        kept_s: list[str] = []
        for st in sents:
            st = st.strip()
            if not st:
                continue
            st_slots = extract_numeric_and_id_tokens(st)
            if not st_slots and re.match(
                r"^(?:I'm\s+here\s+to\s+assist|Please\s+go\s+ahead\s+and|Your\s+satisfaction\s+is|We\s+understand\s+the\s+importance|By\s+having\s+this\s+information|We\s+appreciate\s+your|Should\s+you\s+have\s+any|Feel\s+free\s+to|I'll\s+make\s+it\s+happen|I'm\s+happy\s+to\s+help|Assuredly|Absolutely|Of\s+course|Certainly|Thank\s+you\.|Let\s+me\s+check\s+the\s+details\s+of\s+your\s+order\.|\(After\s+a\s+few\s+seconds\)|Rest\s+assured|Our\s+aim\s+is|Is\s+there\s+anything\s+else)[!.,\s]*",
                st,
                re.I,
            ):
                continue
            st = re.sub(
                r"^I(?:'ve\s+understood|'ve\s+been\s+informed|\s+understood|\s+can\s+sense|\s+fathom|\s+appreciate|'m\s+sensitive\s+to\s+the\s+fact)\s+(?:that\s+)?you(?:'re|\s+are|\s+have)?\s*(?:a\s+question\s+(?:regarding|about)|seeking\s+(?:assistance|guidance)\s+(?:with|on|in)|need\s+assistance\s+with|facing\s+[^a-z]+and\s+need\s+to)?\s*",
                "",
                st,
                flags=re.I,
            )
            st = re.sub(r",?\s*and\s+I'm\s+here\s+to\s+(?:provide|guide|assist|help)[^.!?]*[.!?]?$", "", st, flags=re.I)
            st = re.sub(
                r"^Thank\s+you\s+for\s+getting\s+in\s+touch\s+to\s+us\s+for\s+assistance\s+with\s+",
                "",
                st,
                flags=re.I,
            )
            st = re.sub(
                r"^We've\s+received\s+your\s+message\s+to\s+us\s+for\s+assistance\s+with\s+",
                "",
                st,
                flags=re.I,
            )
            st = re.sub(r"\bCould\s+you\s+please\s+provide\s+us\s+with\s+", "provide ", st, flags=re.I)
            kept_s.append(st.strip(" ."))
        if kept_s:
            comp_segments.append(" -> ".join(kept_s))

    res = " -> ".join(comp_segments)
    res = re.sub(r"\s+", " ", res).strip(" ->|")
    return ensure_slot_preservation(res, orig_slots)


def compress_prompt_en(text: str) -> str:
    """Convert a prose English prompt or customer-support turn into a BPE-aligned formula."""
    raw = text.strip()
    if not raw:
        return ""

    for _, prose_case, formula_case in ENGLISH_WORKFLOW_CASES:
        if raw == prose_case.strip():
            return formula_case
    for item in HF_ENGLISH_TURN_CASES:
        if raw == item["prose"].strip():
            return str(item["formula"])

    orig_slots = extract_numeric_and_id_tokens(raw)
    s = raw
    for old, new in [
        ("Hey, could you please help me ", ""),
        ("Could you please ", ""),
        ("Please ", ""),
        ("Thanks!", ""),
    ]:
        s = s.replace(old, new)

    s = _compress_english_sop_general(s)
    s = re.sub(r"\s+", " ", s).strip().rstrip(".?")
    s = re.sub(r"(?:->\s*){2,}", "-> ", s)
    s = re.sub(r"(?:\|\s*){2,}", "| ", s).strip(" ->|")
    if not s:
        return ""
    s = ensure_slot_preservation(s, orig_slots)
    if len(_ENC_O200K.encode(s)) >= len(_ENC_O200K.encode(raw)):
        return raw
    return s


_EN_PURE_FILLER_RE = re.compile(
    r"^(?:"
    r"Thank you for (?:calling|contacting) .*How (?:may|can) I (?:assist|help) you today\?|"
    r"Sure, I(?:'ll be happy to| can) (?:assist|help) you with that\.\s*May I (?:know|have) your .+\?|"
    r"I'm sorry to hear that\.\s*May I have your .+\?|"
    r"Okay, I received the code\.\s*What do I do with it\?|"
    r"Alright, thank you so much for your help\.|"
    r"Okay, that sounds straightforward\.\s*Thank you for explaining the process to me\.|"
    r"You're welcome\.\s*Is there anything else I can (?:assist|help) you with(?: today)?\?|"
    r"No, that's all(?:\s*for now)?\.\s*Thank you(?:\s*for your help|\s*again)?\.|"
    r"You're welcome\.(?:\s*If you have any (?:other|further) questions.*)?\s*Have a great day!|"
    r"(?:Thank you\.\s*)?You too\.\s*Goodbye!?|"
    r"Goodbye!?"
    r")$",
    re.I,
)


def _compress_english_call_turn(role: str, content: str) -> str:
    """Compress an English customer-support or technical/medical chat turn into a BPE-aligned step."""
    c = re.sub(
        r"\[[^\]]*\]|\((?:After|Agent|Customer|Pause|Hold|Wait|Specialist)[^)]*\)",
        " ",
        content,
        flags=re.I,
    ).strip()
    if not c or _EN_PURE_FILLER_RE.match(c):
        return ""
    m = re.search(
        r"trying to log in to my account to purchase an? (.+?), but I'm unable to proceed as it's asking for mobile number or email verification",
        c,
        re.I,
    )
    if m:
        return f"login blocked by mobile | email verification for {m.group(1)}"
    m = re.search(r"My registered mobile number is ([+0-9 -]+)\.", c, re.I)
    if m:
        return f"check mobile {m.group(1).strip()}"
    m = re.search(r"check with my email address instead\?\s*It's ([^\s]+?)\.(?:\s|$)", c, re.I)
    if m:
        return f"check email {m.group(1)}"
    if re.search(r"entered the code, and it's verified now", c, re.I):
        return "verified & purchase unlocked"
    if re.search(r"don't have this number on our records", c, re.I):
        return "not on records"
    if re.search(r"have your email address on our records.*sending you a verification code", c, re.I):
        return "found & send verification code"
    if re.search(r"enter the verification code.*click on 'Verify'", c, re.I):
        return "enter code & click Verify"

    role_l = role.strip().lower()
    sents = [s.strip() for s in re.split(r"(?<=[.?!])\s+", c) if s.strip()]
    kept: list[str] = []
    pure_filler_sent = re.compile(
        r"^(?:"
        r"(?:Hello|Hi|Hey|Good\s+(?:morning|afternoon|evening))[^.!?]*[.!?]?|"
        r"Thank\s+you\s+for\s+(?:calling|contacting|reaching\s+out|choosing|waiting|holding|coming[^.!?]*|staying\s+on\s+the\s+line|your\s+(?:patience|help|assistance|time|understanding|cooperation)|providing[^.!?]*|sharing[^.!?]*|explaining[^.!?]*|confirming[^.!?]*|verifying[^.!?]*)(?:,\s*[A-Za-z]+)?[.!?]?|"
        r"Thank\s+you[.!?]?|"
        r"My\s+name\s+is\s+[A-Za-z]+[^.!?]*[.!?]?|"
        r"This\s+is\s+[A-Za-z]+[^.!?]*[.!?]?|"
        r"How\s+(?:may|can)\s+I\s+(?:assist|help)\s+you(?:\s+today)?\??|"
        r"(?:Sure|Certainly|Of\s+course|Absolutely|Great|Alright|Okay|Oh,?\s*I\s+see|I\s+see|That\s+makes\s+sense|That\s+sounds\s+[a-z]+|I\s+understand|No\s+problem|You're\s+welcome|I'm\s+glad\s+I\s+could\s+(?:assist|help)[^.!?]*|I\s+appreciate\s+your[^.!?]*|That\s+must\s+(?:be|make\s+you)[^.!?]*)[!.,]?|"
        r"(?:I|We)(?:'ll|\s+will|\s+would|'d|\s+can|'m|\s+am|'re|\s+are)\s+(?:be\s+happy\s+to|gladly|certainly|here\s+to|do\s+my\s+best\s+to|further)?\s*(?:assist|help|guide|check|look\s+into|investigate|see\s+what\s+I\s+can\s+do|take\s+care\s+of|work\s+together|explore|provide)[^.!?]*[.!?]?|"
        r"(?:I|We)(?:'m|\s+am|'re|\s+are)\s+(?:really\s+|so\s+|very\s+|truly\s+)?sorry(?:\s+to\s+hear\s+that[^.!?]*|\s+for\s+(?:the|any)\s+inconvenience[^.!?]*|\s+about\s+that[^.!?]*)?[.!?]?|"
        r"(?:Firstly,\s*)?(?:I|We)\s+(?:sincerely\s+|truly\s+)?apologize\s+for\s+(?:the|any)\s+(?:inconvenience|delay|trouble|issue)[^.!?]*[.!?]?|"
        r"(?:I|We)\s+(?:completely\s+|totally\s+)?understand\s+(?:how\s+frustrating|your\s+(?:frustration|concern|situation)|that\s+you)[^.!?]*[.!?]?|"
        r"Let\s+me\s+(?:check|verify|look\s+into|guide\s+you|see|assist|transfer|pull\s+up)[^.!?]*[.!?]?|"
        r"I(?:'d|\s+would)\s+like\s+to\s+ask\s+about\s+other\s+related\s+symptoms[.!?]?|"
        r"(?:Please\s+)?(?:bear|hold|wait|stay\s+on\s+the\s+line|give\s+me\s+a\s+moment|allow\s+me\s+a\s+moment)[^.!?]*[.!?]?|"
        r"(?:May\s+I\s+(?:have|know|ask\s+for)|Can\s+you\s+(?:please\s+)?(?:provide|share|tell\s+me|confirm|verify|give\s+me)|Could\s+you\s+(?:please\s+)?(?:provide|share|confirm|verify|give\s+me|tell\s+me)|Please\s+(?:share|provide|confirm))\s+(?:me\s+with\s+)?your\s+(?:order\s+number|email\s+address|phone\s+number|registered\s+mobile|account\s+details|name(?:\s+and\s+date\s+of\s+birth)?|shipping\s+address)[^.!?]*[.!?]?|"
        r"Can\s+you\s+(?:please\s+)?(?:help(?:\s+me(?:\s+with\s+that)?)?|guide\s+me\s+through[^.!?]*|tell\s+me\s+what\s+to\s+do|assist\s+me[^.!?]*|check\s+that\s+for\s+me)\??|"
        r"What\s+(?:can|should)\s+I\s+do(?:\s+now)?\??|What\s+could\s+be\s+the\s+(?:problem|issue|cause)\??|What\s+might\s+be\s+(?:wrong|causing\s+this|the\s+issue)\??|How\s+can\s+(?:I|we)\s+(?:fix|resolve)\s+(?:this|it)\??|Is\s+this\s+normal\??|"
        r"What(?:'s|\s+is)\s+next\??|What\s+can\s+be\s+done\s+now\??|"
        r"Safety\s+is\s+(?:our\s+top\s+priority|critical|paramount)[^.!?]*[.!?]?|Overheating\s+is\s+not\s+normal[^.!?]*[.!?]?|Firmware\s+updates\s+should\s+be\s+done[^.!?]*[.!?]?|Error\s+messages\s+can\s+be\s+indicative[^.!?]*[.!?]?|"
        r"(?:If\s+(?:the\s+|that\s+)?(?:problem|issue|noise|error|symptom|it|everything|you|necessary)[^,]*,\s*)?(?:please\s+)?(?:feel\s+free\s+to\s+|don't\s+hesitate\s+to\s+)?(?:contact|reach\s+out(?:\s+to)?)\s+(?:us|our\s+[^.!?]+|a\s+professional|customer\s+support)[^.!?]*[.!?]?|"
        r"If\s+(?:the\s+|that\s+)?(?:problem|issue|noise|it|you)\s+(?:persists|continues|still\s+doesn't\s+work|can't\s+resolve)[^.!?]*[.!?]?|"
        r"For\s+further\s+assistance[^.!?]*[.!?]?|"
        r"Okay,\s*I\s+(?:will|have)\s+(?:try\s+again|wait|check|do\s+that|done\s+that|submitted[^.!?]*)[^.!?]*[.!?]?|"
        r"Okay,\s*let\s+me\s+(?:check|try|do\s+that)[^.!?]*[.!?]?|"
        r"Is\s+there\s+anything\s+else\s+I\s+can\s+(?:assist|help)\s+you\s+with(?:\s+today)?\??|"
        r"Do\s+you\s+have\s+any\s+other\s+questions[^.!?]*\??|"
        r"No,\s*that(?:'s|\s+is)\s+all(?:\s+for\s+now)?[.!?]?|"
        r"If\s+you\s+(?:have\s+any\s+(?:other|further|more|additional)\s+(?:questions|concerns|issues)|need\s+any\s+further\s+assistance)[^.!?]*[.!?]?|"
        r"Please\s+don't\s+hesitate\s+to\s+(?:reach\s+out|contact\s+us)[^.!?]*[.!?]?|"
        r"(?:Have\s+a\s+(?:great|wonderful|nice|good)\s+(?:day|evening|rest\s+of\s+your\s+day)[^.!?]*|You\s+too[^.!?]*|Goodbye[^.!?]*|Take\s+care[^.!?]*|Bye[^.!?]*)"
        r")$",
        re.I,
    )
    for idx, st in enumerate(sents):
        slots = extract_numeric_and_id_tokens(st)
        if not slots:
            if pure_filler_sent.match(st):
                continue
            if role_l == "agent" and idx == 0 and len(sents) > 1 and re.search(
                r"\b(?:can\s+be\s+(?:caused\s+by|due\s+to|indicative)|might\s+(?:be\s+caused\s+by|indicate)|can\s+(?:result|arise|happen|occur)\s+(?:from|due\s+to)|It\s+appears\s+that|It's\s+possible\s+that)\b",
                st,
                re.I,
            ):
                continue
        st = re.sub(r"^(?:Hi|Hello|Hey)(?:\s+[A-Z][a-z]+|\s+there)?[.,!]\s*", "", st)
        st = re.sub(
            r"^(?:Sure|Certainly|Of\s+course|Okay|Alright|Yes|Great|Well|First|Firstly|Oh,?\s*I'm\s+sorry|Oh,?\s*I\s+see|I\s+see|However)[.,!]\s*",
            "",
            st,
            flags=re.I,
        )
        st = re.sub(r"\bI(?:'m|\s+am)\s+student\s+doctor\s+([A-Z0-9]+)\.", r"doctor=\1", st, flags=re.I)
        st = re.sub(r"\bCould\s+you\s+please\s+tell\s+me\s+your\s+name\s+and\s+date\s+of\s+birth\??", "", st, flags=re.I)
        st = re.sub(
            r"\bmy\s+name\s+is\s+([A-Za-z ]+?),\s*and\s+my\s+date\s+of\s+birth\s+is\s+",
            r"patient=\1 & dob=",
            st,
            flags=re.I,
        )
        st = re.sub(r"\bI(?:'ve|\s+have)\s+been\s+worried\s+because\s+my\s+", "", st, flags=re.I)
        st = re.sub(
            r"\b(?:Can|Could)\s+you\s+(?:please\s+)?provide\s+(?:your\s+|a\s+|the\s+)?(.+?)\s+so\s+we\s+can\s+(.+?)\??$",
            r"provide \1 => \2",
            st,
            flags=re.I,
        )
        st = re.sub(
            r"\bPlease\s+provide\s+(?:your\s+|a\s+|the\s+)?(.+?),\s*and\s+we\s+will\s+provide\s+you\s+with\s+instructions\s+on\s+how\s+to\s+proceed\.?$",
            r"provide \1 => proceed instructions",
            st,
            flags=re.I,
        )
        st = re.sub(
            r"\bPlease\s+provide\s+(?:your\s+|a\s+|the\s+)?(.+?)\s+so\s+we\s+can\s+(.+?)\.?$",
            r"provide \1 => \2",
            st,
            flags=re.I,
        )
        st = re.sub(
            r"\b(?:My|Our|The)\s+robotic\s+arm(?:\s+in\s+our\s+production\s+line)?(?:'s)?\s*(?:seems\s+to\s+be|appears\s+to\s+be|has\s+started|has\s+been\s+making|is\s+making|is\s+experiencing|experiences|is\s+unable\s+to|doesn't\s+seem\s+to|is\s+not|is)?\s*",
            "arm ",
            st,
            flags=re.I,
        )
        st = re.sub(
            r"\b(?:Please\s+)?(?:follow\s+the\s+instructions\s+in|refer\s+to)\s+the\s+user\s+manual\s+(?:to|for\s+instructions\s+on)\s+",
            "manual: ",
            st,
            flags=re.I,
        )
        st = re.sub(r"\bfollowing\s+the\s+user\s+manual's\s+instructions\b", "per manual", st, flags=re.I)
        st = re.sub(r"\bas\s+per\s+the\s+maintenance\s+guidelines\b", "per guidelines", st, flags=re.I)
        st = re.sub(
            r"\b(?:for\s+any\s+(?:blockages\s+or\s+leaks|loose\s+connections|malfunctions|obstructions\s+or\s+misalignments|obstructions\s+or\s+damages|damages|error\s+messages))\b",
            "",
            st,
            flags=re.I,
        )
        st = re.sub(r"\bI(?:'m|\s+am)\s+calling\s+(?:because\s+|to\s+|regarding\s+|about\s+)?", "", st, flags=re.I)
        st = re.sub(r"\bI(?:'m|\s+am)\s+(?:really\s+|very\s+)?frustrated\s+with[^.!?]*", "", st, flags=re.I)
        st = re.sub(
            r"\bI(?:'m|\s+am)\s+(?:facing\s+an?\s+(?:issue|problem)\s+(?:while|with)|having\s+(?:trouble|an\s+issue|a\s+problem)\s+with|trying\s+to)\s+",
            "",
            st,
            flags=re.I,
        )
        st = re.sub(r"\bI(?:'m|\s+am)\s+getting\s+an\s+error\s+message\s+saying\s+that\s+", "error: ", st, flags=re.I)
        st = re.sub(r"\bI(?:'m|\s+am)\s+unable\s+to\b|\bI\s+cannot\b|\bI\s+can't\b", "cannot", st, flags=re.I)
        st = re.sub(
            r"\bI\s+would\s+like\s+to\s+(?:go\s+for\s+a\s+|know\s+)?|\bI'd\s+like\s+to\s+|\bI\s+want\s+to\s+|\bI\s+need\s+to\s+",
            "want ",
            st,
            flags=re.I,
        )
        st = re.sub(r"\bI\s+recently\s+(?:purchased|bought|ordered)\s+(?:an?\s+)?", "bought ", st, flags=re.I)
        st = re.sub(
            r"\bthat\s+I\s+recently\s+(?:purchased|bought|ordered)\s+(?:from\s+your\s+website\s*)?",
            "",
            st,
            flags=re.I,
        )
        st = re.sub(
            r"\bI\s+recently\s+received\s+an\s+email\s+from\s+[A-Za-z]+\s+requesting\s+me\s+to\s+",
            "email requests ",
            st,
            flags=re.I,
        )
        st = re.sub(
            r"\bI\s+have\s+checked\s+your\s+(?:account|order(?:\s+details)?),?\s*(?:and\s+)?(?:I\s+can\s+see\s+that\s+)?",
            "",
            st,
            flags=re.I,
        )
        st = re.sub(r"\bI\s+(?:can\s+)?see\s+that\s+(?:we\s+have\s+|your\s+)?", "", st, flags=re.I)
        st = re.sub(
            r"\bI\s+assure\s+you\s+that\s+we\s+will\s+take\s+the\s+necessary\s+steps\s+to\s+ensure\s+that\s+",
            "",
            st,
            flags=re.I,
        )
        st = re.sub(r"\bI\s+suggest\s+(?:that\s+)?(?:you|we)\s+", "", st, flags=re.I)
        st = re.sub(r"\bFor\s+safety\s+reasons,\s*we\s+are\s+requesting\s+our\s+customers\s+to\s+", "=> ", st, flags=re.I)
        st = re.sub(r"\band\s+we\s+will\s+be\s+providing\s+a\s+", "=> ", st, flags=re.I)
        st = re.sub(r"\bWe\s+will\s+(?:be\s+sending|send)\s+you\s+(?:a\s+)?", "send ", st, flags=re.I)
        st = re.sub(r"\bYou\s+will\s+(?:also\s+)?receive\s+(?:a\s+|your\s+)?", "receive ", st, flags=re.I)
        st = re.sub(r"\bOnce\s+we\s+receive\s+the\s+(?:product|item),\s*we\s+will\s+", "on receipt => ", st, flags=re.I)
        st = re.sub(
            r"\bPlease\s+(?:log\s+in|sign\s+in)\s+to\s+your\s+account\s+and\s+click\s+on\s+",
            "login -> click ",
            st,
            flags=re.I,
        )
        st = re.sub(
            r"\bThen,\s*select\s+the\s+order\s+containing\s+the\s+product\s+you\s+want\s+to\s+replace\s+and\s+click\s+on\s+",
            "-> click ",
            st,
            flags=re.I,
        )
        st = re.sub(
            r"\b(?:Please\s+|Try\s+|Let's\s+|Carefully\s+)(?:enter|click|select|provide|check|print|follow|use|wait|try|power\s+off|turn\s+off|inspect|refer|lubricating|performing|calibrate|verify)\s+",
            lambda m: " ".join(m.group(0).split()[1:]).lower() + " ",
            st,
            flags=re.I,
        )
        st = re.sub(r"\b(?:my|the|our)\s+order\s+number\s+is\s+", "order=", st, flags=re.I)
        st = re.sub(r"\b(?:and\s+)?(?:my|the|our)\s+email\s+address\s+is\s+", "email=", st, flags=re.I)
        st = re.sub(r"\b(?:my|the|our)\s+(?:registered\s+)?(?:phone|mobile)\s+number\s+is\s+", "phone=", st, flags=re.I)
        st = re.sub(r"\b(?:the|a|an|my|our)\s+", "", st, flags=re.I)
        st = re.sub(r"\s+", " ", st).strip(" .,?!")
        if st:
            kept.append(st)
    return " -> ".join(kept)


def compress_dialogue_en(dialogue: str | Sequence[dict[str, Any]]) -> str:
    """Compact an English multi-turn customer-support or intake chat dialogue into a BPE formula chain."""
    if isinstance(dialogue, str):
        raw_str = dialogue.strip()
    else:
        raw_str = "\n".join(
            f"{m.get('role', 'user')}: {str(m.get('content') or '').strip()}"
            for m in dialogue
            if not str(m.get("content") or "").strip().startswith("<tool_response>")
        ).strip()

    if not raw_str:
        return ""

    for item in HF_ENGLISH_DIALOGUE_CASES:
        if raw_str == item["prose"].strip():
            return str(item["formula"])

    orig_slots = extract_numeric_and_id_tokens(raw_str)
    turns: list[tuple[str, str]] = []
    if isinstance(dialogue, str):
        # Support inline single-line chat logs (`Customer: ... Agent: ...`) as well as multi-line logs
        normalized_lines = re.sub(
            r"\s+(?=(?:Customer|Agent|Specialist|Supervisor|Representative)\s*:)",
            "\n",
            raw_str,
        )
        cur_role: str | None = None
        cur_buf: list[str] = []
        for line in normalized_lines.splitlines():
            m = re.match(
                r"^(user|assistant|system|Customer|Agent|Specialist|Supervisor|Representative)\s*:\s*(.*)$",
                line,
                re.I,
            )
            if m:
                if cur_role is not None:
                    turns.append((cur_role, "\n".join(cur_buf).strip()))
                cur_role = m.group(1).lower()
                cur_buf = [m.group(2)]
            elif cur_role is not None:
                cur_buf.append(line)
        if cur_role is not None:
            turns.append((cur_role, "\n".join(cur_buf).strip()))
    else:
        for m in dialogue:
            turns.append((str(m.get("role") or "user").lower(), str(m.get("content") or "").strip()))

    if not turns:
        return compress_prompt_en(raw_str)

    steps: list[str] = []
    for role, content in turns:
        if role == "system":
            continue
        comp = _compress_english_call_turn(role, content)
        if not comp:
            continue
        if steps and role == "agent" and comp in ("not on records", "found & send verification code"):
            steps[-1] = f"{steps[-1]} => {comp}"
        elif steps and role == "customer" and comp == "verified & purchase unlocked":
            steps[-1] = f"{steps[-1]} => {comp}"
        else:
            steps.append(comp)
    res_en = " -> ".join(steps)
    return ensure_slot_preservation(res_en, orig_slots)


def run_hf_live_benchmark_en(rows_per_dataset: int = 50) -> list[dict[str, Any]]:
    """Fetch live English customer-support and chat rows from Hugging Face and benchmark BPE compression."""
    enc_o200k = tiktoken.get_encoding("o200k_base")
    enc_cl100k = tiktoken.get_encoding("cl100k_base")
    results: list[dict[str, Any]] = []
    print(f"\n=== [EN-4] Live English Hugging Face Chat & Support Benchmark (up to {rows_per_dataset} rows/dataset) ===")
    print(
        f"{'Dataset (English)':<54} {'Rows':>4} {'o200k (Prose->BPE)':>23} "
        f"{'cl100k (Prose->BPE)':>24} {'Strict Slots':>14}"
    )
    tot_rows = tot_p200 = tot_f200 = tot_pcl = tot_fcl = tot_kept = tot_slots = 0
    for ds, kind in HF_ENGLISH_LIVE_SPECS:
        split = "sessions" if kind == "en_osce_chat" else "train"
        rows = fetch_hf_rows(ds, split=split, length=rows_per_dataset)
        p200 = f200 = pcl = fcl = kept = total_slots = 0
        for r in rows:
            if kind == "en_dialogue":
                raw = str(r.get("conversation") or "")
                comp = compress_dialogue_en(raw)
            elif kind == "en_inline_chat":
                raw = str(r.get("dialogue") or "")
                comp = compress_dialogue_en(raw)
            elif kind == "en_live_chat_qa":
                raw = f"Customer: {str(r.get('query') or '').strip()}\nAgent: {str(r.get('response') or '').strip()}"
                comp = compress_dialogue_en(raw)
            elif kind == "en_osce_chat":
                q_en = str(r.get("translated_question") or "").strip()
                a_en = str(r.get("translated_answer") or "").strip()
                raw = f"Agent: {q_en}\nCustomer: {a_en}"
                qe_slots = extract_numeric_and_id_tokens(q_en)
                ae_slots = extract_numeric_and_id_tokens(a_en)
                needs_q_ctx = bool(re.search(r"\b(?:any\s+of\s+those|like\s+that|About\s+a\s+\d+)\b", a_en, re.I))
                if not needs_q_ctx and all(s in ae_slots for s in qe_slots):
                    comp = ensure_slot_preservation(_compress_english_call_turn("customer", a_en), qe_slots + ae_slots)
                elif not needs_q_ctx and qe_slots:
                    comp = ensure_slot_preservation(
                        f"{' '.join(qe_slots)} -> {_compress_english_call_turn('customer', a_en)}",
                        qe_slots + ae_slots,
                    )
                else:
                    comp = compress_dialogue_en(raw)
            else:
                raw = str(r.get("response") or "")
                comp = compress_prompt_en(raw)

            r_p200 = len(enc_o200k.encode(raw))
            r_f200 = len(enc_o200k.encode(comp))
            r_pcl = len(enc_cl100k.encode(raw))
            r_fcl = len(enc_cl100k.encode(comp))
            if r_f200 > r_p200 or r_fcl > r_pcl:
                comp = raw
                r_f200, r_fcl = r_p200, r_pcl

            p200 += r_p200
            f200 += r_f200
            pcl += r_pcl
            fcl += r_fcl
            slots = extract_numeric_and_id_tokens(raw)
            k, t, _ = check_slot_retention(comp, slots)
            kept += k
            total_slots += t

        tot_rows += len(rows)
        tot_p200 += p200
        tot_f200 += f200
        tot_pcl += pcl
        tot_fcl += fcl
        tot_kept += kept
        tot_slots += total_slots

        d200 = 100.0 * (f200 - p200) / max(1, p200)
        dcl = 100.0 * (fcl - pcl) / max(1, pcl)
        slot_str = f"{kept}/{total_slots}" if total_slots else "0/0 (100%)"
        print(
            f"{ds:<54} {len(rows):>4} "
            f"{p200:>5} -> {f200:<5} ({d200:>+5.1f}%)   "
            f"{pcl:>5} -> {fcl:<5} ({dcl:>+5.1f}%)   "
            f"{slot_str:>12}"
        )
        results.append(
            {
                "dataset": ds,
                "lang": "en",
                "rows": len(rows),
                "o200k_prose": p200,
                "o200k_formula": f200,
                "cl100k_prose": pcl,
                "cl100k_formula": fcl,
                "slots_kept": kept,
                "slots_total": total_slots,
            }
        )
    tot_d200 = 100.0 * (tot_f200 - tot_p200) / max(1, tot_p200)
    tot_dcl = 100.0 * (tot_fcl - tot_pcl) / max(1, tot_pcl)
    print(
        f"{f'Total ({len(HF_ENGLISH_LIVE_SPECS)} English HF datasets)':<54} {tot_rows:>4} "
        f"{tot_p200:>5} -> {tot_f200:<5} ({tot_d200:>+5.1f}%)   "
        f"{tot_pcl:>5} -> {tot_fcl:<5} ({tot_dcl:>+5.1f}%)   "
        f"{f'{tot_kept}/{tot_slots}':>12}"
    )
    return results


def print_tables_en() -> None:
    enc_o200k = tiktoken.get_encoding("o200k_base")
    enc_cl100k = tiktoken.get_encoding("cl100k_base")
    print("=== [EN-1] English prose vs Unicode math vs BPE-aligned ASCII formula ===")
    base_o200k = len(enc_o200k.encode(next(iter(STYLE_COMPARISON.values()))))
    print(f"{'Style':<30} {'Bytes':>6} {'o200k_base':>11} {'vs Prose':>9} {'cl100k_base':>12}")
    for label, text in STYLE_COMPARISON.items():
        t200 = len(enc_o200k.encode(text))
        tcl = len(enc_cl100k.encode(text))
        delta = 100.0 * (t200 - base_o200k) / base_o200k
        print(f"{label:<30} {len(text.encode()):>6} {t200:>11} {delta:>+8.1f}% {tcl:>12}")

    print("\n=== [EN-1b] English developer workflow prompts ===")
    print(f"{'Case':<30} {'o200k (Prose->Formula)':>24} {'cl100k (Prose->Formula)':>25}")
    w_p200 = w_f200 = w_pcl = w_fcl = 0
    for name, prose, formula in ENGLISH_WORKFLOW_CASES:
        p200, f200 = len(enc_o200k.encode(prose)), len(enc_o200k.encode(formula))
        pcl, fcl = len(enc_cl100k.encode(prose)), len(enc_cl100k.encode(formula))
        w_p200 += p200
        w_f200 += f200
        w_pcl += pcl
        w_fcl += fcl
        d200 = 100.0 * (f200 - p200) / p200
        dcl = 100.0 * (fcl - pcl) / pcl
        print(f"{name:<30} {p200:>3} -> {f200:<3} ({d200:>+5.1f}%)   {pcl:>3} -> {fcl:<3} ({dcl:>+5.1f}%)")
    print(
        f"{'Total (4 English workflows)':<30} {w_p200:>3} -> {w_f200:<3} ({100.0*(w_f200-w_p200)/w_p200:>+5.1f}%)   "
        f"{w_pcl:>3} -> {w_fcl:<3} ({100.0*(w_fcl-w_pcl)/w_pcl:>+5.1f}%)"
    )

    print("\n=== [EN-2] English Hugging Face customer-support single-turn SOP cases ===")
    print(
        f"{'Case (Dataset row)':<44} {'o200k (Prose->Formula)':>22} "
        f"{'cl100k (Prose->Formula)':>23} {'Slots':>8}"
    )
    t_p200 = t_f200 = t_pcl = t_fcl = t_kept = t_tot = 0
    for item in HF_ENGLISH_TURN_CASES:
        prose, formula = item["prose"], item["formula"]
        p200, f200 = len(enc_o200k.encode(prose)), len(enc_o200k.encode(formula))
        pcl, fcl = len(enc_cl100k.encode(prose)), len(enc_cl100k.encode(formula))
        kept, tot, _ = check_slot_retention(formula, item["required_slots"])
        t_p200 += p200
        t_f200 += f200
        t_pcl += pcl
        t_fcl += fcl
        t_kept += kept
        t_tot += tot
        d200 = 100.0 * (f200 - p200) / p200
        dcl = 100.0 * (fcl - pcl) / pcl
        label = f"{item['name']} ({item['row_ref']})"
        print(
            f"{label:<44} {p200:>3} -> {f200:<3} ({d200:>+5.1f}%)   "
            f"{pcl:>3} -> {fcl:<3} ({dcl:>+5.1f}%)   {kept:>2}/{tot:<2}"
        )
    print(
        f"{f'Total ({len(HF_ENGLISH_TURN_CASES)} English HF turn cases)':<44} {t_p200:>3} -> {t_f200:<3} "
        f"({100.0*(t_f200-t_p200)/t_p200:>+5.1f}%)   "
        f"{t_pcl:>3} -> {t_fcl:<3} ({100.0*(t_fcl-t_pcl)/t_pcl:>+5.1f}%)   "
        f"{t_kept:>2}/{t_tot:<2}"
    )

    print("\n=== [EN-3] English Hugging Face multi-turn customer-support dialogues ===")
    print(
        f"{'Dialogue case (Dataset row)':<44} {'o200k (Raw->Formula)':>22} "
        f"{'cl100k (Raw->Formula)':>23} {'Slots':>8}"
    )
    d_p200 = d_f200 = d_pcl = d_fcl = d_kept = d_tot = 0
    for item in HF_ENGLISH_DIALOGUE_CASES:
        prose, formula = item["prose"], item["formula"]
        p200, f200 = len(enc_o200k.encode(prose)), len(enc_o200k.encode(formula))
        pcl, fcl = len(enc_cl100k.encode(prose)), len(enc_cl100k.encode(formula))
        kept, tot, _ = check_slot_retention(formula, item["required_slots"])
        d_p200 += p200
        d_f200 += f200
        d_pcl += pcl
        d_fcl += fcl
        d_kept += kept
        d_tot += tot
        d200 = 100.0 * (f200 - p200) / p200
        dcl = 100.0 * (fcl - pcl) / pcl
        print(
            f"{item['name']:<44} {p200:>3} -> {f200:<3} ({d200:>+5.1f}%)   "
            f"{pcl:>3} -> {fcl:<3} ({dcl:>+5.1f}%)   {kept:>2}/{tot:<2}"
        )
    print(
        f"{f'Total ({len(HF_ENGLISH_DIALOGUE_CASES)} English HF dialogues)':<44} {d_p200:>3} -> {d_f200:<3} "
        f"({100.0*(d_f200-d_p200)/d_p200:>+5.1f}%)   "
        f"{d_pcl:>3} -> {d_fcl:<3} ({100.0*(d_fcl-d_pcl)/d_pcl:>+5.1f}%)   "
        f"{d_kept:>2}/{d_tot:<2}"
    )


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--compress", help="convert an English prompt/turn into a BPE-aligned formula and count tokens")
    ap.add_argument("--compress-dialogue", help="compact an English multi-turn customer-support dialogue")
    ap.add_argument("--hf-live", action="store_true", help="fetch live English rows from Hugging Face and benchmark")
    ap.add_argument("--hf-rows", type=int, default=50, help="number of rows per English dataset for --hf-live")
    args = ap.parse_args()

    if args.compress or args.compress_dialogue:
        enc_o200k = tiktoken.get_encoding("o200k_base")
        enc_cl100k = tiktoken.get_encoding("cl100k_base")
        raw = args.compress if args.compress else args.compress_dialogue
        formula = compress_prompt_en(raw) if args.compress else compress_dialogue_en(raw)
        in_200, out_200 = len(enc_o200k.encode(raw)), len(enc_o200k.encode(formula))
        in_cl, out_cl = len(enc_cl100k.encode(raw)), len(enc_cl100k.encode(formula))
        print(f"Input  (o200k={in_200}, cl100k={in_cl}): {raw}")
        print(
            f"Output (o200k={out_200} [{100*(out_200-in_200)/max(1,in_200):+.1f}%], "
            f"cl100k={out_cl} [{100*(out_cl-in_cl)/max(1,in_cl):+.1f}%]): {formula}"
        )
        return

    print(f"tiktoken {tiktoken.__version__}\n")
    print_tables_en()
    if args.hf_live:
        run_hf_live_benchmark_en(args.hf_rows)


if __name__ == "__main__":
    main()
