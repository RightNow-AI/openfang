# Channels

Channels are the communication surfaces where LegendClaw agents receive work, respond, and enforce platform-specific policies. This page is the top-level entry point for choosing the right delivery surface before you dive into adapter-level configuration.

---

## Start Here

- Read [Channel Adapters](channel-adapters.md) for the full adapter catalog and config details.
- Read [Integration Contract](integration-contract.md) if you are building a gateway or backend-owned bridge.
- Read [Security](security.md) before enabling external-facing channels in production.

## What A Channel Covers

A channel is more than a transport. Each one can bring:

- a default agent or routing policy
- per-channel prompt and model overrides
- direct-message and group-message rules
- rate limits, formatting, and threading behavior
- platform-specific credential and webhook handling

## Channel Families

### Core Messaging

Best when you need broad user access across common chat surfaces.

Includes:

- Telegram
- Discord
- Slack
- WhatsApp
- Signal
- Matrix
- Email

Reference: [Channel Adapters](channel-adapters.md)

### Enterprise Collaboration

Best when the system needs to live inside team communication tools and internal operations workflows.

Includes:

- Microsoft Teams
- Mattermost
- Google Chat
- Webex
- Feishu or Lark
- Rocket.Chat
- Zulip
- XMPP

Reference: [Channel Adapters](channel-adapters.md)

### Social And Community

Best when the system needs to participate in audience, community, or content loops rather than internal support only.

Includes:

- LINE
- Viber
- Facebook Messenger
- Mastodon
- Bluesky
- Reddit
- LinkedIn
- Twitch
- IRC
- Guilded
- Revolt
- Keybase
- Discourse
- Gitter

Reference: [Channel Adapters](channel-adapters.md)

### Privacy, Workplace, And Notification

Best when deployment constraints or audience expectations require specialized adapters.

Includes:

- Threema
- Nostr
- Mumble
- Pumble
- Flock
- Twist
- DingTalk
- ntfy
- Gotify
- Webhook

Reference: [Channel Adapters](channel-adapters.md)

## How To Choose A Channel

- Choose chat-native channels when the user interaction loop is conversational.
- Choose workplace channels when the system should live inside existing team operations.
- Choose notification-style channels when the product mainly pushes alerts or approvals.
- Choose webhook-style channels when your own backend is the real interaction surface and LegendClaw is behind it.

## Deployment Pattern

For production systems, prefer this shape:

```text
User or system
  -> your app or gateway
      -> LegendClaw
```

This keeps authentication, tenancy, audit, and kill-switch logic in your own backend instead of scattering it across every adapter.

Read more in [Integration Contract](integration-contract.md).

## Channel Readiness Checklist

- Confirm the target platform actually matches the user workflow.
- Define the default agent and escalation path.
- Apply per-channel overrides where brevity or formatting matters.
- Add rate limits and allowed-user rules for public or semi-public surfaces.
- Validate the auth and secret model from [Security](security.md).

## Next Step

Once you know where the interaction should happen, use [Integrations](integrations.md) to decide how apps, SDKs, MCP servers, and external agents should connect behind the channel layer.
