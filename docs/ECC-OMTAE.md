# ECC + OMTAE Integration

[ECC](https://github.com/affaan-m/ECC) (Everything Claude Code) is a harness toolkit for Cursor: skills, hooks, loop guards, and verification. OMTAE embeds ECC-inspired **runtime gates** plus adapted skills for the daemon on `:4200`.

## What OMTAE Enforces in Code

| Gate | Location | Behavior |
|------|----------|----------|
| **TOOL_REQUIRED** | `loop_guard.rs` + `agent_loop.rs` | Blocks factual claims (paths, URLs, status, lists, counts) without tool evidence |
| **Planning loop** | `loop_guard.rs` | Threshold 2 for repeated planning / meta-narration |
| **Tool-first prompt** | `prompt_builder.rs` | Injected for all agents with tools |
| **Research integrity** | `agent_loop.rs` | Disclaimer on unverified web lists |

Runtime gates run on every agent message — not only when ECC skills are installed.

## Install ECC Skills (OMTAE format)

From the openfang-core repo:

```bash
chmod +x scripts/install-ecc-skills.sh
./scripts/install-ecc-skills.sh
```

Skills land in `~/.omtae/skills/`:

- `verification-loop` — tool evidence checklist
- `search-first` — local → API → web order
- `research-ops` — fact / inference labels
- `no-fake-tools` — no JSON-in-text or phantom execution
- `tool-discipline` — per-turn tool contract

Restart the daemon after install:

```bash
systemctl --user restart omtae-daemon
# or kill + start if binary is locked
```

## Enable in Kernel Config

Add to `~/.omtae/config.toml` or your kernel TOML (e.g. `openfang-kernel.toml`):

```toml
[ecc]
enabled = true
```

When `enabled = true`, the kernel logs ECC integration at boot. Skills still load from `~/.omtae/skills/` via the normal skill registry — run `install-ecc-skills.sh` once.

## Cursor ECC + OMTAE Daemon Together

| Layer | Cursor ECC | OMTAE Daemon |
|-------|------------|--------------|
| IDE sessions | ECC skills, hooks, rules in Cursor | — |
| Agent OS (`:4200`) | — | Runtime gates + `~/.omtae/skills/ecc-*` |
| Shared discipline | verification-loop, search-first | Same concepts, OMTAE tool names |

**Recommended workflow**

1. Use ECC in Cursor for coding (hooks, `/verify`, skills).
2. Run `./scripts/install-ecc-skills.sh` so daemon agents get matching SKILL.md guidance.
3. Set `[ecc] enabled = true` in kernel config.
4. Deploy agent manifests with TOOL-FIRST rules (`agents/researcher/agent.toml`, etc.).
5. Live-test: POST to researcher with a brain question — response must show `shell_exec` / `file_list` evidence or `TOOL_REQUIRED` stop.

## Brain Vault Live Test

```bash
AGENT_ID=$(curl -s http://127.0.0.1:4200/api/agents | python3 -c "import sys,json; print([a for a in json.load(sys.stdin) if a.get('name')=='researcher'][0]['id'])")

curl -s -X POST "http://127.0.0.1:4200/api/agents/${AGENT_ID}/message" \
  -H "Content-Type: application/json" \
  -d '{"message": "What is in my Obsidian brain vault? Use tools — do not guess peer agents."}'
```

Pass: tool calls in the trace or response cites `/api/brain/*` / `file_list` output.  
Fail: peer-agent narration, "let me check" loops, or invented paths without tools.

## ecc-bridge Agent (optional)

`agents/ecc-bridge/agent.toml` is a thin chat agent with ECC tool-discipline prompts preloaded. Copy to `~/.omtae/agents/` and spawn when you want a dedicated verification-focused session.
