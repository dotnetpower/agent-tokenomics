---
created: 2026-11-19
updated: 2026-11-19
dateSource: recorded
title: "Coding Agent Token, Cache & Quota Anatomy (Copilot · Codex · Claude)"
description: "Compare GitHub Copilot, OpenAI Codex, and Claude Code billing, cache behavior, usage diagnostics, session management, and subscription limits."
track: copilot
verified: 2026-11-19
sidebar:
  order: 12
---

Coding agent subscriptions can drain faster than expected because **raw total tokens**, **API charges**, and **subscription quota deductions** are different measurements. This guide compares **GitHub Copilot** (VS Code · CLI · SDK), **OpenAI Codex** (CLI · IDE · Cloud), and **Claude Code** using official documentation.

> **Written and verified as of 2026-11-19.** Prices, plan limits, supported models, and cache behavior can change. This is a review of public documentation, not an independent billing experiment. Check the linked original sources and your account's usage screen before making a purchasing or operating decision. API prices do not automatically describe subscription deductions.

<a id="total-tokens-vs-cost-vs-quota"></a>
## 1. Core principle: Total tokens ≠ API bill ≠ subscription quota

A total-token ratio from a single task is not a cost or efficiency ratio. Compare the same task, model, reasoning settings, cache accounting, and success criteria before drawing conclusions.

In an agentic loop (**search → read → edit → test → re-read on failure**), later requests commonly include earlier conversation and tool results as logical input. An unchanged prefix may be served from a prompt cache instead of being recomputed. This does not mean every client re-sends all bytes over the network: continuation APIs and persistent connections can reduce transport overhead ([Claude Code request anatomy](https://code.claude.com/docs/en/prompt-caching#how-the-cache-is-organized), [VS Code engineering explanation](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot)).

<figure class="at-doc-figure">
  <div class="at-diag-stack">
    <div class="at-diag-row">
      <span>1. Uncached + Cache Write</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-miss" style="width:100%">Separate ordinary input from cache writes</div>
      </div>
      <span>Use the applicable input rate</span>
    </div>
    <div class="at-diag-row">
      <span>2. Prompt Cache Read</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-hit" style="width:100%">Reused prefix, model-specific read rate</div>
      </div>
      <span>Separate volume from price</span>
    </div>
    <div class="at-diag-row">
      <span>3. Billed Output (+Reasoning)</span>
      <div class="at-diag-bar">
        <div class="at-diag-seg seg-tool" style="width:100%">Use reported output, including reasoning</div>
      </div>
      <span>Do not add reasoning twice</span>
    </div>
  </div>
  <figcaption>
    Figure 1. Conceptual token groups, not measured proportions. Equal-width bars do not imply equal token counts, costs, or subscription weights.
  </figcaption>
</figure>

Across $T$ model calls, calculate text-token charges using mutually exclusive buckets and the applicable model, context tier, and cache lifetime:

```text
TextTokenCost = Σ_t ( C_in · UncachedIn_t + C_write · CacheWrite_t + C_read · CacheRead_t + C_out · BilledOut_t )
```

- **Do not double-count input:** Normalize provider usage fields into these buckets first. A cache-write rate is the total rate for those tokens, not ordinary input plus another full write-rate charge. Split writes by lifetime when rates differ. Tool charges and other billable services are outside this text-token formula ([Anthropic cache pricing](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing), [OpenAI cache pricing rules](https://developers.openai.com/api/docs/guides/prompt-caching)).
- **Do not double-count reasoning (`BilledOut_t`):** Use the provider's reported `output_tokens`. OpenAI reports the reasoning subset as `output_tokens_details.reasoning_tokens`; Anthropic also bills thinking as output, including thinking not shown to the user. Do not add reasoning to an already inclusive output total or substitute `max(output_tokens, reasoning_tokens)` for documented field semantics ([OpenAI reasoning usage](https://developers.openai.com/api/docs/guides/reasoning), [Anthropic thinking](https://platform.claude.com/docs/en/build-with-claude/thinking)).

---

<a id="three-agent-comparison"></a>
## 2. Three-agent billing, prompt-cache & quota comparison (2026-11-19)

The table separates **provider API rates** from **product subscription or credit rules**. API behavior is not a guarantee about every client, gateway, or subscription. Dollar plan prices below are monthly USD list prices before applicable taxes.

| Dimension | GitHub Copilot (VS Code · CLI · SDK) | OpenAI Codex (CLI · IDE · Cloud) | Claude Code (Pro / Max / Ultra) |
|---|---|---|---|
| **Quota & billing model** | Usage-based plans convert model-priced tokens into **GitHub AI Credits (`1 credit = $0.01 USD`)**. Individual allowances and organization pools differ ([individuals](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing), [organizations](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)). | Plus and Standard Business have usage estimates per 5-hour period; **Pro currently has no 5-hour limit**. Local messages and cloud chats share the allowance; weekly limits may also apply. API-key sessions use separate API billing ([pricing and limits](https://learn.chatgpt.com/docs/pricing)). | **Max (`$100`)**: `5×` Pro usage every 5 hours and every week. **Ultra (`$200`)**: `10×` Pro weekly usage, with no 5-hour limit for verified accounts ([plans and conditions](https://claude.com/pricing)). |
| **Prompt-cache read rate** | Use **GitHub's published model and context-tier rates**, not an assumed universal provider discount ([model pricing](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)). | **API:** most GPT-5.6+ models `0.10×`; GPT-6.1 Sol/Terra and GPT-6.2 Astra/Sol `0.05×`. **Codex credits:** consult the separate token rate card ([API caching](https://developers.openai.com/api/docs/guides/prompt-caching), [Codex rates](https://learn.chatgpt.com/docs/pricing#token-rates)). | **API:** standard `0.10×` input; Opus 5.5 / Sonnet 5.5 `0.05×`; Fable 5.1/5.2 and Mythos 5.1/5.2 `0.025×` ([model-specific table](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing)). |
| **Prompt-cache write rate** | Write rates are listed for Anthropic and supported newer OpenAI models. Do not infer the charge from undocumented client fields ([GitHub rate card](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)). | **API GPT-5.6+:** default `30m` writes `1.25×`; requested `12h` writes `2.5×`. **Codex credit billing has no separate cache-write charge** ([API rules](https://developers.openai.com/api/docs/guides/prompt-caching#cache-lifetime), [Codex rules](https://learn.chatgpt.com/docs/pricing#token-rates)). | **API:** `5m` writes cost `1.25×` ordinary input; `1h` writes `2.0×`. These are total write rates, not additive fees ([cache pricing](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing)). |
| **Cache lifetime** | Client/model-specific. The **2026-06-17 VS Code report** describes extended OpenAI retention and Anthropic breakpoints, not a permanent guarantee for every Copilot surface ([engineering report](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot)). | **API GPT-5.6+:** `prompt_cache_options.ttl` selects a minimum `30m` or `12h`; older models use different retention settings. These are not universal Codex client defaults ([lifetime rules](https://developers.openai.com/api/docs/guides/prompt-caching#cache-lifetime)). | Default main conversation: `1h` **within a Claude subscription's included usage**, but `5m` with usage credits, an API key, or a cloud provider. Subagents normally get `5m`; overrides exist ([TTL conditions](https://code.claude.com/docs/en/prompt-caching#which-ttl-each-request-gets)). |
| **Where to verify usage** | Compare the usage dashboard with your plan and budgets. Organization user budgets always hard-stop; spending limits require the stop-usage option ([budget semantics](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing#how-can-i-control-costs-with-budgets)). | Use the account usage dashboard for current limits and reset times, rather than treating internal header names as a stable plan contract ([usage guidance](https://learn.chatgpt.com/docs/pricing#included-usage)). | `/usage`: distinguish the session's API-cost estimate from subscription usage bars ([usage screen](https://code.claude.com/docs/en/costs#track-your-costs)). |

**Legacy Copilot exception:** Existing annual Pro/Pro+ subscriptions that remain on request-based billing have `300` / `1,500` monthly premium requests. User prompts count with model multipliers, autonomous tool calls do not, and code review consumes `13` requests per review. These rules do **not** describe usage-based token billing ([legacy rules](https://docs.github.com/en/copilot/reference/copilot-billing/request-based-billing-legacy/copilot-requests)).

<a id="product-surface-and-authentication"></a>
### Product surface and authentication scope

Record the client, version, harness, and authentication mode before applying a pricing or caching rule. A provider API option is not necessarily exposed by an agent's UI or configuration.

| Product | Scope to record | Do not assume |
|---|---|---|
| **GitHub Copilot** | VS Code, CLI, or SDK; GitHub-billed model or a separately configured provider. In VS Code, also record the harness. SDK **BYOK** sends requests using your provider credentials for direct provider billing ([SDK BYOK](https://docs.github.com/en/copilot/how-tos/copilot-sdk/auth/byok), [VS Code harness differences](https://code.visualstudio.com/docs/agents/run/subagents)). | SDK BYOK costs are not the same meter as GitHub AI Credits. CLI commands and Local-harness tools are not interchangeable with every VS Code or SDK session. |
| **OpenAI Codex** | Local CLI/IDE work can use ChatGPT sign-in or an API key; **Codex cloud requires ChatGPT sign-in**. API-key sessions charge the Platform account rather than included ChatGPT usage ([authentication](https://learn.chatgpt.com/docs/auth)). | An API cache parameter or append-only configuration update is not automatically a Codex client setting. Local API-key access does not imply access to cloud features. |
| **Claude Code** | Claude subscription, API key, or cloud provider; included usage versus paid usage credits. These affect billing and default cache TTL ([cost tracking](https://code.claude.com/docs/en/costs), [TTL rules](https://code.claude.com/docs/en/prompt-caching#which-ttl-each-request-gets)). | A subscription's session cost estimate is not its quota deduction, and the subscription's main-thread TTL is not every authentication path's default. |

<a id="usage-observability"></a>
### Usage diagnostics: where to look and what the numbers mean

| Product | Concrete inspection path | Interpretation and limits |
|---|---|---|
| **GitHub Copilot** | **CLI:** `/context` shows context composition; `/usage` shows session/per-model usage and AI Credits on token-billed accounts. **VS Code:** run **Developer: Open Agent Debug Logs**, open the session **Summary**, then **Cache Explorer** for reported reuse and request divergence. **SDK:** subscribe to `assistant.usage`; use `session.usage_info` for context utilization ([CLI reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference), [debug views](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view), [SDK metrics](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing)). | Context occupancy is not cumulative billed usage. Debug capture must be enabled before reproduction and is Preview. Cache Explorer does not prove provider billing. SDK usage events include subagent calls and are not replayed on resume; accumulated metrics RPCs are experimental, so pin SDK/runtime versions. |
| **OpenAI Codex** | **CLI:** `/status` shows remaining plan limits; use the [usage dashboard linked by OpenAI](https://learn.chatgpt.com/docs/pricing) for account readings. **Automation:** `codex exec --json` emits `turn.completed.usage`, including `input_tokens`, `cached_input_tokens`, and `output_tokens` ([JSONL events](https://learn.chatgpt.com/docs/non-interactive-mode)). | A turn's token totals are not the same thing as its subscription deduction or a per-model-call trace. Do not add cached input to an already inclusive input total. Record the authentication mode with the event. |
| **Claude Code** | Run `/usage` for plan bars and session estimates. For structured output, inspect `cache_creation_input_tokens` and `cache_read_input_tokens` in reported usage ([usage screen](https://code.claude.com/docs/en/costs#track-your-costs), [cache fields](https://code.claude.com/docs/en/prompt-caching#check-cache-performance)). | Separate subscription bars from estimated API dollars. Cache creation and read fields describe token categories, not a published conversion into subscription percentages. |

If a client/provider does not expose a field, record **not exposed**, not zero. Preserve whether each reading is per call, per turn, per session, or account-wide; do not add event totals to an aggregate that already includes them. Review logs for prompts, source code, and credentials before sharing ([SDK field semantics](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing), [Cache Explorer limits](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)).

---

<a id="cache-illusion-and-subscription-quotas"></a>
## 3. The "Cache Illusion" and subscription quota opacity

API invoices and flat-rate subscription meters do not measure usage identically:

1. **List-price valuation creates a cache illusion:** Pricing all cache reads as new input overstates the equivalent API input charge. The amount depends on the actual token mix and model rates; there is no universal inflation factor ([Anthropic rates](https://platform.claude.com/docs/en/build-with-claude/prompt-caching#pricing), [OpenAI rules](https://developers.openai.com/api/docs/guides/prompt-caching)).
2. **An API estimate is not a subscription receipt:** Claude Code explicitly says its session cost figure is not the billing amount for Pro/Max/Ultra subscribers. The cited plan pages do not provide a fixed cache-token-to-quota conversion, so this guide does not invent one. Codex likewise states that credit prices alone do not determine included subscription usage ([Claude usage guidance](https://code.claude.com/docs/en/costs#track-your-costs), [Codex token rates](https://learn.chatgpt.com/docs/pricing#token-rates)).
3. **Removing a short window does not remove all limits:** Verified Claude Ultra accounts retain weekly limits, while Codex says weekly limits may apply even though Pro has no 5-hour limit. Copilot instead has monthly credit allowances and budget controls. Check the actual account's meter ([Claude plans](https://claude.com/pricing), [Codex limits](https://learn.chatgpt.com/docs/pricing#included-usage), [Copilot allowances](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing)).

<a id="when-allowance-runs-out"></a>
### When the included allowance runs out

| Product | Limit reached → continuation path | Guardrail |
|---|---|---|
| **GitHub Copilot** | Individual users can configure additional paid usage or wait for the next allowance. Organization users may need an administrator to grant budget; additional usage also depends on policy and spending limits ([individual rules](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing), [organization rules](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing)). | Check hard-stop settings before continuing. Inline completions and next edit suggestions are not billed in AI Credits and remain available under the documented paid-plan rules; a blocked chat budget does not mean every feature stops. |
| **OpenAI Codex** | Eligible Plus/Pro users can buy credits; flexible workspace plans can add workspace credits. An active turn may continue subject to fair-use limits. Additional local API-key work uses separate API billing ([limit handling](https://learn.chatgpt.com/docs/pricing), [authentication scope](https://learn.chatgpt.com/docs/auth)). | API-key billing does not replenish the ChatGPT allowance or unlock Codex cloud. If no authorized continuation path is available, check the account's reset time. |
| **Claude Code** | Configured usage credits can fund paid continuation beyond included plan usage; otherwise follow the plan's reset or upgrade options. `/usage` distinguishes plan usage and usage-credit spend ([cost and usage controls](https://code.claude.com/docs/en/costs)). | Do not mistake the session's API-dollar estimate for a subscription balance. Switching into usage credits also changes the default main-thread TTL unless overridden. |

---

<a id="six-token-drain-mechanisms"></a>
## 4. Six real-world mechanisms that drain tokens and quotas

<a id="1-search-scope-shell-dumps-and-retry-loops"></a>
### 4.1 Search scope, shell dumps, and retry loops matter, not tool names

Tool names alone do not establish efficiency. An unbounded file or search dump can enlarge later requests; an over-narrow read can omit the definition or caller needed to fix a bug. Bound output before it first enters the model's context, and read enough to avoid preventable retry loops ([Claude cost guidance](https://code.claude.com/docs/en/costs#reduce-token-usage)).

Prefer definition-and-caller context with bounded results, for example [`cctx`](../../copilot-tokenomics/cctx/). **Do not silently rewrite old tool results to shrink history:** changing an existing prefix can lose cache reuse and, on newer OpenAI models, reasoning continuity. Filter new results first or use the provider's compaction mechanism ([OpenAI prefix guidance](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged)).

Product controls differ: Copilot CLI can offload large tool results to a temporary file; Codex supports per-MCP-tool `output_token_limit` settings. Neither removes the need to retain failure status, warnings, and enough context to diagnose the result ([Copilot output handling](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management), [Codex MCP controls](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)).

<a id="2-standing-prompt-and-schema-tax"></a>
### 4.2 Standing prompt and MCP schema tax (`standing tokens × steps`)

System instructions, loaded project rules, and eagerly loaded tool schemas can recur across calls. **Enabled tool does not always mean full schema sent upfront:** deferred loading can keep unused schemas out of context. VS Code's report describes discovering tools after the cached prefix, and Claude Code documents when deferred MCP changes preserve its cache ([VS Code tool search](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot), [Claude MCP cache rules](https://code.claude.com/docs/en/prompt-caching#connecting-or-removing-an-mcp-server)).

Keep persistent instructions short and load optional tools on demand where supported. Do not apply a single schema layout or savings percentage to every client.

| Product | Instruction/tool controls | How to verify |
|---|---|---|
| **GitHub Copilot** | CLI `copilot instruction list` inventories discovered instructions; `/mcp` configures servers and available tool names instead of an unrestricted `*` ([instruction reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference), [MCP configuration](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers)). | Inspect `/context` for instruction/MCP shares. In VS Code, inspect the actual request in Chat Debug; discovery alone does not prove inclusion. |
| **OpenAI Codex** | `AGENTS.md` / `AGENTS.override.md` form a startup instruction chain. MCP `enabled_tools` and `disabled_tools` control the catalog; `enabled = false` disables a server ([instruction discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md), [MCP options](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)). | Restart in the intended directory after changing startup instructions; inspect `/mcp` and loaded instruction sources. Do not infer native API `defer_loading` behavior from a client tool list. |
| **Claude Code** | `CLAUDE.md` is loaded into project context; MCP tool search can defer definitions. Upfront tool-list changes and deferred changes have different cache effects ([prompt layers and MCP rules](https://code.claude.com/docs/en/prompt-caching)). | Compare reported cache creation/read usage and the loaded tool set. A permission rule is not automatically removal of the tool's schema. |

<a id="3-silent-cache-invalidators"></a>
### 4.3 Three cache-reuse risks and their exceptions

1. **Model or reasoning-setting changes:** A cache is model-specific; changing effort can also change the prefix, but **not always**. Claude Code documents cache-preserving effort changes for supported newer models and authentication paths. Supported OpenAI models accept an appended `configuration_update` while the original request-level `reasoning.effort` stays unchanged ([Claude exceptions](https://code.claude.com/docs/en/prompt-caching#changing-effort-level), [OpenAI append-only updates](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged)). Check the client implementation rather than assuming every change causes a full miss.
2. **Idle gaps and retention settings:** An expired or unavailable prefix requires fresh processing. Distinguish provider-specific inactivity expiry from OpenAI's newer **minimum** retention lifetimes; reaching a requested minimum is not itself a guaranteed eviction deadline ([Claude TTL](https://code.claude.com/docs/en/prompt-caching#cache-lifetime), [OpenAI lifetime semantics](https://developers.openai.com/api/docs/guides/prompt-caching#cache-lifetime)).
3. **Unnecessary compaction:** Claude `/compact` makes a summarization request and changes the conversation layer, but does **not necessarily discard the system-prompt cache**. Its cost depends on whether the summarization input is warm and how much history remains. No fixed remaining-turn break-even threshold is established here. Compact at natural boundaries when future context savings justify the work; use `/clear` with saved decisions for unrelated work ([Claude compaction details](https://code.claude.com/docs/en/prompt-caching#compacting-the-conversation)).

In Copilot, use [Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer) to compare the first request divergence with reported reuse before blaming a model/effort change. A request diff is diagnostic evidence, not proof of causation or a billing receipt.

<a id="compaction-and-new-sessions"></a>
#### Compaction and fresh-session controls by product

| Product | Continue related work | Switch to unrelated work |
|---|---|---|
| **GitHub Copilot** | CLI supports automatic compaction and `/compact`; VS Code also exposes manual compaction. CLI `/session checkpoints` lets you inspect saved summaries ([context controls](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management)). | Save decisions, then use CLI `/new` or `/clear`, or start a new VS Code chat. A compacted summary preserves goals and key state, not every earlier detail ([CLI commands](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)). |
| **OpenAI Codex** | CLI `/compact` reduces conversation history. Automatic-compaction configuration is model/client-specific; do not copy another product's thresholds ([commands](https://learn.chatgpt.com/docs/developer-commands?surface=cli), [configuration sample](https://learn.chatgpt.com/docs/config-file/config-sample)). | Start a new chat/session with the necessary handoff. `codex fork` copies an existing session for an alternative branch; it is not an empty context. |
| **Claude Code** | `/compact` summarizes the conversation and rebuilds changed layers while eligible stable layers may remain cached ([compaction layers](https://code.claude.com/docs/en/prompt-caching#compacting-the-conversation)). | Save the handoff and use `/clear`. Check the new session's instructions and usage; shorter history is not proof of preserved task quality. |

<a id="4-subagents-vs-fork"></a>
### 4.4 Delegation, parallel work, and session branching

| Product | Delegated context and results | Usage and fork/cache expectations |
|---|---|---|
| **GitHub Copilot** | CLI `/fleet` coordinates independent subagents with separate contexts. VS Code behavior depends on the harness: Local's `runSubagent` does not inherit the main conversation; the Copilot harness uses native `task` instead. Give each task a bounded goal and expected result ([fleet behavior](https://docs.github.com/en/copilot/concepts/agent-capabilities/choosing-and-delegating-agent-work/delegating-and-parallelizing-work-with-agents), [VS Code differences](https://code.visualstudio.com/docs/agents/run/subagents)). | Separate model work can consume more AI Credits. CLI `/fork` can preserve a conversation copy, but that does **not establish a provider-cache guarantee** ([CLI reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)). |
| **OpenAI Codex** | Local clients can run parallel subagents and collect their summaries. Record child model/effort choices; custom agents can change them. Keep noisy intermediate work outside the main thread ([subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents)). | OpenAI warns that subagent workflows consume more tokens than comparable single-agent runs. `codex fork` copies saved history; the cited client documentation does **not guarantee cache reuse** for every fork ([fork commands](https://learn.chatgpt.com/docs/developer-commands?surface=cli)). |
| **Claude Code** | Subagents start separate conversations with their own system prompts/tools and normally a `5m` TTL unless overridden. The parent receives the result without inheriting all intermediate work ([subagent caching](https://code.claude.com/docs/en/prompt-caching#subagents-and-the-cache)). | A subagent's initial prefix differs from the parent's. A fork inherits the parent's prompt, tools, and history and can reuse a matching, available cache; cache-scope conditions still apply. |

**Isolation is not free execution.** Compare total parent-plus-child work, latency, and task success, counting each call once. Delegate independent, substantial work rather than a quick lookup or a sequential trace; see the [delegation break-even guidance](../../copilot-tokenomics/03-context-hygiene/#subagent-delegation-break-even-rule).

<a id="5-tokenizer-and-generation-inflation"></a>
### 4.5 Model generation and tokenizer inflation

Changing models can change both tokenization and agent behavior:
- **Behavior is workload-dependent:** VS Code reports increased tokens per task across successive generations in its own data. That is a vendor observation, not a law that every newer model always uses more tokens ([engineering report](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot)).
- **Recount the same input:** Anthropic says Claude 4.7 and later models and Mythos Preview use a newer tokenizer producing approximately **30% more tokens** than earlier models, depending on content and workload. This is **vendor-reported, not independently reproduced here**, and not a fixed multiplier for every English or Korean prompt. Use the intended model's token counter and actual request usage ([official token-counting note](https://platform.claude.com/docs/en/build-with-claude/token-counting#supported-models), [Language and tokenization](../../copilot-tokenomics/07-language-and-tokenization/)).

Copilot and Codex are **products, not tokenizers**. To isolate harness behavior, select the same available model/version and comparable effort in each product; otherwise label the result a **product-plus-model comparison**. Public prompt text alone does not equalize hidden instructions, tools, or cache warmth. The [controlled exercise below](#controlled-comparison-exercise) records those limits instead of assigning a universal product token multiplier.

<a id="6-reasoning-and-output-weight"></a>
### 4.6 Output and reasoning have their own rate

Reasoning is billed as output, but there is no universal output/input or output/cache-read multiplier. For example, GitHub's default-tier GPT-6.1 Sol rate card lists `$2` input, `$0.10` cached input, `$2.50` cache write, and `$10` output per million tokens; other models and long-context tiers differ ([rate card](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing)).

Choose reasoning effort appropriate to the task, then verify both spend and success. Lower effort can reduce token use but is not a quality-preserving fix by itself ([OpenAI effort guidance](https://developers.openai.com/api/docs/guides/reasoning#reasoning-effort)).

| Product | Select and verify model/effort | Cost interpretation |
|---|---|---|
| **GitHub Copilot** | Choose from the client's available model controls; CLI supports `--model`. In VS Code, record supported reasoning controls and inspect the actual request rather than assuming an API setting is exposed ([CLI model reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference), [Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer)). | Use GitHub's rate card and the account's billing mode. SDK BYOK instead uses the configured provider's meter. |
| **OpenAI Codex** | CLI `/model` selects an available model; verify with `/status`. Record configured reasoning effort and any child overrides ([commands](https://learn.chatgpt.com/docs/developer-commands?surface=cli), [subagent settings](https://learn.chatgpt.com/docs/agent-configuration/subagents)). | Separate ChatGPT allowance/credits from API-key charges. A provider's reasoning configuration update is not proof that every client changes effort the same way. |
| **Claude Code** | `/model` and effort controls select supported settings; authentication/model exceptions determine whether effort changes preserve cache ([model/effort cache rules](https://code.claude.com/docs/en/prompt-caching)). | Interpret API output charges separately from plan bars. Record cache state when testing a model or effort change. |

---

<a id="practical-playbook-and-five-metrics"></a>
## 5. Practical operating routine & the 5-metric comparison standard

### Everyday operating routine

| Situation | Recommended habit | Anti-pattern to avoid |
|---|---|---|
| **Starting a bug fix** | Pass the **error message, target file paths, and repro command** upfront to narrow initial search | Vague prompts (*"fix login"*) that trigger repo-wide `Bash rg` scans |
| **Source lookup & reading** | Retrieve bounded definitions and callers; [`cctx`](../../copilot-tokenomics/cctx/) is one optional implementation | Whole-file dumps or arbitrary slices that omit the relevant definition |
| **Test & build logs** | Run tests; preserve status/counts and extract relevant failures before sending new output | Skipping verification or rewriting past tool messages to shrink them |
| **Switching vs. continuing tasks** | Record decisions before a fresh session; compact continuing work when future savings justify it | Treating a fixed remaining-turn count as a universal compaction rule |
| **Model & effort selection** | Start with task-appropriate settings; check cache-preserving update support before changing them | Assuming every effort change is free, or every effort change invalidates all cache |

<a id="product-operating-checklists"></a>
### Product-specific operating checklists

| Product | Start and scope | Control growth and switch tasks | Verify the result |
|---|---|---|---|
| **GitHub Copilot** | Supply target files, error, and repro command; select the model and VS Code harness. CLI users can inspect instructions and `/mcp`. | CLI `/context` identifies large components. Keep new log results bounded; `/compact` continues related work, while `/new` starts an unrelated task. Use the corresponding VS Code controls rather than assuming CLI parity. | Run tests, compare CLI `/usage` or VS Code Summary/Cache Explorer, and check the account's AI Credits. SDK users capture live usage events separately from accumulated totals. |
| **OpenAI Codex** | Confirm sign-in mode and `/status`, select the model, and check the relevant `AGENTS.md` chain and MCP tools. | Return focused test failures, not entire repeated logs. Use `/compact` for continuing work or start a fresh chat for unrelated work; a fork retains prior context. | Run tests; preserve `codex exec --json` usage when automating. Compare account limits/credits for ChatGPT sign-in or API billing for an API key. |
| **Claude Code** | Confirm model, loaded `CLAUDE.md`/tools, authentication mode, and `/usage` readings. | Read complete relevant definitions with bounded output. Use `/compact` at useful boundaries and `/clear` with a saved handoff for unrelated work. | Run tests and compare cache creation/read usage; keep API-dollar estimates and subscription bars separate. |

These are application checklists for the [diagnostic paths](#usage-observability), [tool controls](#2-standing-prompt-and-schema-tax), and [session controls](#compaction-and-new-sessions) above, not evidence that the clients have identical internals.

### Five metrics required for fair agent & tool comparisons

Whenever you compare coding agents or evaluate an optimization, record the task, model/version, client/version, authentication/billing mode, and cache settings first. Then record these **five groups** side by side, using available telemetry such as [`tokprof`](../../agent-tokenomics/tokprof/):

1. **Uncached input and cache-write tokens, separately:** retain lifetime/rate differences rather than treating them as one price.
2. **Cache-read tokens and hit rate:** state the denominator, for example the fraction of total input served from cache.
3. **Billed output with any reported reasoning subset:** do not count reasoning twice.
4. **Actual metered cost/credits and end-to-end time:** keep API estimates separate from subscription usage readings.
5. **Passed tests / task success ([Rule Zero](../../agent-tokenomics/#rule-zero)):** report quality next to cost, including failed attempts.

<a id="controlled-comparison-exercise"></a>
### Controlled comparison exercise, not a measured benchmark

Use the **same English prompt below in each product**. Select the same available model/version and comparable effort where possible. Record client/harness, authentication, loaded instructions/tools, and reported cache state; a new chat does not guarantee a cold provider cache.

```text
Fix this Python function. Return only the corrected function.
Contract: return the arithmetic mean of a nonempty numeric list,
or None for an empty list. Do not use tools.

def mean(xs):
    return sum(xs) / len(xs)
```

In an isolated scratch environment, check the returned function with the same assertions:

```python
assert mean([]) is None
assert mean([2, 4]) == 3.0
assert mean([0]) == 0.0
```

| Product | Tokens by available category | Actual cost/credits and elapsed time | Passed checks / total |
|---|---|---|---|
| GitHub Copilot | Not measured | Not measured | Not run |
| OpenAI Codex | Not measured | Not measured | Not run |
| Claude Code | Not measured | Not measured | Not run |

This is a **synthetic smoke-test protocol**; no agent calls or savings measurements are claimed here. Replace the placeholders only with observations. Mark unavailable fields **not exposed**, keep dollars and product credits in separate units, and include retries and delegated calls. Different models make this a product-plus-model comparison. For real workflow conclusions, repeat a representative fixed task set and report task success alongside cost; these assertions alone are not a coding-quality benchmark.

---

<a id="faq"></a>
## 6. Frequently asked questions

<div class="at-faq-item">
  <h3>Does 4× higher total token usage mean 4× higher cost or quota drain?</h3>
  <p>No. Cost depends on token categories and applicable rates; subscription deductions are a separate measurement. Use the <a href="#three-agent-comparison">linked rate cards and plan rules</a>, then compare actual usage alongside task success.</p>
</div>

<div class="at-faq-item">
  <h3>Does running /compact frequently always save tokens and subscription quota?</h3>
  <p>No. Copilot, Codex, and Claude Code expose different compaction and fresh-session controls. Summarization and rebuilding changed history have a cost, while retained context can save later work. Use the <a href="#compaction-and-new-sessions">product-specific controls</a> and compare usage plus task success rather than a fixed turn-count rule.</p>
</div>

<div class="at-faq-item">
  <h3>Does changing reasoning effort always invalidate the prompt cache?</h3>
  <p>No. Behavior depends on the model, client, and authentication path. Copilot's Cache Explorer can help investigate reported reuse; supported OpenAI APIs and Claude models have cache-preserving update paths, but a client may not expose them. See the <a href="#3-silent-cache-invalidators">exceptions and original documentation</a>.</p>
</div>

<div class="at-faq-item">
  <h3>Does exhausting Copilot AI Credits stop every coding feature?</h3>
  <p>No. Credit-consuming features depend on additional-usage policy and budget controls, while inline completions and next edit suggestions are not billed in AI Credits under the documented paid-plan rules. See <a href="https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing">Copilot billing and budget behavior</a>.</p>
</div>

<div class="at-faq-item">
  <h3>Does using an API key in Codex increase my ChatGPT plan allowance?</h3>
  <p>No. API-key sessions use separate Platform billing for supported local workflows. They do not replenish ChatGPT usage limits, and Codex cloud requires ChatGPT sign-in. See <a href="https://learn.chatgpt.com/docs/auth">Codex authentication and billing scope</a>.</p>
</div>

<a id="sources"></a>
## Sources

Original sources checked on **2026-11-19**. The VS Code engineering post describes its **2026-06-17** rollout; it is not evidence that every present-day client has identical defaults.

- **Copilot policies and prices:** [Individual billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/individuals/billing), [organization billing](https://docs.github.com/en/copilot/concepts/billing-and-usage/organizations-and-enterprises/billing), [model rates](https://docs.github.com/en/copilot/reference/copilot-billing/models-and-pricing), [legacy request rules](https://docs.github.com/en/copilot/reference/copilot-billing/request-based-billing-legacy/copilot-requests).
- **Copilot workflows:** [CLI commands](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference), [context and checkpoints](https://docs.github.com/en/copilot/concepts/prompting-and-context/context-management), [MCP configuration](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers), [delegation](https://docs.github.com/en/copilot/concepts/agent-capabilities/choosing-and-delegating-agent-work/delegating-and-parallelizing-work-with-agents).
- **Copilot SDK and VS Code diagnostics:** [SDK usage metrics](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing), [SDK BYOK](https://docs.github.com/en/copilot/how-tos/copilot-sdk/auth/byok), [debug views](https://code.visualstudio.com/docs/agents/agent-troubleshooting/chat-debug-view), [Cache Explorer](https://code.visualstudio.com/docs/agents/agent-troubleshooting/cache-explorer), [subagents by harness](https://code.visualstudio.com/docs/agents/run/subagents).
- **Codex product behavior:** [Plans and credits](https://learn.chatgpt.com/docs/pricing), [authentication](https://learn.chatgpt.com/docs/auth), [developer commands](https://learn.chatgpt.com/docs/developer-commands?surface=cli), [JSONL usage events](https://learn.chatgpt.com/docs/non-interactive-mode), [subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents).
- **Codex configuration and OpenAI APIs:** [Instructions](https://learn.chatgpt.com/docs/agent-configuration/agents-md), [MCP controls](https://learn.chatgpt.com/docs/extend/mcp?surface=cli), [configuration sample](https://learn.chatgpt.com/docs/config-file/config-sample), [API prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching), [reasoning usage](https://developers.openai.com/api/docs/guides/reasoning), [prefix-preserving updates](https://developers.openai.com/api/docs/guides/keep-prefix-unchanged).
- **Claude product policies:** [Plans and pricing](https://claude.com/pricing), [usage and API-cost estimates](https://code.claude.com/docs/en/costs).
- **Claude cache and token semantics:** [Claude Code caching](https://code.claude.com/docs/en/prompt-caching), [API cache rates](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [thinking billing](https://platform.claude.com/docs/en/build-with-claude/thinking), [token counting](https://platform.claude.com/docs/en/build-with-claude/token-counting).
- **VS Code engineering, vendor-reported rather than independently reproduced here:** [Improving token efficiency for GitHub Copilot in VS Code](https://code.visualstudio.com/blogs/2026/06/17/improving-token-efficiency-in-github-copilot).
