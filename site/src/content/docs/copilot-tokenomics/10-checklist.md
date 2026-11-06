---
title: Checklist
description: "One-page Copilot tokenomics checklist: high-impact habits before, during, and after VS Code GitHub Copilot sessions to cut token and AI credit spend."
track: copilot
verified: 2026-11-03
sidebar:
  order: 10
---

Impact tiers are qualitative starting priorities, not measured savings; impact varies by billing plan and workload. Confirm cost and quality against your baseline.

## Before you start a session

- [ ] [High] New task → new chat. Side question → `/fork`.
- [ ] [High] Only the tools/MCP servers this task needs are enabled.
- [ ] [High] Model chosen deliberately (auto by default); thinking effort left at default.
- [ ] [Medium] Noisy paths excluded (`.gitignore`, `files.exclude`, `search.exclude`).
- [ ] [High] Prompt names the files/errors/constraints and the success criteria.
- [ ] [Medium] Prompt ends with an output limit (`Code only.` / `3 bullets max.`); one task per prompt.
- [ ] [Medium] Standing instructions and rules are written in English, terse, and de-duplicated.

## During

- [ ] [Medium] Use #-mentions for known files; `#codebase` only when you need search.
- [ ] [High] Complex change? Plan first, then implement; delegate bounded pieces to cheaper subagents.
- [ ] [High] Ambiguous task? Use Ask/Plan before Agent; give a clear "done" criterion.
- [ ] [High] Session getting long and still needed? `/compact` with a focus hint.
- [ ] [Medium] Bulk/polling work → run a script, hand the model the summary.

## Weekly review

- [ ] [Low] Open the Copilot status dashboard: monthly allowance used?
- [ ] [Medium] Find your three most expensive sessions (context-window popover). What made them large?
- [ ] [Low] Run `/chronicle:cost-tips` if available; otherwise inspect Agent Debug Logs Summary and Cache Explorer.
- [ ] [High] Prune instructions and tools you never needed ([MCP audit](../09-mcp-audit/)).

## When in doubt

Measure ([page 2](../02-measure-usage/)), change one thing, measure again, keep it only if quality held.
