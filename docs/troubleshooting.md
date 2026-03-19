# Troubleshooting

This guide focuses on failures that are common in the current repository and deployment assets.

## 1. Quick Triage Order

Start with these commands before guessing:

```bash
openfang doctor
openfang status
curl -s http://127.0.0.1:4200/api/health
```

If auth is enabled:

```bash
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

Then inspect deployment-specific logs:

- local foreground process stderr
- `docker compose logs`
- `journalctl -u openfang`

## 2. Daemon Will Not Start

### Symptoms

- `openfang start` exits early
- the API never appears on port `4200`
- `openfang status` cannot find a daemon

### Checks

```bash
openfang doctor
openfang config show
RUST_LOG=debug openfang start
```

OpenFang now fails closed on malformed `config.toml`, broken `include` chains, and invalid boot-time settings such as an empty `network.shared_secret` when `network_enabled = true` or an unusable `auth.password_hash`. If startup exits after a config change, fix the config instead of expecting the daemon to fall back to defaults.

### Common causes

- malformed `config.toml`
- invalid `auth.password_hash` or missing `network.shared_secret` for an enabled network listener
- no reachable provider credentials
- bind/auth conflict
- another process already using the port

## 3. Refusing to Expose the API on 0.0.0.0

### Symptom

Startup fails with a refusal to expose the API off-loopback.

### Cause

The server validates that non-loopback bind addresses must have auth enabled, and it rejects obvious placeholder API keys on public listeners.

### Fix

Either:

```bash
export OPENFANG_API_KEY="$(openssl rand -hex 32)"
```

or bind locally:

```toml
api_listen = "127.0.0.1:4200"
```

## 4. CLI Says a Daemon Exists but It Is Not Reachable

### Symptom

- `openfang status` points to an old daemon
- commands fail even though no real daemon is serving traffic

### Cause

`~/.openfang/daemon.json` is stale.

### Fix

```bash
openfang doctor --repair
```

Or remove `daemon.json` manually after confirming the daemon is dead.

## 5. `/api/health/detail` Returns 401 or 403

### Cause

`/api/health/detail` is a protected operational endpoint when auth is enabled.

### Fix

Use:

```bash
curl -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

For anonymous liveness checks, use `/api/health` instead.

If `/api/health/detail` returns HTTP 200 but `status = "degraded"`, treat the node as not ready for production traffic yet. Common causes now include config warnings, restore warnings, missing default-provider auth, shutdown-in-progress, or recorded supervisor panics.

## 6. Channel Config Is Ignored

### Symptom

You added channel settings, but the adapter does not start.

### Common cause

Using the wrong TOML shape, for example:

```toml
[telegram]
```

instead of:

```toml
[channels.telegram]
```

### Fix

Move channel config under `channels.<name>`.

## 7. Wrong Field Used for HTTP Bind

### Symptom

You changed `[network]` and expected the HTTP API to move.

### Cause

`[network]` config is for OFP peer networking. The HTTP API uses top-level `api_listen`.

### Fix

Set:

```toml
api_listen = "127.0.0.1:4200"
```

and leave `[network].listen_addresses` for peer networking only.

## 8. Docker Container Is Running but the Host Cannot Reach It

### Checks

```bash
docker compose ps
docker compose logs --tail=200 openfang
```

### Common causes

- `OPENFANG_LISTEN` is still loopback inside the container
- missing provider key
- server refused non-loopback bind without auth

### Fix

Use:

```bash
OPENFANG_LISTEN=0.0.0.0:4200
OPENFANG_API_KEY=$(openssl rand -hex 32)
```

in Compose or the container environment.

## 9. Config Changes Did Not Apply Live

### Cause

OpenFang has config reload support, but not every field reloads hot.

### Fix

Try:

```bash
curl -X POST -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/config/reload
```

Restart the daemon after changing:

- API listen or auth behavior
- network settings
- memory backend settings
- home/data directory settings
- vault settings

Also note:

- `/api/config/reload` now separates `hot_actions_applied` from `hot_actions_pending_follow_up`
- if changes are only detected but not yet applied, treat the node as requiring operational follow-up before considering the config live

## 10. `openfang logs` Does Not Show Daemon Output

### Cause

`openfang logs` tails `~/.openfang/tui.log`, which is only meaningful for TUI logging.

### Fix

Use the real runtime log surface for your deployment:

- foreground stderr for local dev
- `docker compose logs`
- `journalctl -u openfang`

Treat `/api/logs/stream` as an audit stream, not as daemon stderr.

## 11. Telegram Bot Does Not Respond

### Checks

```bash
echo "$TELEGRAM_BOT_TOKEN"
openfang doctor
```

Verify:

- `[channels.telegram]` exists
- `bot_token_env` points to the right env var
- allowed-user filters are not blocking you
- logs show the Telegram adapter actually starting

## 12. Telegram Large File Downloads Still Fail

### Checks

- `download_enabled = true`
- `use_local_api = true` when relying on Local Bot API
- `api_url` or `local_api_port` is correct
- `telegram_api_id` and `telegram_api_hash_env` are present when auto-starting Local Bot API

See [telegram-large-files.md](telegram-large-files.md) for the full Local Bot API path.

## 13. Provider Auth Fails Even Though Config Looks Correct

### Cause

The config usually stores env-var names, not secret values.

### Fix

If config says:

```toml
api_key_env = "GROQ_API_KEY"
```

then the process environment, `~/.openfang/secrets.env`, or `~/.openfang/.env` must actually contain `GROQ_API_KEY`.
If the same key is present in both runtime files, `secrets.env` takes precedence.

Use:

```bash
openfang doctor
openfang config show
```

to confirm both config and secret presence.

## 14. Permission Errors Under `~/.openfang`

### Symptoms

- database cannot open
- workspaces cannot be created
- `daemon.json` cannot be written

### Fix

Verify that the runtime user owns the runtime home:

```bash
ls -la ~/.openfang
```

For server installs, also verify `/var/lib/openfang` ownership and the systemd `User=` setting.

## 15. Route Compiles but Endpoint Is Missing

### Cause

The handler exists, but the router was not updated.

### Fix

For API changes, always verify both:

- `crates/openfang-api/src/routes.rs`
- `crates/openfang-api/src/server.rs`

Then run a live daemon and hit the endpoint directly.

## 16. What to Inspect First

| Area | File |
| --- | --- |
| Config schema | `crates/openfang-types/src/config.rs` |
| Config loading and include behavior | `crates/openfang-kernel/src/config.rs` |
| Config reload | `crates/openfang-kernel/src/config_reload.rs` |
| Boot sequence | `crates/openfang-kernel/src/kernel.rs` |
| Router assembly | `crates/openfang-api/src/server.rs` |
| Runtime request handling | `crates/openfang-runtime/` |
| Channel ingress | `crates/openfang-channels/` |

## 17. Telegram Bot Connected but Not Receiving Messages

### Symptoms

- Logs show `Telegram bot @username connected`
- Bot appears online in Telegram
- Sending messages to the bot gets no response
- Dashboard shows agent as ready but no activity

### Diagnosis Steps

```bash
# 1. Check daemon readiness
curl -s http://127.0.0.1:4200/api/health
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail

# 2. Check runtime status
openfang status
```

### Common Cause

**Environment variables not loaded at startup**. The startup script may use variable substitution like:

```bash
export TELEGRAM_BOT_TOKEN="${TELEGRAM_BOT_TOKEN}"
```

If `TELEGRAM_BOT_TOKEN` is not set in the parent shell, this exports an **empty string**, not an error. OpenFang starts successfully but cannot authenticate with Telegram API.

### Fix

**Option 1: Source environment file before starting**

```bash
# Start with environment loaded
source .env.telegram
target/release/openfang stop
target/release/openfang start
```

**Option 2: Export variables in shell profile**

Add to `~/.zshrc` or `~/.bashrc`:

```bash
export TELEGRAM_BOT_TOKEN="your_token_here"
export TELEGRAM_API_HASH="your_hash_here"
```

Then restart shell and daemon.

**Option 3: Use systemd environment file**

For production deployments, use systemd `EnvironmentFile=`:

```ini
[Service]
EnvironmentFile=/path/to/.env.telegram
ExecStart=/path/to/openfang start
```

### Verification

After restart, confirm the daemon reports healthy channel/provider auth:

```bash
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

Check logs for successful connection:

```bash
# systemd
sudo journalctl -u openfang -f | grep -i telegram

# Docker / Compose
docker compose logs -f openfang | grep -i telegram

# local foreground run
# inspect the terminal running `target/release/openfang start`

# Should see: "Telegram bot @username connected"
# Should see: "Telegram polling loop started"
```

### Related Issues

- Local Bot API not starting: Check `TELEGRAM_API_HASH` is also loaded
- Port 8081 not listening: Verify `use_local_api = true` and `auto_start_local_api = true` in config

## 18. Telegram Bot Shows "正在输入" but No Response

### Symptoms

- User sends message to Telegram bot
- Bot immediately shows emoji reactions (⏳ → 🤔)
- Bot shows "typing..." status
- Long delay (minutes) with no response
- Eventually returns: "The AI service is temporarily overloaded, please try again shortly."

### Root Cause

**HTTP client timeout issue** (60% fixed):
- OpenAI driver (used by NVIDIA API) had no timeout configured
- Requests would wait indefinitely for server response
- NVIDIA API returns 504 Gateway Timeout after internal limit
- User sees typing indicator but backend is blocked waiting

### Fix Applied

Modified `crates/openfang-runtime/src/drivers/openai.rs:30`:

```rust
// Added 120-second timeout
client: reqwest::Client::builder()
    .user_agent(crate::USER_AGENT)
    .timeout(std::time::Duration::from_secs(120))
    .build()
    .unwrap_or_default(),
```

### Why 120 Seconds

- Large models (397B parameters) need longer inference time
- Matches tool execution timeout (`TOOL_TIMEOUT_SECS = 120`)
- Prevents indefinite blocking while allowing completion
- Retry mechanism kicks in after timeout

### Remaining Issues (40%)

1. **NVIDIA API server-side timeout**: Even with client timeout, NVIDIA may return 504 before 120s
2. **Long conversation history**: 20+ messages increase token usage and inference time
3. **User experience**: No progress updates during long waits

### Detailed Documentation

See [telegram-response-timeout-issue.md](telegram-response-timeout-issue.md) for:
- Complete code flow analysis
- Retry mechanism details
- Future optimization plans
- Diagnostic commands

### Quick Diagnosis

```bash
# Monitor timeout errors
sudo journalctl -u openfang -f | grep -E "504|timeout|overload|LLM error"

# Test agent response time
AGENT_ID=$(curl -s http://127.0.0.1:4200/api/agents | python3 -c "import sys,json; print(json.load(sys.stdin)[0]['id'])")
time curl -s -X POST "http://127.0.0.1:4200/api/agents/$AGENT_ID/message" \
  -H "Content-Type: application/json" \
  -d '{"message":"test"}'
```

## 19. Escalation Rule

If a problem crosses OpenFang and `projects/shipinbot/`, treat it as a contract issue, not a one-sided bug. Check both sides before changing schema, batch files, or runtime assumptions.

---

## Common Community Questions

### How do I update OpenFang?

Install the release you want from this fork (recommended source of truth):
```bash
# Replace <release-tag> with the version you want, for example v0.4.9
curl -fsSL https://raw.githubusercontent.com/tytsxai/openfang-upstream-fork/v<release-tag>/scripts/install.sh | sh
```
Or build from source:
```bash
git pull origin main
cargo build --release -p openfang-cli
```

### How do I run OpenFang in Docker?

```bash
docker run -d --name openfang \
  -e GROQ_API_KEY=your_key_here \
  -p 4200:4200 \
  ghcr.io/tytsxai/openfang-upstream-fork:latest
```
If the GHCR image is not publicly available yet, run `docker compose up --build` from the repository root.

### How do I protect the dashboard with a password?

OpenFang doesn't have built-in login. Use a reverse proxy with basic auth:

**Caddy example:**
```
ai.yourdomain.com {
    basicauth {
        username $2a$14$YOUR_HASHED_PASSWORD
    }
    reverse_proxy localhost:4200
}
```

Generate a password hash: `caddy hash-password`

### How do I configure the embedding model for memory?

In `~/.openfang/config.toml`:
```toml
[memory]
embedding_provider = "openai"     # or "ollama", "gemini"
embedding_model = "text-embedding-3-small"
embedding_api_key_env = "OPENAI_API_KEY"
```

For local Ollama embeddings:
```toml
[memory]
embedding_provider = "ollama"
embedding_model = "nomic-embed-text"
```

### Email channel responds to ALL emails — how do I restrict it?

Add `allowed_senders` to your email config:
```toml
[channels.email]
allowed_senders = ["me@example.com", "boss@company.com"]
```
Empty list = responds to everyone. Always set this to avoid auto-replying to spam.

### How do I use Z.AI / GLM-5?

```toml
[default_model]
provider = "zai"
model = "glm-5-20250605"
api_key_env = "ZHIPU_API_KEY"
```

### How do I add Kimi 2.5?

Kimi models are built-in. Use alias `kimi` or the full model ID:
```toml
[default_model]
provider = "moonshot"
model = "kimi-k2.5"
api_key_env = "MOONSHOT_API_KEY"
```

### Can I use multiple Telegram bots?

Not yet — each channel type currently supports one bot. Multi-bot routing is tracked as a feature request (#586). As a workaround, run multiple OpenFang instances on different ports with different configs.

### Claude Code integration shows errors

OpenFang already launches the Claude Code provider with `--dangerously-skip-permissions` enabled for non-interactive daemon use. If Claude Code still errors, accept permissions once in your shell:
```bash
claude --dangerously-skip-permissions
```
Then restart the daemon. No extra top-level `config.toml` section is required for this.

### Trader hand shell permissions

The trader hand needs shell access for executing trading scripts. In your agent's `agent.toml`:
```toml
[capabilities]
shell = ["python *", "node *"]
```

### OpenRouter free models don't work

OpenRouter free models have strict rate limits and may return empty responses. Use a paid model or try a different free provider like Groq (`GROQ_API_KEY`).
