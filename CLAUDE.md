# OpenFang — Agent Instructions

## 🎯 What This Project Does

This is a **private fork** of OpenFang (Agent OS) integrated with **shipinbot** (video processing agent). The main purpose:

**Autonomous Telegram Video Processing Workflow:**
1. User sends video + images to Telegram
2. OpenFang receives media group, creates structured batch manifest
3. shipinfabu-hand agent processes batch:
   - Asks user to select source video (if multiple)
   - Downloads selected video only (saves bandwidth)
   - Removes watermarks using AI
   - Publishes to target platforms
4. All automated through OpenFang's agent system

## Project Structure

This repository contains **two integrated projects via Git submodule**:

### 1. OpenFang Core (`crates/*`) - Rust Framework
- **Purpose**: Agent Operating System, handles Telegram integration, message routing
- **Key files**:
  - `crates/openfang-channels/src/telegram.rs` - Telegram adapter with media group support
  - `crates/openfang-channels/src/telegram_media_batch.rs` - Structured media batch types
  - `crates/openfang-channels/src/bridge.rs` - Routes messages to agents, writes inbox manifests
- **Config**: `~/.openfang/config.toml`
- **API**: `http://127.0.0.1:4200`
- **Binary**: `target/release/openfang.exe`

### 2. shipinbot (`projects/shipinbot/`) - Python Video Agent
- **Purpose**: Video watermark removal and publishing automation
- **Location**: Work on `projects/shipinbot/` from this checkout by default. Treat any external checkout path as implementation detail, not the default operational path.
- **Key files**:
  - `projects/shipinbot/scripts/openfang_clean_publish_bridge.py` - Python CLI bridge (2733 lines)
    - `collect-telegram-batch` - Reads manifest, stages media files
    - `fetch-telegram-video` - Downloads user-selected video
    - `clean_publish_submit` - Submits watermark removal job
    - `clean_publish_poll` - Polls job status
  - `projects/shipinbot/openfang-hand/shipinfabu/HAND.toml` - Agent manifest (1759 lines)
  - `projects/shipinbot/openfang-hand/shipinfabu/README.md` - Agent documentation

### Integration Flow

```
Telegram → OpenFang Channels → Bridge → shipinfabu-hand Agent
                                  ↓
                    Inbox Manifest: ~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/<batch_key>.json
                                  ↓
                    Python Bridge: collect-telegram-batch → fetch-telegram-video → clean_publish_submit
```

## 🔄 Cross-Project Development

When modifying Telegram media handling, **both codebases must stay synchronized**:

### OpenFang Side (Rust)
- `crates/openfang-channels/src/telegram.rs` - Media group merging logic
- `crates/openfang-channels/src/telegram_media_batch.rs` - Batch structure definitions
- `crates/openfang-channels/src/bridge.rs` - Inbox manifest writing

### shipinbot Side (Python)
- `projects/shipinbot/scripts/openfang_clean_publish_bridge.py` - Manifest parsing and video download
- `projects/shipinbot/openfang-hand/shipinfabu/HAND.toml` - Agent prompt and tool definitions

**Important**: The submodule at `projects/shipinbot/` may share Git history with another checkout, but local development and deployment guidance in this fork should point at the current checkout first.

## Build & Verify Workflow
After every feature implementation, run ALL THREE checks:
```bash
cargo build --workspace --lib          # Must compile (use --lib if exe is locked)
cargo test --workspace                 # All tests must pass (currently 1744+)
cargo clippy --workspace --all-targets -- -D warnings  # Zero warnings
```

## MANDATORY: Live Integration Testing
**After implementing any new endpoint, feature, or wiring change, you MUST run live integration tests.** Unit tests alone are not enough — they can pass while the feature is actually dead code. Live tests catch:
- Missing route registrations in server.rs
- Config fields not being deserialized from TOML
- Type mismatches between kernel and API layers
- Endpoints that compile but return wrong/empty data

### How to Run Live Integration Tests

#### Step 1: Stop any running daemon
```bash
tasklist | grep -i openfang
taskkill //PID <pid> //F
# Wait 2-3 seconds for port to release
sleep 3
```

#### Step 2: Build fresh release binary
```bash
cargo build --release -p openfang-cli
```

#### Step 3: Start daemon with required API keys
```bash
GROQ_API_KEY=<key> target/release/openfang.exe start &
sleep 6  # Wait for full boot
curl -s http://127.0.0.1:4200/api/health  # Verify it's up
```
The daemon command is `start` (not `daemon`).

#### Step 4: Test every new endpoint
```bash
# GET endpoints — verify they return real data, not empty/null
curl -s http://127.0.0.1:4200/api/<new-endpoint>

# POST/PUT endpoints — send real payloads
curl -s -X POST http://127.0.0.1:4200/api/<endpoint> \
  -H "Content-Type: application/json" \
  -d '{"field": "value"}'

# Verify write endpoints persist — read back after writing
curl -s -X PUT http://127.0.0.1:4200/api/<endpoint> -d '...'
curl -s http://127.0.0.1:4200/api/<endpoint>  # Should reflect the update
```

#### Step 5: Test real LLM integration
```bash
# Get an agent ID
curl -s http://127.0.0.1:4200/api/agents | python3 -c "import sys,json; print(json.load(sys.stdin)[0]['id'])"

# Send a real message (triggers actual LLM call to Groq/OpenAI)
curl -s -X POST "http://127.0.0.1:4200/api/agents/<id>/message" \
  -H "Content-Type: application/json" \
  -d '{"message": "Say hello in 5 words."}'
```

#### Step 6: Verify side effects
After an LLM call, verify that any metering/cost/usage tracking updated:
```bash
curl -s http://127.0.0.1:4200/api/budget       # Cost should have increased
curl -s http://127.0.0.1:4200/api/budget/agents  # Per-agent spend should show
```

#### Step 7: Verify dashboard HTML
```bash
# Check that new UI components exist in the served HTML
curl -s http://127.0.0.1:4200/ | grep -c "newComponentName"
# Should return > 0
```

#### Step 8: Cleanup
```bash
tasklist | grep -i openfang
taskkill //PID <pid> //F
```

### Key API Endpoints for Testing
| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/health` | GET | Basic health check |
| `/api/agents` | GET | List all agents |
| `/api/agents/{id}/message` | POST | Send message (triggers LLM) |
| `/api/budget` | GET/PUT | Global budget status/update |
| `/api/budget/agents` | GET | Per-agent cost ranking |
| `/api/budget/agents/{id}` | GET | Single agent budget detail |
| `/api/network/status` | GET | OFP network status |
| `/api/peers` | GET | Connected OFP peers |
| `/api/a2a/agents` | GET | External A2A agents |
| `/api/a2a/discover` | POST | Discover A2A agent at URL |
| `/api/a2a/send` | POST | Send task to external A2A agent |
| `/api/a2a/tasks/{id}/status` | GET | Check external A2A task status |

## Architecture Notes
- **Don't touch `openfang-cli`** — user is actively building the interactive CLI
- `KernelHandle` trait avoids circular deps between runtime and kernel
- `AppState` in `server.rs` bridges kernel to API routes
- New routes must be registered in `server.rs` router AND implemented in `routes.rs`
- Dashboard is Alpine.js SPA in `static/index_body.html` — new tabs need both HTML and JS data/methods
- Config fields need: struct field + `#[serde(default)]` + Default impl entry + Serialize/Deserialize derives

## Telegram Media Batch Architecture

**Key Innovation**: Structured media batches instead of text degradation.

### Data Flow
1. **Telegram Adapter** (`telegram.rs:merge_media_group_updates`):
   - Collects media group items (500ms buffer)
   - Builds `TelegramMediaBatch` with structured metadata
   - Marks large videos (>100MB) as `needs_project_download` to avoid Local Bot API restarts
   - Writes batch to message metadata

2. **Bridge Layer** (`bridge.rs:dispatch_message`):
   - Detects `telegram_media_batch` in metadata
   - If target agent is `shipinfabu-hand`, writes manifest to inbox:
     `~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/<batch_key>.json`
   - Forwards short summary text to agent

3. **shipinfabu-hand Agent** (Python):
   - Reads inbox manifest
   - Calls `collect-telegram-batch --manifest <path>` to stage ready media
   - If multiple videos, asks user to select source
   - Calls `fetch-telegram-video --item-index <N>` to download selected video
   - Submits watermark removal job

### Batch Structure
```rust
pub struct TelegramMediaBatch {
    pub batch_key: String,           // Stable ID: "group_<chat_id>_<media_group_id>"
    pub chat_id: i64,
    pub message_id: i64,
    pub media_group_id: String,
    pub caption: Option<String>,
    pub items: Vec<TelegramMediaItem>,
}

pub struct TelegramMediaItem {
    pub kind: MediaItemKind,         // Image/Video/Document
    pub file_id: String,
    pub file_size: u64,
    pub status: MediaItemStatus,     // Ready/NeedsProjectDownload/SkippedSafeLimit
    pub local_path: Option<String>,  // If already downloaded
    pub download_hint: Option<TelegramDownloadHint>,
}
```

### Safety Thresholds
- **Local Bot API**: Skip `getFile` for videos >100MB (prevents server restart)
- **Official Bot API**: Skip for videos >20MB (hard limit)
- Videos marked `needs_project_download` are downloaded by shipinbot only if user selects them

## Common Gotchas
- `openfang.exe` may be locked if daemon is running — use `--lib` flag or kill daemon first
- `PeerRegistry` is `Option<PeerRegistry>` on kernel but `Option<Arc<PeerRegistry>>` on `AppState` — wrap with `.as_ref().map(|r| Arc::new(r.clone()))`
- Config fields added to `KernelConfig` struct MUST also be added to the `Default` impl or build fails
- `AgentLoopResult` field is `.response` not `.response_text`
- CLI command to start daemon is `start` not `daemon`
- On Windows: use `taskkill //PID <pid> //F` (double slashes in MSYS2/Git Bash)

## Telegram Group Configuration

**CRITICAL**: For bot to work in groups, you MUST configure BOTH layers:

### Layer 1: BotFather Settings (Telegram Server)
1. Open @BotFather in Telegram
2. Send `/mybots` → Select your bot → `Bot Settings` → `Group Privacy`
3. Choose **Turn off**

**Why**: With Group Privacy ON, Telegram only sends `/commands` to the bot, NOT @mentions.

### Layer 2: OpenFang Config (Application)
```toml
[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
allowed_users = ["user_id", "-group_id"]  # Group IDs are negative

[channels.telegram.overrides]
dm_policy = "respond"
group_policy = "mention_only"  # Only respond to @mentions
```

### Register Group as RBAC User
```toml
[[users]]
name = "group-123456789"
role = "admin"

[users.channel_bindings]
telegram = "-123456789"  # Negative number for groups
```

**Result**: Bot receives all messages (Telegram layer) but only responds to @mentions (OpenFang layer).

**See**:
- `docs/telegram-group-setup.md` for detailed setup guide
- `docs/telegram-mention-troubleshooting.md` for @mention issues and UTF-16 bug fixes

### Common Telegram Issues

**Problem**: Cannot send `@botname 消息` in groups (发送按钮消失)

**Solution**: Check BotFather → Bot Settings → Inline Mode → must be **OFF**
- Inline Mode ON causes `@botname ` to trigger inline search mode
- This blocks normal @mention messages
- Turn off Inline Mode and restart Telegram client

**Problem**: Bot crashes with "byte index is not a char boundary" panic

**Solution**: Update to commit 52ccc8a or later (UTF-16 bug fix)
- Old code directly used byte indices for Telegram entity offsets
- Telegram uses UTF-16 encoding, Rust uses UTF-8
- Fixed by using `telegram_entity_utf16_range_to_bytes` conversion function

**See full troubleshooting guide**: `docs/telegram-mention-troubleshooting.md`

## Git Submodule Management

**Important**: `projects/shipinbot/` is a Git submodule, not a copy.

### How It Works
- `projects/shipinbot/` is a Git submodule in this checkout
- use `projects/shipinbot/` as the default working path in docs, prompts, and commands
- OpenFang repo stores the submodule pointer (commit hash), not a vendored copy
- if an archived standalone `shipinbot` checkout exists elsewhere on disk, treat it as reference only

### Common Operations

**Update submodule to latest**:
```bash
cd projects/shipinbot
git pull origin main
cd ../..
git add projects/shipinbot
git commit -m "Update shipinbot submodule"
```

**Preferred commit flow**:
```bash
scripts/shipinbot-commit.sh "fix: <what changed>"
```

**Equivalent manual flow**:
```bash
cd projects/shipinbot
git add <files>
git commit -m "Update shipinbot code"
git push origin main
cd ../..
git add projects/shipinbot
git commit -m "Update submodule pointer"
git push origin main
```

**Clone this repo elsewhere**:
```bash
git clone --recurse-submodules <repo-url>
# Or if already cloned:
git submodule update --init --recursive
```

## Quick Reference: Key Files

### OpenFang (Rust)
- `crates/openfang-channels/src/telegram.rs:1076` - Video handling with safety thresholds
- `crates/openfang-channels/src/telegram.rs:merge_media_group_updates` - Media group merging
- `crates/openfang-channels/src/telegram_media_batch.rs` - Batch structure definitions
- `crates/openfang-channels/src/bridge.rs:dispatch_message` - Inbox manifest writing

### shipinbot (Python)
- `projects/shipinbot/scripts/openfang_clean_publish_bridge.py:2487` - `_collect_telegram_batch`
- `projects/shipinbot/scripts/openfang_clean_publish_bridge.py:2673` - `_fetch_telegram_video`
- `projects/shipinbot/scripts/openfang_clean_publish_bridge.py:2633` - `_telegram_resolve_download_url`
- `projects/shipinbot/openfang-hand/shipinfabu/HAND.toml` - Agent prompt and tools

### Documentation
- `PROJECTS.md` - Project structure and submodule management
- `docs/telegram-deployment-guide.md` - Telegram setup guide
- `docs/telegram-large-files.md` - Telegram large-file behavior and config
- `projects/shipinbot/docs/INDEX.md` - shipinbot operational docs index
