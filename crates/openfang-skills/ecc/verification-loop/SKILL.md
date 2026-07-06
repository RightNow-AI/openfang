---
name: ecc-verification-loop
description: OMTAE verification gate — tool evidence required before factual claims. Adapted from ECC verification-loop.
origin: ECC
---

# ECC Verification Loop (OMTAE)

Use when an OMTAE agent must prove claims with tools before answering.

## When to Activate

- User asks about brain vault, Obsidian, peer agents, service status, or file contents
- Agent is about to list paths, URLs, counts, or ranked recommendations
- Prior turn was blocked with `TOOL_REQUIRED` or planning-loop nudge

## OMTAE Tool Checklist

Run the lightest check that answers the question:

| Claim type | Required tool |
|------------|---------------|
| Daemon / API health | `shell_exec`: `curl -s http://127.0.0.1:4200/api/health` |
| Brain vault status | `shell_exec`: `curl -s -H "X-OMTAE-Pin: <pin>" http://127.0.0.1:4200/api/brain/status` |
| Brain search | `shell_exec`: `curl -s -H "X-OMTAE-Pin: <pin>" "http://127.0.0.1:4200/api/brain/search?q=..."` |
| Workspace files | `file_list` on workspace root or vault path |
| File contents | `file_read` on specific path from `file_list` output |
| Public facts | `web_search` then `web_fetch` on top URLs |
| Prior context | `memory_recall` — never treat recall as live system status |

## Rules

1. **No tool output → no factual claim.** Say "checking…" and call a tool.
2. **Quote evidence.** Paste key lines from tool results; do not paraphrase into new facts.
3. **One command per shell_exec** when ops-fixer policy applies (no pipes).
4. **Stop on TOOL_REQUIRED.** Do not repeat the same planning sentence — change the tool or parameters.

## Report Format

```
VERIFICATION
- Tools used: shell_exec, file_list, …
- Evidence: <excerpt from tool output>
- Answer: <only claims backed by evidence>
- Gaps: <what could not be verified>
```
