<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-channels

## Purpose

The channel bridge layer provides 40 pluggable messaging integrations that convert platform-specific messages into unified `ChannelMessage` events for the kernel. Channels include Discord, Slack, Telegram, WhatsApp, Email (SMTP/IMAP), MQTT, IRC, Matrix, Mastodon, Bluesky, Feishu, Dingtalk, and 25+ others. Each channel adapter handles authentication, message formatting, media uploads, and bidirectional routing (agent messages back to platform).

## Key Files

| File | Description |
|------|-------------|
| `src/lib.rs` | Public module exports — 40 channel implementations. |
| `src/bridge.rs` | Core abstraction: `ChannelBridge` trait, `ChannelMessage` unified struct, message router. Routes platform-specific msgs to kernel, kernel responses back to platforms. |
| `src/types.rs` | Shared types: `ChannelMessage`, `ChannelEvent`, auth configs, metadata. |
| `src/router.rs` | Message routing logic — dispatch to correct channel adapter. |
| `src/formatter.rs` | Message formatting utilities — markdown-to-platform, media handling, mentions/links conversion. |
| `src/discord.rs`, `src/slack.rs`, etc. | Per-channel adapters (40 total). Each handles platform SDK, auth, event loop, message translation. |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/` | Bridge trait, unified message types, router, formatters, 40 channel implementations. |
| `tests/` | Integration tests (mocked channel backends). |

## For AI Agents

### Working In This Directory

- **Adding a new channel**: Create `src/newchannel.rs` implementing `ChannelBridge` trait. Register in `src/lib.rs` and route in `router.rs`.
- **Message formatting**: Update `formatter.rs` for new markup/media types.
- **Extending ChannelMessage**: Add fields for new platforms (e.g., reactions, thread IDs, ephemeral messages).
- **Auth handling**: Add credential types to config, handle rotation/refresh in channel adapters.
- **Media support**: Extend media upload logic in bridge for new platforms.

### Testing Requirements

- Unit tests for each channel's message parsing and formatting.
- Mock SDK responses — test message delivery, event handling, error cases.
- Test router — verify messages route to correct channel.
- Test formatter — convert markdown/media for different platforms.
- No live API tests (use SDK mocks).

### Common Patterns

- Channels implement `ChannelBridge` async trait: `connect()`, `send_message()`, `receive()`, `disconnect()`.
- `ChannelMessage` is the canonical message type — all platforms convert to/from this.
- Platform-specific fields (e.g., Discord reactions, Slack threads) go in optional metadata.
- Auth credentials come from config/env — never hardcoded.
- Media URLs are resolved at send time (upload to platform, include in message).
- Mentions and links are normalized across platforms (e.g., `@username` → `<@user_id>` for Discord).
- Emoji and special chars are platform-agnostic (handled by formatter).

## Dependencies

### Internal

- `openfang-types` — shared types.

### External

- `tokio` — async runtime, channels.
- `serde`, `serde_json`, `toml` — config/message serialization.
- `reqwest` — HTTP for webhook/REST API channels.
- `async-trait` — async trait definitions.
- `lettre`, `imap` — email (SMTP/IMAP).
- `rumqttc` — MQTT.
- `tokio-tungstenite` — WebSocket (Matrix, Mattermost).
- `base64`, `hex`, `hmac`, `sha2`, `sha1` — auth/signing.
- `aes`, `cbc` — encryption (WhatsApp, Signal).
- `roxmltree` — XML parsing (Feishu, some XMPP).
- `mailparse` — email body parsing.

<!-- MANUAL: -->
