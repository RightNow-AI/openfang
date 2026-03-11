//! Channel Bridge Layer for the OpenFang Agent OS.
//!
//! Provides 40 pluggable messaging integrations that convert platform messages
//! into unified `ChannelMessage` events for the kernel.

pub mod bridge;
pub mod discord;
pub mod email;
pub mod formatter;
pub mod google_chat;
pub mod irc;
pub mod matrix;
pub mod mattermost;
pub mod rocketchat;
pub mod router;
pub mod signal;
pub mod slack;
pub mod teams;
pub mod telegram;
pub mod twitch;
pub mod types;
pub mod whatsapp;
pub mod xmpp;
pub mod zulip;
// Wave 3 — High-value channels
pub mod bluesky;
pub mod feishu;
pub mod line;
pub mod mastodon;
pub mod messenger;
pub mod reddit;
pub mod revolt;
pub mod viber;
// Wave 4 — Enterprise & community channels
pub mod flock;
pub mod guilded;
pub mod keybase;
pub mod nextcloud;
pub mod nostr;
pub mod pumble;
pub mod threema;
pub mod twist;
pub mod webex;
// Wave 5 — Niche & differentiating channels
pub mod dingtalk;
pub mod dingtalk_stream;
pub mod discourse;
pub mod gitter;
pub mod gotify;
pub mod linkedin;
pub mod mumble;
pub mod ntfy;
pub mod webhook;
pub mod wecom;
