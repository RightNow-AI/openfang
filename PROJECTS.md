# Projects Structure

This repository contains two integrated projects:

## 1. OpenFang Core (`crates/*`)

The Agent Operating System framework written in Rust.

**Key directories:**
- `crates/openfang-kernel` - Orchestration, workflows, metering
- `crates/openfang-runtime` - Agent loop, tools, WASM sandbox
- `crates/openfang-channels` - 40 messaging adapters including Telegram
- `crates/openfang-api` - REST/WS/SSE endpoints
- `crates/openfang-hands` - Autonomous agent capabilities

**Telegram Media Handling:**
- `crates/openfang-channels/src/telegram.rs` - Telegram adapter with media group support
- `crates/openfang-channels/src/telegram_media_batch.rs` - Structured media batch types
- `crates/openfang-channels/src/bridge.rs` - Channel-to-agent message routing

## 2. shipinbot (`projects/shipinbot/`)

Video processing agent implementation demonstrating OpenFang integration.

**Key files:**
- `projects/shipinbot/scripts/openfang_clean_publish_bridge.py` - Python bridge for video workflows
- `projects/shipinbot/openfang-hand/shipinfabu/HAND.toml` - Agent manifest (1759 lines)
- `projects/shipinbot/openfang-hand/shipinfabu/README.md` - Agent documentation

**Integration Points:**
- Receives structured Telegram media batches from OpenFang
- Processes video files with selective download strategy
- Uses OpenFang inbox manifest pattern for async batch handling

## Submodule Management

shipinbot is included as a Git submodule:

```bash
# Clone with submodules
git clone --recurse-submodules <repo-url>

# Initialize submodules in existing clone
git submodule update --init --recursive

# Update shipinbot to latest
git submodule update --remote projects/shipinbot

# Work on shipinbot changes
cd projects/shipinbot
git checkout -b feature-branch
# make changes, commit, push
cd ../..
git add projects/shipinbot
git commit -m "Update shipinbot submodule"
```

## Cross-Project Development

When modifying Telegram media handling:

1. **OpenFang side** - Update channel adapter and types:
   - `crates/openfang-channels/src/telegram.rs`
   - `crates/openfang-channels/src/telegram_media_batch.rs`
   - Run: `cargo test --workspace`

2. **shipinbot side** - Update bridge to match new structure:
   - `projects/shipinbot/scripts/openfang_clean_publish_bridge.py`
   - Test: `python3 projects/shipinbot/scripts/openfang_clean_publish_bridge.py --help`

3. **Integration test** - Verify end-to-end:
   - Start OpenFang daemon with Telegram configured
   - Send test media group
   - Verify manifest written to `~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/`
   - Run `collect-telegram-batch` and `fetch-telegram-video` commands

## Documentation

- [Telegram Deployment Guide](docs/telegram-deployment-guide.md)
- [Telegram Large Files](docs/telegram-large-files.md)
- [shipinbot Docs Index](projects/shipinbot/docs/INDEX.md)
