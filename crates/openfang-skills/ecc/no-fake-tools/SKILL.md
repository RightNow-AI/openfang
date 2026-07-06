---
name: ecc-no-fake-tools
description: Prevent hallucinated tool execution in OMTAE. Native function calls only; no JSON-in-text or claimed commands without shell_exec evidence.
origin: ECC
---

# ECC No Fake Tools (OMTAE)

Stops the most common OMTAE hallucination: claiming tools ran when they did not.

## Hard Rules

1. **Native function calls only** — `shell_exec`, `file_list`, `web_search`, etc. Never `{"name":"web_search",...}` in plain text.
2. **No phantom execution** — Do not write "I ran curl…" or paste fake command output unless `shell_exec` returned it this conversation.
3. **agent_send is not a JSON block** — Use the `agent_send` function with `agent_id` + `message`; never fake specialist replies.
4. **Peer agents are not facts** — Listing orchestrator/researcher/coder requires `agent_list` or `/api/agents` output.

## OMTAE Tool Names (correct)

| Wrong (hallucinated) | Correct |
|----------------------|---------|
| `run_terminal_cmd` | `shell_exec` |
| `read_file` | `file_read` |
| `list_dir` | `file_list` |
| `{"name":"researcher"}` | `agent_send(agent_id=..., message=...)` |

## Recovery

When you catch yourself narrating instead of calling tools:

1. Stop mid-sentence
2. Issue one tool call in the same turn
3. Answer only from the tool result
