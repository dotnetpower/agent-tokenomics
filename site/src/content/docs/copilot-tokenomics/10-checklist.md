---
title: Checklist
description: A one-page routine for lower Copilot cost.
track: copilot
verified: 2026-11-03
sidebar:
  order: 10
---

## Before you start a session

- [ ] New task → new chat. Side question → `/fork`.
- [ ] Only the tools/MCP servers this task needs are enabled.
- [ ] Model chosen deliberately (auto by default); thinking effort left at default.
- [ ] Noisy paths excluded (`.gitignore`, `files.exclude`, `search.exclude`).
- [ ] Prompt names the files/errors/constraints and the success criteria.
- [ ] Prompt ends with an output limit (`Code only.` / `3 bullets max.`); one task per prompt.
- [ ] Standing instructions and rules are written in English, terse, and de-duplicated.

## During

- [ ] Use #-mentions for known files; `#codebase` only when you need search.
- [ ] Complex change? Plan first, then implement; delegate bounded pieces to cheaper subagents.
- [ ] Ambiguous task? Use Ask/Plan before Agent; give a clear "done" criterion.
- [ ] Session getting long and still needed? `/compact` with a focus hint.
- [ ] Bulk/polling work → run a script, hand the model the summary.

## Weekly review

- [ ] Open the Copilot status dashboard: monthly allowance used?
- [ ] Find your three most expensive sessions (context-window popover). What made them large?
- [ ] Run `/chronicle:cost-tips`.
- [ ] Prune instructions and tools you never needed ([MCP audit](../09-mcp-audit/)).

## When in doubt

Measure ([page 2](../02-measure-usage/)), change one thing, measure again, keep it only if quality held.
