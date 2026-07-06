---
name: ecc-research-ops
description: Evidence-first research discipline for OMTAE. Separate facts, inference, and recommendations. Adapted from ECC research-ops.
origin: ECC
---

# ECC Research Ops (OMTAE)

Evidence-first workflow for researcher and analyst agents on the OMTAE daemon.

## Guardrails

- Do not answer current-state questions from memory alone — use tools
- Separate: **sourced fact** | **inference** | **recommendation** | **unverified**
- Never fabricate top-N lists when `web_search` returns nothing

## OMTAE Research Stack

1. **Local evidence** — `file_list`, `file_read`, `memory_recall`
2. **Brain API** — `curl /api/brain/search?q=...` with `X-OMTAE-Pin`
3. **Web** — `web_search` with multiple query phrasings, then `web_fetch`
4. **Synthesis** — cite URLs and command output excerpts only

## Output Labels

For each important claim, prefix:

- `[TOOL]` — from shell_exec / file_* / web_* output this turn
- `[MEMORY]` — from memory_recall (may be stale)
- `[INFERENCE]` — your reasoning
- `[UNVERIFIED]` — could not confirm with tools

## When Blocked

If runtime returns `TOOL_REQUIRED` or stops planning loops:

1. Stop meta-narration ("The user is asking about…")
2. Call the smallest tool that settles the question
3. Retry with a different query if the first tool returns empty
