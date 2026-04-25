# OpenFang deployment assets

Operator-facing files for installing the OpenFang Agent OS as a long-running daemon.

## Choose your install path

| Platform | Mode | File |
|---|---|---|
| macOS | Per-user LaunchAgent | [`launchd/io.openfang.plist`](launchd/io.openfang.plist) |
| Linux | Per-user systemd service | [`systemd/user/openfang.service`](systemd/user/openfang.service) |
| Linux | System-wide systemd service (dedicated `openfang` user) | [`openfang.service`](openfang.service) |
| Homebrew tap | Formula skeleton (publish to `<org>/homebrew-openfang`) | [`brew/openfang.rb`](brew/openfang.rb) |

For day-to-day operation regardless of install path:

| Tool | File |
|---|---|
| Warp workflows (start, tail logs, health, pinboard, agents, budget) | [`warp/openfang.yaml`](warp/openfang.yaml) |
| Bash tab-completion | [`shell/openfang.bash`](shell/openfang.bash) |
| Zsh tab-completion | [`shell/openfang.zsh`](shell/openfang.zsh) |

Each file's header comment carries its own one-shot install + uninstall + tail-logs snippet. This README only steers you to the right one.

## What survives across reboots

- **launchd** with `RunAtLoad=true`: agent restarts at login.
- **systemd user** with `loginctl enable-linger`: agent persists across logout. Without lingering, it stops at logout.
- **systemd system-wide**: agent runs as the `openfang` user, persists indefinitely.

## Where the daemon expects to find things

Regardless of install path, the daemon reads from the working directory's `~/.openfang/`:

```
~/.openfang/
├── config.toml          # main config (API keys via env or set-key subcommand)
├── env                  # 0600 perms, sourced for API keys (optional)
├── agents/              # per-agent SOUL.md / IDENTITY.md / context.md
├── quarantine/          # Phase 5 isolated content (XDG_DATA_HOME if set)
├── logs/
└── sessions.db          # SQLite store
```

API binds to `127.0.0.1:4200` by default. To change, set `[server] bind_address` in config.toml. Non-loopback binds require an explicit `[server] api_key` for auth (Phase 5.10 patched the loopback bypass; see `SECURITY.md`).

## Verifying the install

```bash
# 1. Daemon is alive
curl -s http://127.0.0.1:4200/api/health | jq .

# 2. Authenticated detail (post-Phase 6 wiring will enrich with warm-state)
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail | jq .

# 3. Agent list — expect at least the bundled `assistant`
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/agents | jq '.[].name'

# 4. Send a message to an agent
curl -s -X POST \
  -H "Authorization: Bearer $OPENFANG_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"message":"Say hello in 5 words."}' \
  http://127.0.0.1:4200/api/agents/<id>/message
```

If the daemon won't start under launchd / systemd, run it manually with `openfang start` first to get unfiltered stderr — the unit configurations swallow stdout/stderr to log files, which is exactly what you want at runtime but exactly what you don't want at install time.

## Hardening notes (v0.6.1)

The hardening pass shipping in v0.6.1 lays primitive layers that the operator-facing flow benefits from once the API plumbing lands:

- **Boot-warm gating** (Phase 6.1) — `/api/health` will return `503 {state: "warming", pending: [...]}` until every required subsystem (`mempalace`, `obsidian`, `skills`, ...) has warmed. NonCritical subsystems flip to `degraded` after a 30s soft timeout instead of blocking. Today the primitive lives at `openfang_runtime::boot_warm`; AppState wiring is a follow-up.
- **Universal untrusted-content channel** (Phase 5.1) — every external input (web fetch, MCP tool result, channel message, file read) gets SHA256-delimited and jailbreak-marker-stripped before reaching the model.
- **Triage pipeline** (Phases 5.2–5.4) — heuristic + Moonlock scanners → cyber-agent classifier → safe / questionable / malicious routing → pinboard for questionable items. Configure the cyber-intel vault per [`docs/security/cyber-intel-vault-setup.md`](../docs/security/cyber-intel-vault-setup.md).
- **Mempalace required** — the daemon's boot path verifies the mempalace MCP is reachable; if not, boot fails with a pointer to `INTEGRATION_PLAN.md`. See [`docs/hardening-status.md`](../docs/hardening-status.md) for the current wiring state.

## Removing OpenFang

Each install snippet has a matching uninstall block in its header. The data dir under `~/.openfang/` is left in place so re-installing later restores agent state — `rm -rf ~/.openfang` if you want a clean slate.
