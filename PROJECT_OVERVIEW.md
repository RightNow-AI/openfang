# Project Overview - OpenFang + shipinbot Integration

## 🎯 Purpose

**Autonomous Telegram video processing workflow** - from media group reception to AI watermark removal and publishing.

## 📁 Repository Structure

```
openfang-upstream-fork/
├── crates/                          # OpenFang Core (Rust)
│   ├── openfang-channels/           # Telegram adapter, media batch handling
│   │   ├── src/telegram.rs          # Media group merging (line 1076: video handling)
│   │   ├── src/telegram_media_batch.rs  # Batch structure definitions
│   │   └── src/bridge.rs            # Inbox manifest writing
│   ├── openfang-kernel/             # Agent orchestration
│   ├── openfang-runtime/            # Agent loop, tools
│   └── openfang-api/                # REST/WS/SSE endpoints
│
├── projects/
│   └── shipinbot/                   # Git submodule → /Users/xiaomo/Desktop/shipinbot
│       ├── scripts/
│       │   └── openfang_clean_publish_bridge.py  # Python CLI bridge (2733 lines)
│       │       ├── collect-telegram-batch (line 2487)
│       │       ├── fetch-telegram-video (line 2673)
│       │       └── _telegram_resolve_download_url (line 2633)
│       └── openfang-hand/shipinfabu/
│           ├── HAND.toml            # Agent manifest (1759 lines)
│           └── README.md            # Agent documentation
│
├── docs/                            # Documentation
│   ├── telegram-deployment-guide.md
│   ├── telegram-shipinbot-integration.md
│   └── ...
│
├── CLAUDE.md                        # AI development guide
├── PROJECTS.md                      # Submodule management
├── TELEGRAM_MEDIA_BATCH_IMPLEMENTATION.md  # Implementation summary
└── SHIPINBOT_INTEGRATION_GUIDE.md  # Integration guide
```

## 🔄 Data Flow

```
1. User sends to Telegram
   └─ 1 video (150MB) + 9 images

2. OpenFang Channels Layer (telegram.rs)
   └─ Merges into TelegramMediaBatch
      ├─ Video: status=NeedsProjectDownload (>100MB, skip getFile)
      └─ Images: status=Ready, local_path="/tmp/img*.jpg"

3. Bridge Layer (bridge.rs)
   └─ Writes manifest to:
      ~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/group_123_abc.json

4. shipinfabu-hand Agent
   └─ Reads manifest, calls Python bridge:
      ├─ collect-telegram-batch --manifest <path>
      │  └─ Stages ready media, lists pending videos
      ├─ (User selects video #1)
      └─ fetch-telegram-video --manifest <path> --item-index 1
         └─ Downloads selected video only

5. Python Bridge
   └─ clean_publish_submit
      └─ AI watermark removal → Publish
```

## 🔑 Key Concepts

### Structured Media Batches

**Before**: Telegram media groups degraded to text links:
```
[Photo: https://...]
[Video: https://...]
[Photo: https://...]
```

**After**: Structured batch with metadata:
```json
{
  "batch_key": "group_123_abc",
  "items": [
    {
      "kind": "video",
      "file_id": "BAACAgEAAxkBAAI...",
      "file_size": 157286400,
      "status": "needs_project_download",
      "download_hint": {
        "strategy": "telegram_bot_api_file",
        "file_id": "BAACAgEAAxkBAAI...",
        "api_base_url": "http://127.0.0.1:8081",
        "use_local_api": true
      }
    },
    {
      "kind": "image",
      "file_id": "AgACAgEAAxkBAAI...",
      "file_size": 89234,
      "status": "ready",
      "local_path": "/tmp/telegram_downloads/img1.jpg"
    }
  ]
}
```

### Selective Download Strategy

**Problem**: Downloading all videos wastes bandwidth and may crash Local Bot API Server.

**Solution**:
1. OpenFang marks large videos (>100MB) as `needs_project_download`
2. shipinbot stages ready media (images) immediately
3. Agent asks user to select source video
4. Only selected video is downloaded

**Bandwidth savings**: 1 video downloaded instead of 10 videos.

### Safety Thresholds

- **Local Bot API**: Skip `getFile` for videos >100MB (prevents server restart)
- **Official Bot API**: Skip for videos >20MB (hard limit)
- Videos marked `needs_project_download` include `download_hint` for project-side downloaders

## 🛠️ Development Workflow

### Modifying Telegram Media Handling

**Both codebases must stay synchronized:**

1. **OpenFang side** (Rust):
   ```bash
   # Edit batch structure
   vim crates/openfang-channels/src/telegram_media_batch.rs

   # Edit merging logic
   vim crates/openfang-channels/src/telegram.rs

   # Test
   cargo test --workspace
   cargo clippy --workspace --all-targets -- -D warnings
   ```

2. **shipinbot side** (Python):
   ```bash
   # Edit bridge to match new structure
   vim projects/shipinbot/scripts/openfang_clean_publish_bridge.py

   # Test
   cd projects/shipinbot
   python3 -m pytest tests/
   ```

3. **Integration test**:
   ```bash
   # Start OpenFang daemon
   cargo build --release -p openfang-cli
   GROQ_API_KEY=<key> target/release/openfang.exe start

   # Send test media group to Telegram
   # Verify manifest written to inbox
   ls ~/.openfang/workspaces/shipinfabu-hand/inbox/telegram/

   # Test Python bridge
   python3 projects/shipinbot/scripts/openfang_clean_publish_bridge.py \
     collect-telegram-batch --manifest <path>
   ```

### Git Submodule Operations

**Commit changes in shipinbot**:
```bash
cd projects/shipinbot
git add scripts/openfang_clean_publish_bridge.py
git commit -m "Update bridge to handle new field"
git push origin main

cd ../..
git add projects/shipinbot
git commit -m "Update shipinbot submodule"
git push origin main
```

**Update submodule to latest**:
```bash
cd projects/shipinbot
git pull origin main
cd ../..
git add projects/shipinbot
git commit -m "Sync shipinbot to latest"
```

## 📚 Documentation

- **CLAUDE.md** - AI development guide (architecture, gotchas, quick reference)
- **PROJECTS.md** - Project structure and submodule management
- **TELEGRAM_MEDIA_BATCH_IMPLEMENTATION.md** - OpenFang implementation details
- **SHIPINBOT_INTEGRATION_GUIDE.md** - shipinbot integration guide
- **docs/telegram-deployment-guide.md** - Telegram setup and troubleshooting

## 🧪 Testing

```bash
# OpenFang tests
cargo test --workspace                              # 1747+ tests
cargo clippy --workspace --all-targets -- -D warnings  # Zero warnings

# shipinbot tests
cd projects/shipinbot
python3 -m pytest tests/

# Integration test
# (See "Development Workflow" section above)
```

## 🔗 Key File Locations

### OpenFang (Rust)
- `crates/openfang-channels/src/telegram.rs:1076` - Video handling with safety thresholds
- `crates/openfang-channels/src/telegram.rs:merge_media_group_updates` - Media group merging
- `crates/openfang-channels/src/telegram_media_batch.rs` - Batch structure
- `crates/openfang-channels/src/bridge.rs:dispatch_message` - Inbox manifest writing

### shipinbot (Python)
- `projects/shipinbot/scripts/openfang_clean_publish_bridge.py:2487` - `_collect_telegram_batch`
- `projects/shipinbot/scripts/openfang_clean_publish_bridge.py:2673` - `_fetch_telegram_video`
- `projects/shipinbot/scripts/openfang_clean_publish_bridge.py:2633` - `_telegram_resolve_download_url`
- `projects/shipinbot/openfang-hand/shipinfabu/HAND.toml` - Agent manifest

## 🚀 Quick Start

```bash
# Clone with submodules
git clone --recurse-submodules <repo-url>

# Build OpenFang
cargo build --workspace --lib

# Run tests
cargo test --workspace

# Start daemon
cargo build --release -p openfang-cli
GROQ_API_KEY=<key> target/release/openfang.exe start

# Dashboard at http://127.0.0.1:4200
```

## 📝 Notes

- **Submodule**: `projects/shipinbot/` is a Git submodule pointing to `/Users/xiaomo/Desktop/shipinbot`
- **Not a copy**: Changes in either location affect the same Git repository
- **Original repo**: Keep `/Users/xiaomo/Desktop/shipinbot` intact, it's the source of truth
