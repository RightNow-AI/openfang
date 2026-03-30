# Channel Adapters

OpenFang connects to messaging platforms through **40 channel adapters**, allowing users to interact with their agents across every major communication platform. Adapters span consumer messaging, enterprise collaboration, social media, community platforms, privacy-focused protocols, and generic webhooks.

All adapters share a common foundation: graceful shutdown via `watch::channel`, exponential backoff on connection failures, `Zeroizing<String>` for secrets, automatic message splitting for platform limits, per-channel model/prompt overrides, DM/group policy enforcement, per-user rate limiting, and output formatting (Markdown, TelegramHTML, SlackMrkdwn, PlainText).

## Table of Contents

- [All 40 Channels](#all-40-channels)
- [Channel Configuration](#channel-configuration)
- [Channel Overrides](#channel-overrides)
- [Formatter, Rate Limiter, and Policies](#formatter-rate-limiter-and-policies)
- [Telegram](#telegram)
- [Discord](#discord)
- [Slack](#slack)
- [WhatsApp](#whatsapp)
- [Feishu / Lark](#feishu--lark)
- [Signal](#signal)
- [Matrix](#matrix)
- [Email](#email)
- [WebChat (Built-in)](#webchat-built-in)
- [Agent Routing](#agent-routing)
- [Writing Custom Adapters](#writing-custom-adapters)

---

## All 40 Channels

### Core (7)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| Telegram | Bot API long-polling | `TELEGRAM_BOT_TOKEN` | `Telegram` |
| Discord | Gateway WebSocket v10 | `DISCORD_BOT_TOKEN` | `Discord` |
| Slack | Socket Mode WebSocket | `SLACK_BOT_TOKEN`, `SLACK_APP_TOKEN` | `Slack` |
| WhatsApp | Cloud API webhook | `WA_ACCESS_TOKEN`, `WA_PHONE_ID`, `WA_VERIFY_TOKEN` | `WhatsApp` |
| Signal | signal-cli REST/JSON-RPC | _(system service)_ | `Signal` |
| Matrix | Client-Server API `/sync` | `MATRIX_TOKEN` | `Matrix` |
| Email | IMAP + SMTP | `EMAIL_PASSWORD` | `Email` |

### Enterprise (8)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| Microsoft Teams | Bot Framework v3 webhook + OAuth2 | `TEAMS_APP_ID`, `TEAMS_APP_SECRET` | `Teams` |
| Mattermost | WebSocket + REST v4 | `MATTERMOST_TOKEN`, `MATTERMOST_URL` | `Mattermost` |
| Google Chat | Service account webhook | `GOOGLE_CHAT_SA_KEY`, `GOOGLE_CHAT_SPACE` | `Custom("google_chat")` |
| Webex | Bot SDK WebSocket | `WEBEX_BOT_TOKEN` | `Custom("webex")` |
| Feishu / Lark | Open Platform Webhook / WebSocket | `FEISHU_APP_ID`, `FEISHU_APP_SECRET` | `Custom("feishu")` |
| Rocket.Chat | REST polling | `ROCKETCHAT_TOKEN`, `ROCKETCHAT_URL` | `Custom("rocketchat")` |
| Zulip | Event queue long-polling | `ZULIP_EMAIL`, `ZULIP_API_KEY`, `ZULIP_URL` | `Custom("zulip")` |
| XMPP | XMPP protocol (stub) | `XMPP_JID`, `XMPP_PASSWORD`, `XMPP_SERVER` | `Custom("xmpp")` |

### Social (8)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| LINE | Messaging API webhook | `LINE_CHANNEL_SECRET`, `LINE_CHANNEL_TOKEN` | `Custom("line")` |
| Viber | Bot API webhook | `VIBER_AUTH_TOKEN` | `Custom("viber")` |
| Facebook Messenger | Platform API webhook | `MESSENGER_PAGE_TOKEN`, `MESSENGER_VERIFY_TOKEN` | `Custom("messenger")` |
| Mastodon | Streaming API WebSocket | `MASTODON_TOKEN`, `MASTODON_INSTANCE` | `Custom("mastodon")` |
| Bluesky | AT Protocol WebSocket | `BLUESKY_HANDLE`, `BLUESKY_APP_PASSWORD` | `Custom("bluesky")` |
| Reddit | OAuth2 polling | `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET`, `REDDIT_USERNAME`, `REDDIT_PASSWORD` | `Custom("reddit")` |
| LinkedIn | Messaging API polling | `LINKEDIN_ACCESS_TOKEN` | `Custom("linkedin")` |
| Twitch | IRC gateway | `TWITCH_TOKEN`, `TWITCH_CHANNEL` | `Custom("twitch")` |

### Community (6)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| IRC | Raw TCP PRIVMSG | `IRC_SERVER`, `IRC_NICK`, `IRC_PASSWORD` | `Custom("irc")` |
| Guilded | WebSocket | `GUILDED_BOT_TOKEN` | `Custom("guilded")` |
| Revolt | WebSocket | `REVOLT_BOT_TOKEN` | `Custom("revolt")` |
| Keybase | Bot API polling | `KEYBASE_USERNAME`, `KEYBASE_PAPERKEY` | `Custom("keybase")` |
| Discourse | REST polling | `DISCOURSE_API_KEY`, `DISCOURSE_URL` | `Custom("discourse")` |
| Gitter | Streaming API | `GITTER_TOKEN` | `Custom("gitter")` |

### Self-hosted (1)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| Nextcloud Talk | REST polling | `NEXTCLOUD_TOKEN`, `NEXTCLOUD_URL` | `Custom("nextcloud")` |

### Privacy (3)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| Threema | Gateway API webhook | `THREEMA_ID`, `THREEMA_SECRET` | `Custom("threema")` |
| Nostr | NIP-01 relay WebSocket | `NOSTR_PRIVATE_KEY`, `NOSTR_RELAY` | `Custom("nostr")` |
| Mumble | TCP text protocol | `MUMBLE_SERVER`, `MUMBLE_USERNAME`, `MUMBLE_PASSWORD` | `Custom("mumble")` |

### Workplace (4)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| Pumble | Webhook | `PUMBLE_WEBHOOK_URL`, `PUMBLE_TOKEN` | `Custom("pumble")` |
| Flock | Webhook | `FLOCK_TOKEN` | `Custom("flock")` |
| Twist | API v3 polling | `TWIST_TOKEN` | `Custom("twist")` |
| DingTalk | Robot API webhook | `DINGTALK_TOKEN`, `DINGTALK_SECRET` | `Custom("dingtalk")` |

### Notification (2)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| ntfy | SSE pub/sub | `NTFY_TOPIC`, `NTFY_SERVER` | `Custom("ntfy")` |
| Gotify | WebSocket | `GOTIFY_TOKEN`, `GOTIFY_URL` | `Custom("gotify")` |

### Integration (1)

| Channel | Protocol | Env Vars | ChannelType Variant |
|---------|----------|----------|---------------------|
| Webhook | Generic HTTP with HMAC-SHA256 | `WEBHOOK_URL`, `WEBHOOK_SECRET` | `Custom("webhook")` |

---

## Channel Configuration

All channel configurations live in `~/.openfang/config.toml` under the `[channels]` section. Each channel is a subsection:

```toml
[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
default_agent = "assistant"
allowed_users = ["123456789"]

[channels.discord]
bot_token_env = "DISCORD_BOT_TOKEN"
default_agent = "coder"

[channels.slack]
bot_token_env = "SLACK_BOT_TOKEN"
app_token_env = "SLACK_APP_TOKEN"
default_agent = "ops"

# Enterprise example
[channels.teams]
app_id_env = "TEAMS_APP_ID"
app_secret_env = "TEAMS_APP_SECRET"
default_agent = "ops"

# Social example
[channels.mastodon]
token_env = "MASTODON_TOKEN"
instance = "https://mastodon.social"
default_agent = "social-media"
```

### Common Fields

- `bot_token_env` / `token_env` -- The environment variable holding the bot/access token. OpenFang reads the token from this env var at startup. All secrets are stored as `Zeroizing<String>` and wiped from memory on drop.
- `default_agent` -- The agent name (or ID) that receives messages when no specific routing applies.
- `allowed_users` -- Optional list of platform user IDs allowed to interact. Empty means allow all.
- `overrides` -- Optional per-channel behavior overrides (see [Channel Overrides](#channel-overrides) below).

### Environment Variables Reference (Core Channels)

| Channel | Required Env Vars |
|---------|-------------------|
| Telegram | `TELEGRAM_BOT_TOKEN` |
| Discord | `DISCORD_BOT_TOKEN` |
| Slack | `SLACK_BOT_TOKEN`, `SLACK_APP_TOKEN` |
| WhatsApp | `WA_ACCESS_TOKEN`, `WA_PHONE_ID`, `WA_VERIFY_TOKEN` |
| Matrix | `MATRIX_TOKEN` |
| Email | `EMAIL_PASSWORD` |

Env vars for all other channels are listed in the [All 40 Channels](#all-40-channels) tables above.

---

## Channel Overrides

Every channel adapter supports `ChannelOverrides`, which let you customize behavior per channel without modifying the agent manifest. Add an `[channels.<name>.overrides]` section in `config.toml`:

```toml
[channels.telegram.overrides]
model = "gemini-2.5-flash"
system_prompt = "You are a concise Telegram assistant. Keep replies under 200 words."
dm_policy = "respond"
group_policy = "mention_only"
rate_limit_per_user = 10
threading = true
output_format = "telegram_html"
usage_footer = "compact"
```

### Override Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `model` | `Option<String>` | Agent default | Override the LLM model for this channel. |
| `system_prompt` | `Option<String>` | Agent default | Override the system prompt for this channel. |
| `dm_policy` | `DmPolicy` | `Respond` | How to handle direct messages. |
| `group_policy` | `GroupPolicy` | `MentionOnly` | How to handle group/channel messages. |
| `rate_limit_per_user` | `u32` | `0` (unlimited) | Max messages per minute per user. |
| `threading` | `bool` | `false` | Send replies as thread responses (platforms that support it). |
| `output_format` | `Option<OutputFormat>` | `Markdown` | Output format for this channel. |
| `usage_footer` | `Option<UsageFooterMode>` | None | Whether to append token usage to responses. |

---

## Formatter, Rate Limiter, and Policies

### Output Formatter

The `formatter` module (`openfang-channels/src/formatter.rs`) converts Markdown output from the LLM into platform-native formats:

| OutputFormat | Target | Notes |
|-------------|--------|-------|
| `Markdown` | Standard Markdown | Default; passed through as-is. |
| `TelegramHtml` | Telegram HTML subset | Converts `**bold**` to `<b>`, `` `code` `` to `<code>`, etc. |
| `SlackMrkdwn` | Slack mrkdwn | Converts `**bold**` to `*bold*`, links to `<url\|text>`, etc. |
| `PlainText` | Plain text | Strips all formatting. |

### Per-User Rate Limiter

The `ChannelRateLimiter` (`openfang-channels/src/rate_limiter.rs`) uses a `DashMap` to track per-user message counts. When `rate_limit_per_user` is set on a channel's overrides, the limiter enforces a sliding-window cap of N messages per minute. Excess messages receive a polite rejection.

### DM Policy

Controls how the adapter handles direct messages:

| DmPolicy | Behavior |
|----------|----------|
| `Respond` | Respond to all DMs (default). |
| `AllowedOnly` | Only respond to DMs from users in `allowed_users`. |
| `Ignore` | Silently drop all DMs. |

### Group Policy

Controls how the adapter handles messages in group chats, channels, and rooms:

| GroupPolicy | Behavior |
|-------------|----------|
| `All` | Respond to every message in the group. |
| `MentionOnly` | Only respond when the bot is @mentioned (default). |
| `CommandsOnly` | Only respond to `/command` messages. |
| `Ignore` | Silently ignore all group messages. |

Policy enforcement happens in `dispatch_message()` before the message reaches the agent loop. This means ignored messages consume zero LLM tokens.

---

## Telegram

### Prerequisites

- A Telegram bot token (from [@BotFather](https://t.me/botfather))

### Setup

1. Open Telegram and message `@BotFather`.
2. Send `/newbot` and follow the prompts to create a new bot.
3. Copy the bot token.
4. Set the environment variable:

```bash
export TELEGRAM_BOT_TOKEN=123456789:ABCdefGHIjklMNOpqrsTUVwxyz
```

5. Add to config:

```toml
[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
default_agent = "assistant"
# Optional: restrict to specific Telegram user IDs
# allowed_users = ["123456789"]

[channels.telegram.overrides]
# Optional: Telegram-native HTML formatting
# output_format = "telegram_html"
# group_policy = "mention_only"
```

6. Restart the daemon:

```bash
openfang start
```

### How It Works

The Telegram adapter uses long-polling via the `getUpdates` API. It polls every few seconds with a 30-second long-poll timeout. On API failures, it applies exponential backoff (starting at 1 second, up to 60 seconds). Shutdown is coordinated via a `watch::channel`.

Messages from authorized users are converted to `ChannelMessage` events and routed to the configured agent. Responses are sent back via the `sendMessage` API. Long responses are automatically split into multiple messages to respect Telegram's 4096-character limit using the shared `split_message()` utility.

### Interactive Setup

```bash
openfang channel setup telegram
```

This walks you through the setup interactively.

---

## Discord

### Prerequisites

- A Discord application and bot (from the [Discord Developer Portal](https://discord.com/developers/applications))

### Setup

1. Go to [Discord Developer Portal](https://discord.com/developers/applications).
2. Click "New Application" and name it.
3. Go to the **Bot** section and click "Add Bot".
4. Copy the bot token.
5. Under **Privileged Gateway Intents**, enable:
   - **Message Content Intent** (required to read message content)
6. Go to **OAuth2 > URL Generator**:
   - Select scopes: `bot`
   - Select permissions: `Send Messages`, `Read Message History`
   - Copy the generated URL and open it to invite the bot to your server.
7. Set the environment variable:

```bash
export DISCORD_BOT_TOKEN=MTIzNDU2Nzg5.ABCDEF.ghijklmnop
```

8. Add to config:

```toml
[channels.discord]
bot_token_env = "DISCORD_BOT_TOKEN"
default_agent = "coder"
```

9. Restart the daemon.

### How It Works

The Discord adapter connects to the Discord Gateway via WebSocket (v10). It listens for `MESSAGE_CREATE` events and routes messages to the configured agent. Responses are sent via the REST API's `channels/{id}/messages` endpoint.

The adapter handles Gateway reconnection, heartbeating, and session resumption automatically.

---

## Slack

### Prerequisites

- A Slack app with Socket Mode enabled

### Setup

1. Go to [Slack API](https://api.slack.com/apps) and click "Create New App" > "From Scratch".
2. Enable **Socket Mode** (Settings > Socket Mode):
   - Generate an App-Level Token with scope `connections:write`.
   - Copy the token (`xapp-...`).
3. Go to **OAuth & Permissions** and add Bot Token Scopes:
   - `chat:write`
   - `app_mentions:read`
   - `im:history`
   - `im:read`
   - `im:write`
4. Install the app to your workspace.
5. Copy the Bot User OAuth Token (`xoxb-...`).
6. Set the environment variables:

```bash
export SLACK_APP_TOKEN=xapp-1-...
export SLACK_BOT_TOKEN=xoxb-...
```

7. Add to config:

```toml
[channels.slack]
bot_token_env = "SLACK_BOT_TOKEN"
app_token_env = "SLACK_APP_TOKEN"
default_agent = "ops"

[channels.slack.overrides]
# Optional: Slack-native mrkdwn formatting
# output_format = "slack_mrkdwn"
# threading = true
```

8. Restart the daemon.

### How It Works

The Slack adapter uses Socket Mode, which establishes a WebSocket connection to Slack's servers. This avoids the need for a public webhook URL. The adapter receives events (app mentions, direct messages) and routes them to the configured agent. Responses are posted via the `chat.postMessage` Web API. When `threading = true`, replies are sent to the message's thread via `thread_ts`.

---

## WhatsApp

### Prerequisites

- A Meta Business account with WhatsApp Cloud API access

### Setup

1. Go to [Meta for Developers](https://developers.facebook.com/).
2. Create a Business App.
3. Add the WhatsApp product.
4. Set up a test phone number (or use a production one).
5. Copy:
   - Phone Number ID
   - Permanent Access Token
   - Choose a Verify Token (any string you choose)
6. Set environment variables:

```bash
export WA_PHONE_ID=123456789012345
export WA_ACCESS_TOKEN=EAABs...
export WA_VERIFY_TOKEN=my-secret-verify-token
```

7. Add to config:

```toml
[channels.whatsapp]
mode = "cloud_api"
phone_number_id_env = "WA_PHONE_ID"
access_token_env = "WA_ACCESS_TOKEN"
verify_token_env = "WA_VERIFY_TOKEN"
webhook_port = 8443
default_agent = "assistant"
```

8. Set up a webhook in the Meta dashboard pointing to your server's public URL:
   - URL: `https://your-domain.com:8443/webhook/whatsapp`
   - Verify Token: the value you chose above
   - Subscribe to: `messages`

9. Restart the daemon.

### How It Works

The WhatsApp adapter runs an HTTP server (on the configured `webhook_port`) that receives incoming webhooks from the WhatsApp Cloud API. It handles webhook verification (GET) and message reception (POST). Responses are sent via the Cloud API's `messages` endpoint.

---

## Feishu / Lark

### Prerequisites

- A Feishu/Lark app created in [open.feishu.cn](https://open.feishu.cn/)
- App ID and App Secret

### Setup

1. Create a custom app in Feishu Open Platform.
2. Enable the IM message event subscription for your app.
3. Set environment variable:

```bash
export FEISHU_APP_SECRET=cli_xxx_secret
```

4. Add to config (default: `websocket` mode):

```toml
[channels.feishu]
app_id = "cli_xxx"
app_secret_env = "FEISHU_APP_SECRET"
mode = "websocket"
default_agent = "assistant"
```

5. Restart the daemon.

### Webhook Compatibility Mode

If you need the legacy callback flow, switch to `webhook` and expose a public callback URL:

```toml
[channels.feishu]
app_id = "cli_xxx"
app_secret_env = "FEISHU_APP_SECRET"
mode = "webhook"
webhook_port = 8453
default_agent = "assistant"
```

Then configure Feishu event callback to:

`https://<your-domain>:8453/feishu/webhook`

### How It Works

- **websocket mode**: OpenFang obtains endpoint from Feishu and receives events via long connection (no public inbound webhook needed).
- **webhook mode**: OpenFang starts an HTTP callback server and receives Feishu push events.
- **send path (both modes)**: outbound messages still go through Feishu OpenAPI HTTP `im/v1/messages`.

---

## Signal

### Prerequisites

- Signal CLI installed and linked to a phone number

### Setup

1. Install [signal-cli](https://github.com/AsamK/signal-cli).
2. Register or link a phone number.
3. Add to config:

```toml
[channels.signal]
signal_cli_path = "/usr/local/bin/signal-cli"
phone_number = "+1234567890"
default_agent = "assistant"
```

4. Restart the daemon.

### How It Works

The Signal adapter spawns `signal-cli` as a subprocess in daemon mode and communicates via JSON-RPC. Incoming messages are read from the signal-cli output stream and routed to the configured agent.

---

## Matrix

### Prerequisites

- A Matrix homeserver account and access token

### Setup

1. Create a bot account on your Matrix homeserver.
2. Generate an access token.
3. Set the environment variable:

```bash
export MATRIX_TOKEN=syt_...
```

4. Add to config:

```toml
[channels.matrix]
homeserver_url = "https://matrix.org"
access_token_env = "MATRIX_TOKEN"
user_id = "@openfang-bot:matrix.org"
default_agent = "assistant"
```

5. Invite the bot to the rooms you want it to monitor.
6. Restart the daemon.

### How It Works

The Matrix adapter uses the Matrix Client-Server API. It syncs with the homeserver using long-polling (`/sync` with a timeout) and processes new messages from joined rooms. Responses are sent via the `/rooms/{roomId}/send` endpoint.

---

## Email

### Prerequisites

- An email account with IMAP and SMTP access

### Setup

1. For Gmail, create an [App Password](https://myaccount.google.com/apppasswords).
2. Set the environment variable:

```bash
export EMAIL_PASSWORD=abcd-efgh-ijkl-mnop
```

3. Add to config:

```toml
[channels.email]
imap_host = "imap.gmail.com"
imap_port = 993
smtp_host = "smtp.gmail.com"
smtp_port = 587
username = "you@gmail.com"
password_env = "EMAIL_PASSWORD"
poll_interval = 30
default_agent = "email-assistant"
```

4. Restart the daemon.

### How It Works

The email adapter polls the IMAP inbox at the configured interval. New emails are parsed (subject + body) and routed to the configured agent. Responses are sent as reply emails via SMTP, preserving the subject line threading.

---

## WebChat (Built-in)

The WebChat UI is embedded in the daemon and requires no configuration. When the daemon is running:

```
http://127.0.0.1:4200/
```

Features:
- Real-time chat via WebSocket
- Streaming responses (text deltas as they arrive)
- Agent selection (switch between running agents)
- Token usage display
- No authentication required on localhost (protected by CORS)

---

## Agent Routing

The `AgentRouter` determines which agent receives an incoming message. The routing logic is:

1. **Per-channel default**: Each channel config has a `default_agent` field. Messages from that channel go to that agent.
2. **User-agent binding**: If a user has previously been associated with a specific agent (via commands or configuration), messages from that user route to that agent.
3. **Command prefix**: Users can switch agents by sending a command like `/agent coder` in the chat. Subsequent messages will be routed to the "coder" agent.
4. **Fallback**: If no routing applies, messages go to the first available agent.

---

## Writing Custom Adapters

To add support for a new messaging platform, implement the `ChannelAdapter` trait. The trait is defined in `crates/openfang-channels/src/types.rs`.

### The ChannelAdapter Trait

```rust
pub trait ChannelAdapter: Send + Sync {
    /// Human-readable name of this adapter.
    fn name(&self) -> &str;

    /// The channel type this adapter handles.
    fn channel_type(&self) -> ChannelType;

    /// Start receiving messages. Returns a stream of incoming messages.
    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>;

    /// Send a response back to a user on this channel.
    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Send a typing indicator (optional -- default no-op).
    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Stop the adapter and clean up resources.
    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>>;

    /// Get the current health status of this adapter (optional -- default returns disconnected).
    fn status(&self) -> ChannelStatus {
        ChannelStatus::default()
    }

    /// Send a response as a thread reply (optional -- default falls back to `send()`).
    async fn send_in_thread(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        _thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.send(user, content).await
    }
}
```

### 1. Define Your Adapter

Create `crates/openfang-channels/src/myplatform.rs`:

```rust
use crate::types::{
    ChannelAdapter, ChannelContent, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
};
use futures::stream::{self, Stream};
use std::pin::Pin;
use tokio::sync::watch;
use zeroize::Zeroizing;

pub struct MyPlatformAdapter {
    token: Zeroizing<String>,
    client: reqwest::Client,
    shutdown: watch::Receiver<bool>,
}

impl MyPlatformAdapter {
    pub fn new(token: String, shutdown: watch::Receiver<bool>) -> Self {
        Self {
            token: Zeroizing::new(token),
            client: reqwest::Client::new(),
            shutdown,
        }
    }
}

impl ChannelAdapter for MyPlatformAdapter {
    fn name(&self) -> &str {
        "MyPlatform"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("myplatform".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>> {
        // Return a stream that yields ChannelMessage items.
        // Use self.shutdown to detect when the daemon is stopping.
        // Apply exponential backoff on connection failures.
        let stream = stream::empty(); // Replace with your polling/WebSocket logic
        Ok(Box::pin(stream))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Send the response back to the platform.
        // Use split_message() if the platform has message length limits.
        // Use self.client and self.token to call the platform's API.
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Clean shutdown: close connections, stop polling.
        Ok(())
    }

    fn status(&self) -> ChannelStatus {
        ChannelStatus::default()
    }
}
```

**Key points for new adapters:**
- Use `ChannelType::Custom("myplatform".to_string())` for the channel type. Only the 9 most common channels have named `ChannelType` variants (`Telegram`, `WhatsApp`, `Slack`, `Discord`, `Signal`, `Matrix`, `Email`, `Teams`, `Mattermost`). All others use `Custom(String)`.
- Wrap secrets in `Zeroizing<String>` so they are wiped from memory on drop.
- Accept a `watch::Receiver<bool>` for coordinated shutdown with the daemon.
- Use exponential backoff for resilience on connection failures.
- Use the shared `split_message(text, max_len)` utility for platforms with message length limits.

### 2. Register the Module

In `crates/openfang-channels/src/lib.rs`:

```rust
pub mod myplatform;
```

### 3. Wire It Into the Bridge

In `crates/openfang-api/src/channel_bridge.rs`, add initialization logic for your adapter alongside the existing adapters.

### 4. Add Config Support

In `openfang-types`, add a config struct:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MyPlatformConfig {
    pub token_env: String,
    pub default_agent: Option<String>,
    #[serde(default)]
    pub overrides: ChannelOverrides,
}
```

Add it to the `ChannelsConfig` struct and `config.toml` parsing. The `overrides` field gives your channel automatic support for model/prompt overrides, DM/group policies, rate limiting, threading, and output format selection.

### 5. Add CLI Setup Wizard

In `crates/openfang-cli/src/main.rs`, add a case to `cmd_channel_setup` with step-by-step instructions for your platform.

### 6. Test

Write integration tests. Use the `ChannelMessage` type to simulate incoming messages without connecting to the real platform.
