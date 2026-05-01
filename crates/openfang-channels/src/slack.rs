//! Slack Socket Mode adapter for the OpenFang channel bridge.
//!
//! Uses Slack Socket Mode WebSocket (app token) for receiving events and the
//! Web API (bot token) for sending responses. No external Slack crate.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use dashmap::DashMap;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{debug, error, info, warn};
use zeroize::Zeroizing;

const SLACK_API_BASE: &str = "https://slack.com/api";
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const SLACK_MSG_LIMIT: usize = 3000;
/// TTL for envelope_id dedup entries. Well above the typical Slack
/// connection-rotation overlap window (< 10s).
const ENVELOPE_TTL: Duration = Duration::from_secs(60);
/// Soft cap on the dedup cache size. When exceeded we GC expired entries.
/// Recent envelope IDs are not reused by Slack, so 10k is more than enough.
const ENVELOPE_CACHE_CAP: usize = 10_000;

/// Returns true if `envelope_id` was already seen within `ENVELOPE_TTL`.
/// On first sight, records the timestamp and returns false. Performs
/// opportunistic GC of expired entries when the cache grows large.
///
/// Slack Socket Mode delivers the same event to multiple active WebSocket
/// connections during connection rotation. Apps must dedupe on `envelope_id`
/// to avoid double-processing.
fn is_duplicate_envelope(cache: &DashMap<String, Instant>, envelope_id: &str) -> bool {
    if envelope_id.is_empty() {
        return false;
    }

    // Opportunistic GC: bound growth without per-call work.
    if cache.len() > ENVELOPE_CACHE_CAP {
        cache.retain(|_, ts| ts.elapsed() < ENVELOPE_TTL);
    }

    if let Some(prev) = cache.get(envelope_id) {
        if prev.elapsed() < ENVELOPE_TTL {
            return true;
        }
    }
    cache.insert(envelope_id.to_string(), Instant::now());
    false
}

/// Slack Socket Mode adapter.
pub struct SlackAdapter {
    /// SECURITY: Tokens are zeroized on drop to prevent memory disclosure.
    app_token: Zeroizing<String>,
    bot_token: Zeroizing<String>,
    client: reqwest::Client,
    allowed_channels: Vec<String>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Bot's own user ID (populated after auth.test).
    bot_user_id: Arc<RwLock<Option<String>>>,
    /// Threads where the bot was @-mentioned. Maps thread_ts -> last interaction time.
    active_threads: Arc<DashMap<String, Instant>>,
    /// How long to track a thread after last interaction.
    thread_ttl: Duration,
    /// Whether auto-thread-reply is enabled.
    auto_thread_reply: bool,
    /// Whether to unfurl (expand previews for) links in posted messages.
    unfurl_links: bool,
    /// Recently-seen envelope_ids. Slack Socket Mode redelivers the same event
    /// across rotated WebSocket connections; this prevents double-processing.
    seen_envelopes: Arc<DashMap<String, Instant>>,
}

impl SlackAdapter {
    pub fn new(
        app_token: String,
        bot_token: String,
        allowed_channels: Vec<String>,
        auto_thread_reply: bool,
        thread_ttl_hours: u64,
        unfurl_links: bool,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_token: Zeroizing::new(app_token),
            bot_token: Zeroizing::new(bot_token),
            client: reqwest::Client::new(),
            allowed_channels,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_user_id: Arc::new(RwLock::new(None)),
            active_threads: Arc::new(DashMap::new()),
            thread_ttl: Duration::from_secs(thread_ttl_hours * 3600),
            auto_thread_reply,
            unfurl_links,
            seen_envelopes: Arc::new(DashMap::new()),
        }
    }

    /// Validate the bot token by calling auth.test.
    async fn validate_bot_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let resp: serde_json::Value = self
            .client
            .post(format!("{SLACK_API_BASE}/auth.test"))
            .header(
                "Authorization",
                format!("Bearer {}", self.bot_token.as_str()),
            )
            .send()
            .await?
            .json()
            .await?;

        if resp["ok"].as_bool() != Some(true) {
            let err = resp["error"].as_str().unwrap_or("unknown error");
            return Err(format!("Slack auth.test failed: {err}").into());
        }

        let user_id = resp["user_id"].as_str().unwrap_or("unknown").to_string();
        Ok(user_id)
    }

    /// Send a message to a Slack channel via chat.postMessage.
    async fn api_send_message(
        &self,
        channel_id: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chunks = split_message(text, SLACK_MSG_LIMIT);

        for chunk in chunks {
            let mut body = serde_json::json!({
                "channel": channel_id,
                "text": chunk,
                "unfurl_links": self.unfurl_links,
                "unfurl_media": self.unfurl_links,
            });
            if let Some(ts) = thread_ts {
                body["thread_ts"] = serde_json::json!(ts);
            }

            let resp: serde_json::Value = self
                .client
                .post(format!("{SLACK_API_BASE}/chat.postMessage"))
                .header(
                    "Authorization",
                    format!("Bearer {}", self.bot_token.as_str()),
                )
                .json(&body)
                .send()
                .await?
                .json()
                .await?;

            if resp["ok"].as_bool() != Some(true) {
                let err = resp["error"].as_str().unwrap_or("unknown");
                warn!("Slack chat.postMessage failed: {err}");
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for SlackAdapter {
    fn name(&self) -> &str {
        "slack"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Slack
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate bot token first
        let bot_user_id_val = self.validate_bot_token().await?;
        *self.bot_user_id.write().await = Some(bot_user_id_val.clone());
        info!("Slack bot authenticated (user_id: {bot_user_id_val})");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);

        let app_token = self.app_token.clone();
        let bot_user_id = self.bot_user_id.clone();
        let allowed_channels = self.allowed_channels.clone();
        let client = self.client.clone();
        let mut shutdown = self.shutdown_rx.clone();
        let active_threads = self.active_threads.clone();
        let auto_thread_reply = self.auto_thread_reply;
        let seen_envelopes = self.seen_envelopes.clone();

        // Spawn periodic cleanup of expired thread entries.
        {
            let active_threads = self.active_threads.clone();
            let thread_ttl = self.thread_ttl;
            let mut cleanup_shutdown = self.shutdown_rx.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            active_threads.retain(|_, last| last.elapsed() < thread_ttl);
                        }
                        _ = cleanup_shutdown.changed() => {
                            if *cleanup_shutdown.borrow() {
                                return;
                            }
                        }
                    }
                }
            });
        }

        tokio::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;

            loop {
                if *shutdown.borrow() {
                    break;
                }

                // Get a fresh WebSocket URL
                let ws_url_result = get_socket_mode_url(&client, &app_token)
                    .await
                    .map_err(|e| e.to_string());
                let ws_url = match ws_url_result {
                    Ok(url) => url,
                    Err(err_msg) => {
                        warn!("Slack: failed to get WebSocket URL: {err_msg}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                info!("Connecting to Slack Socket Mode...");

                let ws_result = tokio_tungstenite::connect_async(&ws_url).await;
                let ws_stream = match ws_result {
                    Ok((stream, _)) => stream,
                    Err(e) => {
                        warn!("Slack WebSocket connection failed: {e}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                        continue;
                    }
                };

                backoff = INITIAL_BACKOFF;
                info!("Slack Socket Mode connected");

                let (mut ws_tx, mut ws_rx) = ws_stream.split();

                let should_reconnect = 'inner: loop {
                    let msg = tokio::select! {
                        msg = ws_rx.next() => msg,
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() {
                                let _ = ws_tx.close().await;
                                return;
                            }
                            continue;
                        }
                    };

                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            warn!("Slack WebSocket error: {e}");
                            break 'inner true;
                        }
                        None => {
                            info!("Slack WebSocket closed");
                            break 'inner true;
                        }
                    };

                    let text = match msg {
                        tokio_tungstenite::tungstenite::Message::Text(t) => t,
                        tokio_tungstenite::tungstenite::Message::Close(_) => {
                            info!("Slack Socket Mode closed by server");
                            break 'inner true;
                        }
                        _ => continue,
                    };

                    let payload: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Slack: failed to parse message: {e}");
                            continue;
                        }
                    };

                    let envelope_type = payload["type"].as_str().unwrap_or("");

                    match envelope_type {
                        "hello" => {
                            debug!("Slack Socket Mode hello received");
                        }

                        "events_api" => {
                            // Acknowledge the envelope
                            let envelope_id = payload["envelope_id"].as_str().unwrap_or("");
                            if !envelope_id.is_empty() {
                                let ack = serde_json::json!({ "envelope_id": envelope_id });
                                if let Err(e) = ws_tx
                                    .send(tokio_tungstenite::tungstenite::Message::Text(
                                        serde_json::to_string(&ack).unwrap(),
                                    ))
                                    .await
                                {
                                    error!("Slack: failed to send ack: {e}");
                                    break 'inner true;
                                }
                            }

                            // Dedup: Slack redelivers the same event on the new
                            // connection during the rotation overlap. Ack on
                            // both, but only forward to the agent once.
                            if is_duplicate_envelope(&seen_envelopes, envelope_id) {
                                debug!(
                                    "Slack: skipping duplicate envelope_id {envelope_id}"
                                );
                                continue;
                            }

                            // Extract the event
                            let event = &payload["payload"]["event"];
                            if let Some(msg) = parse_slack_event(
                                event,
                                &bot_user_id,
                                &allowed_channels,
                                &active_threads,
                                auto_thread_reply,
                            )
                            .await
                            {
                                debug!(
                                    "Slack message from {}: {:?}",
                                    msg.sender.display_name, msg.content
                                );
                                if tx.send(msg).await.is_err() {
                                    return;
                                }
                            }
                        }

                        "disconnect" => {
                            let reason = payload["reason"].as_str().unwrap_or("unknown");
                            info!("Slack disconnect request: {reason}");
                            break 'inner true;
                        }

                        _ => {
                            debug!("Slack envelope type: {envelope_type}");
                        }
                    }
                };

                if !should_reconnect || *shutdown.borrow() {
                    break;
                }

                warn!("Slack: reconnecting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }

            info!("Slack Socket Mode loop stopped");
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let channel_id = &user.platform_id;
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(channel_id, &text, None).await?;
            }
            _ => {
                self.api_send_message(channel_id, "(Unsupported content type)", None)
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_in_thread(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let channel_id = &user.platform_id;
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(channel_id, &text, Some(thread_id))
                    .await?;
            }
            _ => {
                self.api_send_message(channel_id, "(Unsupported content type)", Some(thread_id))
                    .await?;
            }
        }
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

/// Helper to get Socket Mode WebSocket URL.
async fn get_socket_mode_url(
    client: &reqwest::Client,
    app_token: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let resp: serde_json::Value = client
        .post(format!("{SLACK_API_BASE}/apps.connections.open"))
        .header("Authorization", format!("Bearer {app_token}"))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .await?
        .json()
        .await?;

    if resp["ok"].as_bool() != Some(true) {
        let err = resp["error"].as_str().unwrap_or("unknown error");
        return Err(format!("Slack apps.connections.open failed: {err}").into());
    }

    resp["url"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "Missing 'url' in connections.open response".into())
}

/// Parse a Slack event into a `ChannelMessage`.
async fn parse_slack_event(
    event: &serde_json::Value,
    bot_user_id: &Arc<RwLock<Option<String>>>,
    allowed_channels: &[String],
    active_threads: &Arc<DashMap<String, Instant>>,
    auto_thread_reply: bool,
) -> Option<ChannelMessage> {
    let event_type = event["type"].as_str()?;
    if event_type != "message" && event_type != "app_mention" {
        return None;
    }

    // Handle message_changed subtype: extract inner message
    let subtype = event["subtype"].as_str();
    let (msg_data, is_edit) = match subtype {
        Some("message_changed") => {
            // Edited messages have the new content in event.message
            match event.get("message") {
                Some(inner) => (inner, true),
                None => return None,
            }
        }
        Some(_) => return None, // Skip other subtypes (joins, leaves, etc.)
        None => (event, false),
    };

    // Filter out bot's own messages
    if msg_data.get("bot_id").is_some() {
        return None;
    }
    let user_id = msg_data["user"]
        .as_str()
        .or_else(|| event["user"].as_str())?;
    if let Some(ref bid) = *bot_user_id.read().await {
        if user_id == bid {
            return None;
        }
    }

    let channel = event["channel"].as_str()?;

    // Filter by allowed channels
    if !allowed_channels.is_empty() && !allowed_channels.contains(&channel.to_string()) {
        return None;
    }

    let text = msg_data["text"].as_str().unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let ts = if is_edit {
        msg_data["ts"]
            .as_str()
            .unwrap_or(event["ts"].as_str().unwrap_or("0"))
    } else {
        event["ts"].as_str().unwrap_or("0")
    };

    // Parse timestamp (Slack uses epoch.microseconds format)
    let timestamp = ts
        .split('.')
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|epoch| chrono::DateTime::from_timestamp(epoch, 0))
        .unwrap_or_else(chrono::Utc::now);

    // Parse commands (messages starting with /)
    let content = if text.starts_with('/') {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let cmd_name = &parts[0][1..];
        let args = if parts.len() > 1 {
            parts[1].split_whitespace().map(String::from).collect()
        } else {
            vec![]
        };
        ChannelContent::Command {
            name: cmd_name.to_string(),
            args,
        }
    } else {
        ChannelContent::Text(text.to_string())
    };

    // Extract thread_id: threaded replies have `thread_ts`, top-level messages
    // use their own `ts` so the reply will start a thread under the original.
    let thread_id = msg_data["thread_ts"]
        .as_str()
        .or_else(|| event["thread_ts"].as_str())
        .map(|s| s.to_string())
        .or_else(|| Some(ts.to_string()));

    // Check if the bot was @-mentioned (for group_policy = "mention_only")
    let mut metadata = HashMap::new();
    // Stash the Slack user ID so the router can key bindings on user, not channel.
    // (`sender.platform_id` below is the channel ID, used for the send path.)
    metadata.insert("sender_user_id".to_string(), serde_json::json!(user_id));
    if event_type == "app_mention" {
        metadata.insert("was_mentioned".to_string(), serde_json::Value::Bool(true));
    }

    // Determine the real thread_ts from the event (None for top-level messages).
    let real_thread_ts = msg_data["thread_ts"]
        .as_str()
        .or_else(|| event["thread_ts"].as_str());

    let mut explicitly_mentioned = false;
    if let Some(ref bid) = *bot_user_id.read().await {
        let mention_tag = format!("<@{bid}>");
        if text.contains(&mention_tag) {
            explicitly_mentioned = true;
            metadata.insert("was_mentioned".to_string(), serde_json::json!(true));

            // Track thread for auto-reply on subsequent messages.
            if let Some(tts) = real_thread_ts {
                active_threads.insert(tts.to_string(), Instant::now());
            }
        }
    }

    // Auto-reply to follow-up messages in tracked threads.
    if !explicitly_mentioned && auto_thread_reply {
        if let Some(tts) = real_thread_ts {
            if let Some(mut entry) = active_threads.get_mut(tts) {
                // Refresh TTL and mark as mentioned so dispatch proceeds.
                *entry = Instant::now();
                metadata.insert("was_mentioned".to_string(), serde_json::json!(true));
            }
        }
    }

    Some(ChannelMessage {
        channel: ChannelType::Slack,
        platform_message_id: ts.to_string(),
        sender: ChannelUser {
            platform_id: channel.to_string(),
            display_name: user_id.to_string(), // Slack user IDs as display name
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp,
        is_group: true,
        thread_id,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_slack_event_basic() {
        let bot_id = Arc::new(RwLock::new(Some("B123".to_string())));
        let event = serde_json::json!({
            "type": "message",
            "user": "U456",
            "channel": "C789",
            "text": "Hello agent!",
            "ts": "1700000000.000100"
        });

        let msg = parse_slack_event(&event, &bot_id, &[], &Arc::new(DashMap::new()), true)
            .await
            .unwrap();
        assert_eq!(msg.channel, ChannelType::Slack);
        assert_eq!(msg.sender.platform_id, "C789");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello agent!"));
    }

    #[tokio::test]
    async fn test_parse_slack_event_filters_bot() {
        let bot_id = Arc::new(RwLock::new(Some("B123".to_string())));
        let event = serde_json::json!({
            "type": "message",
            "user": "U456",
            "channel": "C789",
            "text": "Bot message",
            "ts": "1700000000.000100",
            "bot_id": "B999"
        });

        let msg = parse_slack_event(&event, &bot_id, &[], &Arc::new(DashMap::new()), true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_slack_event_filters_own_user() {
        let bot_id = Arc::new(RwLock::new(Some("U456".to_string())));
        let event = serde_json::json!({
            "type": "message",
            "user": "U456",
            "channel": "C789",
            "text": "My message",
            "ts": "1700000000.000100"
        });

        let msg = parse_slack_event(&event, &bot_id, &[], &Arc::new(DashMap::new()), true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_slack_event_channel_filter() {
        let bot_id = Arc::new(RwLock::new(None));
        let event = serde_json::json!({
            "type": "message",
            "user": "U456",
            "channel": "C789",
            "text": "Hello",
            "ts": "1700000000.000100"
        });

        // Not in allowed channels
        let msg = parse_slack_event(
            &event,
            &bot_id,
            &["C111".to_string(), "C222".to_string()],
            &Arc::new(DashMap::new()),
            true,
        )
        .await;
        assert!(msg.is_none());

        // In allowed channels
        let msg = parse_slack_event(
            &event,
            &bot_id,
            &["C789".to_string()],
            &Arc::new(DashMap::new()),
            true,
        )
        .await;
        assert!(msg.is_some());
    }

    #[tokio::test]
    async fn test_parse_slack_event_skips_other_subtypes() {
        let bot_id = Arc::new(RwLock::new(None));
        // Non-message_changed subtypes should still be filtered
        let event = serde_json::json!({
            "type": "message",
            "subtype": "channel_join",
            "user": "U456",
            "channel": "C789",
            "text": "joined",
            "ts": "1700000000.000100"
        });

        let msg = parse_slack_event(&event, &bot_id, &[], &Arc::new(DashMap::new()), true).await;
        assert!(msg.is_none());
    }

    #[tokio::test]
    async fn test_parse_slack_command() {
        let bot_id = Arc::new(RwLock::new(None));
        let event = serde_json::json!({
            "type": "message",
            "user": "U456",
            "channel": "C789",
            "text": "/agent hello-world",
            "ts": "1700000000.000100"
        });

        let msg = parse_slack_event(&event, &bot_id, &[], &Arc::new(DashMap::new()), true)
            .await
            .unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agent");
                assert_eq!(args, &["hello-world"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_parse_slack_event_message_changed() {
        let bot_id = Arc::new(RwLock::new(Some("B123".to_string())));
        let event = serde_json::json!({
            "type": "message",
            "subtype": "message_changed",
            "channel": "C789",
            "message": {
                "user": "U456",
                "text": "Edited message text",
                "ts": "1700000000.000100"
            },
            "ts": "1700000001.000200"
        });

        let msg = parse_slack_event(&event, &bot_id, &[], &Arc::new(DashMap::new()), true)
            .await
            .unwrap();
        assert_eq!(msg.channel, ChannelType::Slack);
        assert_eq!(msg.sender.platform_id, "C789");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Edited message text"));
    }

    #[test]
    fn test_slack_adapter_creation() {
        let adapter = SlackAdapter::new(
            "xapp-test".to_string(),
            "xoxb-test".to_string(),
            vec!["C123".to_string()],
            true,
            24,
            true,
        );
        assert_eq!(adapter.name(), "slack");
        assert_eq!(adapter.channel_type(), ChannelType::Slack);
    }

    #[test]
    fn test_slack_adapter_unfurl_links_enabled() {
        let adapter = SlackAdapter::new(
            "xapp-test".to_string(),
            "xoxb-test".to_string(),
            vec![],
            true,
            24,
            true,
        );
        assert!(adapter.unfurl_links);
    }

    #[test]
    fn test_slack_adapter_unfurl_links_disabled() {
        let adapter = SlackAdapter::new(
            "xapp-test".to_string(),
            "xoxb-test".to_string(),
            vec![],
            true,
            24,
            false,
        );
        assert!(!adapter.unfurl_links);
    }

    #[test]
    fn test_envelope_dedup_skips_second_delivery() {
        // Simulates Slack redelivering the same event across a connection
        // rotation: the envelope is acked on both connections but the agent
        // must only see it once.
        let cache: DashMap<String, Instant> = DashMap::new();
        let envelope_id = "8d2e1c5a-4f3b-49a1-b6e2-7c0a9f1234ab";

        // First delivery on the old connection: not a duplicate, forward.
        assert!(
            !is_duplicate_envelope(&cache, envelope_id),
            "first sight of envelope must not be flagged as duplicate"
        );

        // Second delivery on the new connection: duplicate, skip.
        assert!(
            is_duplicate_envelope(&cache, envelope_id),
            "second sight of same envelope must be flagged as duplicate"
        );

        // Simulate the receive-loop pattern: count how many times the agent
        // would actually be invoked across two deliveries.
        let mut agent_invocations = 0;
        for _delivery in 0..2 {
            if !is_duplicate_envelope(&cache, envelope_id) {
                agent_invocations += 1;
            }
        }
        assert_eq!(
            agent_invocations, 0,
            "after initial double-delivery, no further invocations should occur within TTL"
        );
    }

    #[test]
    fn test_envelope_dedup_distinct_ids_pass_through() {
        let cache: DashMap<String, Instant> = DashMap::new();
        assert!(!is_duplicate_envelope(&cache, "envelope-a"));
        assert!(!is_duplicate_envelope(&cache, "envelope-b"));
        assert!(!is_duplicate_envelope(&cache, "envelope-c"));
        // Each unique envelope_id should be seen exactly once.
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_envelope_dedup_empty_id_never_dedupes() {
        // Defensive: malformed payloads with no envelope_id should not poison
        // the cache or short-circuit forwarding.
        let cache: DashMap<String, Instant> = DashMap::new();
        assert!(!is_duplicate_envelope(&cache, ""));
        assert!(!is_duplicate_envelope(&cache, ""));
        assert_eq!(cache.len(), 0);
    }
}
