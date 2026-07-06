---
name: ecc-tool-discipline
description: Agent architecture tool-discipline layer for OMTAE. Enforces tool-before-claim at every turn. Adapted from ECC agent-architecture-audit layer 7.
origin: ECC
---

# ECC Tool Discipline (OMTAE)

Layer-7 fix from ECC agent-architecture-audit: tools declared in prompt must be **called**, not narrated.

## Symptoms This Skill Fixes

- "Must use tool X" in prompt but model answers without calling it
- Tool results look correct but were never executed
- Model describes peer agents, vault files, or health without `shell_exec` / `file_list`

## Per-Turn Contract

Before sending a user-visible answer, confirm:

- [ ] Did I call a tool for every factual claim in this message?
- [ ] Are paths/URLs copied from tool output, not invented?
- [ ] Did I avoid "The user is asking about…" meta-narration?
- [ ] If blocked by TOOL_REQUIRED, did I call a **different** tool than last attempt?

## OMTAE High-Risk Prompts

| User says | First tool |
|-----------|------------|
| brain / Obsidian / vault | `curl /api/brain/status` or `file_list` on vault |
| system check / health | `curl /api/health` or `nvidia-smi` |
| how many agents | `curl /api/agents` |
| research / top N / list | `web_search` |

## Runtime Integration

OMTAE `loop_guard` enforces this in code:

- `TOOL_REQUIRED` — factual claims without tool evidence
- Planning loop — repeated "let me check" without tools (threshold: 2)
- Research integrity — unverified URLs get a disclaimer

Prompts alone are insufficient; the runtime gates are the backstop.
