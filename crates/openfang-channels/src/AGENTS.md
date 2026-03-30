<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-channels — Pluggable Messaging Integrations

## Purpose

Provides 40+ channel integrations that convert platform-specific messages into unified `ChannelMessage` events for the kernel. Each channel driver handles auth, message formatting, media handling, and bidirectional synchronization.

## Key Files

| File | Purpose |
|------|---------|
| `router.rs` | `AgentRouter` — routes messages to agents by binding rules, user defaults, channel defaults |
| `bridge.rs` | Bridge traits and lifecycle — channel initialization, message dispatch, cleanup |
| `types.rs` | Shared types — `ChannelMessage`, `ChannelType`, channel-agnostic structures |
| `formatter.rs` | Message formatting — markdown-to-platform conversions (Discord embeds, etc.) |
| **Chat Channels** |
| `slack.rs` | Slack bot integration — messages, reactions, threads, slash commands |
| `telegram.rs` | Telegram bot integration — messages, media, inline keyboards |
| `discord.rs` | Discord bot integration — messages, embeds, reactions, voice |
| `teams.rs` | Microsoft Teams bot integration — messages, rich cards |
| `matrix.rs` | Matrix/Element integration — messages, reactions, encrypted rooms |
| `rocketchat.rs` | Rocket.Chat bot integration |
| `mattermost.rs` | Mattermost bot integration |
| `signal.rs` | Signal messenger integration — encrypted messages |
| `xmpp.rs` | XMPP/Jabber integration |
| `irc.rs` | IRC integration |
| **Social Media** |
| `twitter.rs` | Twitter integration (if present; see list output) |
| `mastodon.rs` | Mastodon/Fediverse integration |
| `bluesky.rs` | Bluesky AT Protocol integration |
| `linkedin.rs` | LinkedIn messaging |
| `reddit.rs` | Reddit bot integration |
| **Messaging Platforms** |
| `whatsapp.rs` | WhatsApp Business API integration |
| `viber.rs` | Viber bot integration |
| `line.rs` | LINE messaging platform |
| `messenger.rs` | Facebook Messenger integration |
| `twitch.rs` | Twitch chat bot |
| `gotify.rs` | Gotify push notifications |
| `ntfy.rs` | ntfy.sh push notifications |
| **Enterprise & Community** |
| `feishu.rs` | Feishu (ByteDance) integration |
| `dingtalk.rs` | DingTalk integration |
| `dingtalk_stream.rs` | DingTalk stream API |
| `wecom.rs` | WeCom (WeChat Work) integration |
| `google_chat.rs` | Google Chat bot integration |
| `webex.rs` | Cisco Webex integration |
| `flock.rs` | Flock team messaging |
| `guilded.rs` | Guilded server integration |
| `discourse.rs` | Discourse forum integration |
| `gitter.rs` | Gitter community chat |
| `keybase.rs` | Keybase team messaging |
| `nextcloud.rs` | Nextcloud Talk integration |
| `nostr.rs` | Nostr protocol integration |
| `mumble.rs` | Mumble voice server |
| `pumble.rs` | Pumble team chat |
| `threema.rs` | Threema encrypted messaging |
| `twist.rs` | Twist team communication |
| `zulip.rs` | Zulip team chat |
| `mqtt.rs` | MQTT pub-sub integration |
| `email.rs` | Email (SMTP/IMAP) integration |
| `webhook.rs` | Generic webhook receiver |

## For AI Agents

**When to read:** Understand message routing, implementing new channels, or debugging channel-specific issues.

**Key interface:**
- `ChannelMessage` — unified message type across all platforms
- `AgentRouter` — binding evaluation, message routing logic
- `ChannelBridge` trait — channel lifecycle and dispatch

**Common tasks:**
- Adding a new channel → create `channel_name.rs`, implement `ChannelBridge`
- Modifying routing rules → `router.rs` binding evaluation
- Adding media attachment handling → `types.rs` + channel-specific code
- Message formatting → `formatter.rs` conversion helpers

**Routing logic:** Bindings (specific user/guild) → Direct routes → User defaults → Channel defaults → System default.

**Architecture note:** Channels are loosely coupled via `ChannelBridge` trait. Each driver manages its own auth, event loops, and state.
