---
name: ecc-search-first
description: Research-before-answering for OMTAE agents. Search repo, brain API, or web before claiming facts. Adapted from ECC search-first.
origin: ECC
---

# ECC Search-First (OMTAE)

Systematizes "check with tools before answering" inside the OMTAE daemon.

## Trigger

- Status, brain, vault, peer-agent, or workspace questions
- User asks for lists, comparisons, or recommendations
- You are about to write paths, URLs, or counts without tool output

## Workflow

```
0. PREFLIGHT — which OMTAE tools are granted? (shell_exec, file_list, web_search, memory_recall)
1. LOCAL FIRST — file_list / file_read / memory_recall on workspace or vault
2. LIVE API — curl /api/health, /api/brain/*, /api/agents (via shell_exec)
3. WEB — web_search + web_fetch for external facts
4. ANSWER — only from steps 1–3 output; label gaps as unverified
```

## OMTAE Shortcuts

| Need | Tool |
|------|------|
| Brain vault | `curl -s -H "X-OMTAE-Pin: <pin>" http://127.0.0.1:4200/api/brain/status` |
| Agent roster | `curl -s http://127.0.0.1:4200/api/agents` |
| Disk layout | `file_list` on `/home/jay/vaults/omtae-brain` or workspace |
| External research | `web_search` → `web_fetch` |

## Anti-Patterns

- Narrating peer agents without `agent_list` or `/api/agents` output
- "Let me check…" across multiple turns without a tool call
- Inventing `https://` URLs or business names without web tools
- Treating memory_recall as current daemon or vault state
