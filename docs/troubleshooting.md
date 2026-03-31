# Troubleshooting & FAQ

Common issues, diagnostics, and answers to frequently asked questions about OpenFang.

## Table of Contents

- [Quick Diagnostics](#quick-diagnostics)
- [Installation Issues](#installation-issues)
- [Configuration Issues](#configuration-issues)
- [LLM Provider Issues](#llm-provider-issues)
- [Channel Issues](#channel-issues)
- [Agent Issues](#agent-issues)
- [API Issues](#api-issues)
- [Desktop App Issues](#desktop-app-issues)
- [Performance](#performance)
- [FAQ](#faq)

---

## Quick Diagnostics

Run the built-in diagnostic tool:

```bash
openfang doctor
```

This checks:
- Configuration file exists and is valid TOML
- API keys are set in environment
- Database is accessible
- Daemon status (running or not)
- Port availability
- Tool dependencies (Python, signal-cli, etc.)

### Check Daemon Status

```bash
openfang status
```

### Check Health via API

```bash
curl http://127.0.0.1:4200/api/health
curl http://127.0.0.1:4200/api/health/detail  # Requires auth
```

### View Logs

OpenFang uses `tracing` for structured logging. Set the log level via environment:

```bash
RUST_LOG=info openfang start          # Default
RUST_LOG=debug openfang start         # Verbose
RUST_LOG=openfang=debug openfang start  # Only OpenFang debug, deps at info
```

---

## Installation Issues

### `cargo install` fails with compilation errors

**Cause**: Rust toolchain too old or missing system dependencies.

**Fix**:
```bash
rustup update stable
rustup default stable
rustc --version  # Need 1.75+
```

On Linux, you may also need:
```bash
# Debian/Ubuntu
sudo apt install pkg-config libssl-dev libsqlite3-dev

# Fedora
sudo dnf install openssl-devel sqlite-devel
```

### `openfang` command not found after install

**Fix**: Ensure `~/.cargo/bin` is in your PATH:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
# Add to ~/.bashrc or ~/.zshrc to persist
```

### Docker container won't start

**Common causes**:
- No API key provided: `docker run -e GROQ_API_KEY=... ghcr.io/RightNow-AI/openfang`
- Port already in use: change the port mapping `-p 3001:4200`
- Permission denied on volume mount: check directory permissions

---

## Configuration Issues

### "Config file not found"

**Fix**: Run `openfang init` to create the default config:
```bash
openfang init
```

This creates `~/.openfang/config.toml` with sensible defaults.

### "Missing API key" warnings on start

**Cause**: No LLM provider API key found in environment.

**Fix**: Set at least one provider key:
```bash
export GROQ_API_KEY="gsk_..."     # Groq (free tier available)
# OR
export ANTHROPIC_API_KEY="sk-ant-..."
# OR
export OPENAI_API_KEY="sk-..."
```

Add to your shell profile to persist across sessions.

### Config validation errors

Run validation manually:
```bash
openfang config show
```

Common issues:
- Malformed TOML syntax (use a TOML validator)
- Invalid port numbers (must be 1-65535)
- Missing required fields in channel configs

### "Port already in use"

**Fix**: Change the port in config or kill the existing process:
```bash
# Change API port
# In config.toml:
# [api]
# listen_addr = "127.0.0.1:3001"

# Or find and kill the process using the port
# Linux/macOS:
lsof -i :4200
# Windows:
netstat -aon | findstr :4200
```

---

## LLM Provider Issues

### "Authentication failed" / 401 errors

**Causes**:
- API key not set or incorrect
- API key expired or revoked
- Wrong env var name

**Fix**: Verify your key:
```bash
# Check if the env var is set
echo $GROQ_API_KEY

# Test the provider
curl http://127.0.0.1:4200/api/providers/groq/test -X POST
```

### "Rate limited" / 429 errors

**Cause**: Too many requests to the LLM provider.

**Fix**:
- The driver automatically retries with exponential backoff
- Reduce `max_llm_tokens_per_hour` in agent capabilities
- Switch to a provider with higher rate limits
- Use multiple providers with model routing

### Slow responses

**Possible causes**:
- Provider API latency (try Groq for fast inference)
- Large context window (use `/compact` to shrink session)
- Complex tool chains (check iteration count in response)

**Fix**: Use per-agent model overrides to use faster models for simple agents:
```toml
[model]
provider = "groq"
model = "llama-3.1-8b-instant"  # Fast, small model
```

### "Model not found"

**Fix**: Check available models:
```bash
curl http://127.0.0.1:4200/api/models
```

Or use an alias:
```toml
[model]
model = "llama"  # Alias for llama-3.3-70b-versatile
```

See the full alias list:
```bash
curl http://127.0.0.1:4200/api/models/aliases
```

### Ollama / local models not connecting

**Fix**: Ensure the local server is running:
```bash
# Ollama
ollama serve  # Default: http://localhost:11434

# vLLM
python -m vllm.entrypoints.openai.api_server --model ...

# LM Studio
# Start from the LM Studio UI, enable API server
```

---

## Channel Issues

### Telegram bot not responding

**Checklist**:
1. Bot token is correct: `echo $TELEGRAM_BOT_TOKEN`
2. Bot has been started (send `/start` in Telegram)
3. If `allowed_users` is set, your Telegram user ID is in the list
4. Check logs for "Telegram adapter" messages

### Discord bot offline

**Checklist**:
1. Bot token is correct
2. **Message Content Intent** is enabled in Discord Developer Portal
3. Bot has been invited to the server with correct permissions
4. Check Gateway connection in logs

### Slack bot not receiving messages

**Checklist**:
1. Both `SLACK_BOT_TOKEN` (xoxb-) and `SLACK_APP_TOKEN` (xapp-) are set
2. Socket Mode is enabled in the Slack app settings
3. Bot has been added to the channels it should monitor
4. Required scopes: `chat:write`, `app_mentions:read`, `im:history`, `im:read`, `im:write`

### Webhook-based channels (WhatsApp, LINE, Viber, etc.)

**Checklist**:
1. Your server is publicly accessible (or use a tunnel like ngrok)
2. Webhook URL is correctly configured in the platform dashboard
3. Webhook port is open and not blocked by firewall
4. Verify token matches between config and platform dashboard

### "Channel adapter failed to start"

**Common causes**:
- Missing or invalid token
- Port already in use (for webhook-based channels)
- Network connectivity issues

Check logs for the specific error:
```bash
RUST_LOG=openfang_channels=debug openfang start
```

---

## Agent Issues

### Agent stuck in a loop

**Cause**: The agent is repeatedly calling the same tool with the same parameters.

**Automatic protection**: OpenFang has a built-in loop guard:
- **Warn** at 3 identical tool calls
- **Block** at 5 identical tool calls
- **Circuit breaker** at 30 total blocked calls (stops the agent)

**Manual fix**: Cancel the agent's current run:
```bash
curl -X POST http://127.0.0.1:4200/api/agents/{id}/stop
```

Or via chat command: `/stop`

### Agent running out of context

**Cause**: Conversation history is too long for the model's context window.

**Fix**: Compact the session:
```bash
curl -X POST http://127.0.0.1:4200/api/agents/{id}/session/compact
```

Or via chat command: `/compact`

Auto-compaction is enabled by default when the session reaches the threshold (configurable in `[compaction]`).

### Agent not using tools

**Cause**: Tools not granted in the agent's capabilities.

**Fix**: Check the agent's manifest:
```toml
[capabilities]
tools = ["file_read", "web_fetch", "shell_exec"]  # Must list each tool
# OR
# tools = ["*"]  # Grant all tools (use with caution)
```

### "Permission denied" errors in agent responses

**Cause**: The agent is trying to use a tool or access a resource not in its capabilities.

**Fix**: Add the required capability to the agent manifest. Common ones:
- `tools = [...]` for tool access
- `network = ["*"]` for network access
- `memory_write = ["self.*"]` for memory writes
- `shell = ["*"]` for shell commands (use with caution)

### Agent spawning fails

**Check**:
1. TOML manifest is valid: `openfang agent spawn --dry-run manifest.toml`
2. LLM provider is configured and has a valid key
3. Model specified in manifest exists in the catalog

---

## API Issues

### 401 Unauthorized

**Cause**: API key required but not provided.

**Fix**: Include the Bearer token:
```bash
curl -H "Authorization: Bearer your-api-key" http://127.0.0.1:4200/api/agents
```

### 429 Too Many Requests

**Cause**: GCRA rate limiter triggered.

**Fix**: Wait for the `Retry-After` period, or increase rate limits in config:
```toml
[api]
rate_limit_per_second = 20  # Increase if needed
```

### CORS errors from browser

**Cause**: Trying to access API from a different origin.

**Fix**: Add your origin to CORS config:
```toml
[api]
cors_origins = ["http://localhost:5173", "https://your-app.com"]
```

### WebSocket disconnects

**Possible causes**:
- Idle timeout (send periodic pings)
- Network interruption (reconnect automatically)
- Agent crashed (check logs)

**Client-side fix**: Implement reconnection logic with exponential backoff.

### OpenAI-compatible API not working with my tool

**Checklist**:
1. Use `POST /v1/chat/completions` (not `/api/agents/{id}/message`)
2. Set the model to `openfang:agent-name` (e.g., `openfang:coder`)
3. Streaming: set `"stream": true` for SSE responses
4. Images: use `image_url` with `data:image/png;base64,...` format

---

## Desktop App Issues

### App won't start

**Checklist**:
1. Only one instance can run at a time (single-instance enforcement)
2. Check if the daemon is already running on the same ports
3. Try deleting `~/.openfang/daemon.json` and restarting

### White/blank screen in app

**Cause**: The embedded API server hasn't started yet.

**Fix**: Wait a few seconds. If persistent, check logs for server startup errors.

### System tray icon missing

**Platform-specific**:
- **Linux**: Requires a system tray (e.g., `libappindicator` on GNOME)
- **macOS**: Should work out of the box
- **Windows**: Check notification area settings, may need to show hidden icons

---

## Performance

### High memory usage

**Tips**:
- Reduce the number of concurrent agents
- Use session compaction for long-running agents
- Use smaller models (Llama 8B instead of 70B for simple tasks)
- Clear old sessions: `DELETE /api/sessions/{id}`

### Slow startup

**Normal startup**: <200ms for the kernel, ~1-2s with channel adapters.

If slower:
- Check database size (`~/.openfang/data/openfang.db`)
- Reduce the number of enabled channels
- Check network connectivity (MCP server connections happen at boot)

### High CPU usage

**Possible causes**:
- WASM sandbox execution (fuel-limited, should self-terminate)
- Multiple agents running simultaneously
- Channel adapters reconnecting (exponential backoff)

---

## FAQ

### How do I switch the default LLM provider?

Edit `~/.openfang/config.toml`:
```toml
[default_model]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"
```

### Can I use multiple providers at the same time?

Yes. Each agent can use a different provider via its manifest `[model]` section. The kernel creates a dedicated driver per unique provider configuration.

### How do I add a new channel?

1. Add the channel config to `~/.openfang/config.toml` under `[channels]`
2. Set the required environment variables (tokens, secrets)
3. Restart the daemon

### How do I update OpenFang?

```bash
# From source
cd openfang && git pull && cargo install --path crates/openfang-cli

# Docker
docker pull ghcr.io/RightNow-AI/openfang:latest
```

### Can agents talk to each other?

Yes. Agents can use the `agent_send`, `agent_spawn`, `agent_find`, and `agent_list` tools to communicate. The orchestrator template is specifically designed for multi-agent delegation.

### Is my data sent to the cloud?

Only LLM API calls go to the provider's servers. All agent data, memory, sessions, and configuration are stored locally in SQLite (`~/.openfang/data/openfang.db`). The OFP wire protocol uses HMAC-SHA256 mutual authentication for P2P communication.

### How do I back up my data?

Back up these files:
- `~/.openfang/config.toml` (configuration)
- `~/.openfang/data/openfang.db` (all agent data, memory, sessions)
- `~/.openfang/skills/` (installed skills)

### How do I reset everything?

```bash
rm -rf ~/.openfang
openfang init  # Start fresh
```

### Can I run OpenFang without an internet connection?

Yes, if you use a local LLM provider:
- **Ollama**: `ollama serve` + `ollama pull llama3.2`
- **vLLM**: Self-hosted model server
- **LM Studio**: GUI-based local model runner

Set the provider in config:
```toml
[default_model]
provider = "ollama"
model = "llama3.2"
```

### What's the difference between OpenFang and OpenClaw?

| Aspect | OpenFang | OpenClaw |
|--------|----------|----------|
| Language | Rust | Python |
| Channels | 40 | 38 |
| Skills | 60 | 57 |
| Providers | 20 | 3 |
| Security | 16 systems | Config-based |
| Binary size | ~30 MB | ~200 MB |
| Startup | <200 ms | ~3 s |

OpenFang can import OpenClaw configs: `openfang migrate --from openclaw`

### How do I report a bug or request a feature?

- Bugs: Open an issue on GitHub
- Security: See [SECURITY.md](../SECURITY.md) for responsible disclosure
- Features: Open a GitHub discussion or PR

### What are the system requirements?

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| RAM | 128 MB | 512 MB |
| Disk | 50 MB (binary) | 500 MB (with data) |
| CPU | Any x86_64/ARM64 | 2+ cores |
| OS | Linux, macOS, Windows | Any |
| Rust | 1.75+ (build only) | Latest stable |

### How do I enable debug logging for a specific crate?

```bash
RUST_LOG=openfang_runtime=debug,openfang_channels=info openfang start
```

### Can I use OpenFang as a library?

Yes. Each crate is independently usable:
```toml
[dependencies]
openfang-runtime = { path = "crates/openfang-runtime" }
openfang-memory = { path = "crates/openfang-memory" }
```

The `openfang-kernel` crate assembles everything, but you can use individual crates for custom integrations.

---

## Common Community Questions

### How do I update OpenFang?

Re-run the install script to get the latest release:
```bash
curl -fsSL https://openfang.sh/install | sh
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
  ghcr.io/rightnow-ai/openfang:latest
```

### How do I protect the dashboard with a password?

OpenFang has built-in dashboard authentication. Enable it in `~/.openfang/config.toml`:

```toml
[auth]
enabled = true
username = "admin"
password_hash = "$argon2id$..."  # see below
```

Generate the password hash:

```bash
openfang auth hash-password
```

Paste the output into the `password_hash` field and restart the daemon.

For public-facing deployments, you should also place a reverse proxy (Caddy, nginx) in front for TLS termination.

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

Add to `~/.openfang/config.toml`:
```toml
[claude_code]
skip_permissions = true
```
Then restart the daemon.

### Trader hand shell permissions

The trader hand needs shell access for executing trading scripts. In your agent's `agent.toml`:
```toml
[capabilities]
shell = ["python *", "node *"]
```

### OpenRouter free models don't work

OpenRouter free models have strict rate limits and may return empty responses. Use a paid model or try a different free provider like Groq (`GROQ_API_KEY`).
