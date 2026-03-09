//! Slack Socket Mode adapter for the OpenFang channel bridge.
//!
//! Uses Slack Socket Mode WebSocket (app token) for receiving events and the
//! Web API (bot token) for sending responses. No external Slack crate.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use futures::{SinkExt, Stream, StreamExt};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{debug, error, info, warn};
use zeroize::Zeroizing;

const SLACK_API_BASE: &str = "https://slack.com/api";
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const SLACK_MSG_LIMIT: usize = 3000;

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
}

impl SlackAdapter {
    pub fn new(app_token: String, bot_token: String, allowed_channels: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_token: Zeroizing::new(app_token),
            bot_token: Zeroizing::new(bot_token),
            client: reqwest::Client::new(),
            allowed_channels,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_user_id: Arc::new(RwLock::new(None)),
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chunks = split_message(text, SLACK_MSG_LIMIT);

        for chunk in chunks {
            let body = serde_json::json!({
                "channel": channel_id,
                "text": chunk,
            });

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

                            // Extract the event
                            let event = &payload["payload"]["event"];
                            if let Some(msg) =
                                parse_slack_event(event, &bot_user_id, &allowed_channels).await
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
                self.api_send_message(channel_id, &text).await?;
            }
            _ => {
                self.api_send_message(channel_id, "(Unsupported content type)")
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
) -> Option<ChannelMessage> {
    let event_type = event["type"].as_str()?;
    if event_type != "message" {
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

    // Filter by allowed channels (DMs are exempt — dm_policy handles those)
    if !channel.starts_with('D')
        && !allowed_channels.is_empty()
        && !allowed_channels.contains(&channel.to_string())
    {
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

    // Slack channel prefixes: C=public, G=private/mpim, D=1:1 DM, W=org-wide.
    // Only D channels are true DMs where dm_policy should apply.
    let is_group = !channel.starts_with('D');

    let mut metadata = HashMap::new();
    metadata.insert(
        "sender_user_id".to_string(),
        serde_json::Value::String(user_id.to_string()),
    );

    Some(ChannelMessage {
        channel: ChannelType::Slack,
        platform_message_id: ts.to_string(),
        sender: ChannelUser {
            platform_id: channel.to_string(),
            display_name: user_id.to_string(),
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp,
        is_group,
        thread_id: None,
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

        let msg = parse_slack_event(&event, &bot_id, &[]).await.unwrap();
        assert_eq!(msg.channel, ChannelType::Slack);
        assert_eq!(msg.sender.platform_id, "C789");
        assert_eq!(
            msg.metadata.get("sender_user_id").and_then(|v| v.as_str()),
            Some("U456")
        );
        assert!(msg.is_group);
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

        let msg = parse_slack_event(&event, &bot_id, &[]).await;
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

        let msg = parse_slack_event(&event, &bot_id, &[]).await;
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
        let msg =
            parse_slack_event(&event, &bot_id, &["C111".to_string(), "C222".to_string()]).await;
        assert!(msg.is_none());

        // In allowed channels
        let msg = parse_slack_event(&event, &bot_id, &["C789".to_string()]).await;
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

        let msg = parse_slack_event(&event, &bot_id, &[]).await;
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

        let msg = parse_slack_event(&event, &bot_id, &[]).await.unwrap();
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

        let msg = parse_slack_event(&event, &bot_id, &[]).await.unwrap();
        assert_eq!(msg.channel, ChannelType::Slack);
        assert_eq!(msg.sender.platform_id, "C789");
        assert_eq!(
            msg.metadata.get("sender_user_id").and_then(|v| v.as_str()),
            Some("U456")
        );
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Edited message text"));
    }

    #[tokio::test]
    async fn test_parse_slack_event_dm_detected() {
        let bot_id = Arc::new(RwLock::new(Some("B123".to_string())));
        let event = serde_json::json!({
            "type": "message",
            "user": "U456",
            "channel": "D789",
            "text": "Hello via DM",
            "ts": "1700000000.000100"
        });

        let msg = parse_slack_event(&event, &bot_id, &[]).await.unwrap();
        assert!(!msg.is_group);
        assert_eq!(msg.sender.platform_id, "D789");
        assert_eq!(
            msg.metadata.get("sender_user_id").and_then(|v| v.as_str()),
            Some("U456")
        );
    }

    #[test]
    fn test_slack_adapter_creation() {
        let adapter = SlackAdapter::new(
            "xapp-test".to_string(),
            "xoxb-test".to_string(),
            vec!["C123".to_string()],
        );
        assert_eq!(adapter.name(), "slack");
        assert_eq!(adapter.channel_type(), ChannelType::Slack);
    }
}
