//! Webex Bot channel adapter.
//!
//! Connects to the Webex platform via the Mercury WebSocket for receiving
//! real-time message events and uses the Webex REST API for sending messages.
//! Authentication is performed via a Bot Bearer token. Supports room filtering
//! and automatic WebSocket reconnection.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{info, warn};
use zeroize::Zeroizing;

/// Webex REST API base URL.
const WEBEX_API_BASE: &str = "https://webexapis.com/v1";

/// Webex Mercury WebSocket URL for device connections.
const WEBEX_WS_URL: &str = "wss://mercury-connection-a.wbx2.com/v1/apps/wx2/registrations";

/// Maximum message length for Webex (official limit is 7439 characters).
const MAX_MESSAGE_LEN: usize = 7439;

/// Webex Bot channel adapter using WebSocket for events and REST for sending.
///
/// Connects to the Webex Mercury WebSocket gateway for real-time message
/// notifications and fetches full message content via the REST API. Outbound
/// messages are sent directly via the REST API.
pub struct WebexAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// SECURITY: Bot token is zeroized on drop.
    bot_token: Zeroizing<String>,
    /// Room IDs to listen on (empty = all rooms the bot is in).
    allowed_rooms: Vec<String>,
    /// HTTP client for REST API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Cached bot identity (ID and display name).
    bot_info: Arc<RwLock<Option<(String, String)>>>,
}

impl WebexAdapter {
    /// Create a new Webex adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `bot_token` - Webex Bot access token.
    /// * `allowed_rooms` - Room IDs to filter events for (empty = all).
    pub fn new(id: String, bot_token: String, allowed_rooms: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            id,
            bot_token: Zeroizing::new(bot_token),
            allowed_rooms,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            bot_info: Arc::new(RwLock::new(None)),
        }
    }

    /// Validate credentials and retrieve bot identity.
    async fn validate(&self) -> Result<(String, String), Box<dyn std::error::Error>> {
        let url = format!("{}/people/me", WEBEX_API_BASE);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.bot_token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Webex authentication failed".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let bot_id = body["id"].as_str().unwrap_or("unknown").to_string();
        let display_name = body["displayName"]
            .as_str()
            .unwrap_or("OpenFang Bot")
            .to_string();

        *self.bot_info.write().await = Some((bot_id.clone(), display_name.clone()));

        Ok((bot_id, display_name))
    }

    /// Fetch the full message content by ID (Mercury events only include activity data).
    #[allow(dead_code)]
    async fn get_message(
        &self,
        message_id: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let url = format!("{}/messages/{}", WEBEX_API_BASE, message_id);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.bot_token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(format!("Webex: failed to get message {message_id}: {status}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        Ok(body)
    }

    /// Register a webhook for receiving message events (alternative to WebSocket).
    #[allow(dead_code)]
    async fn register_webhook(
        &self,
        target_url: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/webhooks", WEBEX_API_BASE);
        let body = serde_json::json!({
            "name": "OpenFang Bot Webhook",
            "targetUrl": target_url,
            "resource": "messages",
            "event": "created",
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(self.bot_token.as_str())
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!("Webex webhook registration failed {status}: {resp_body}").into());
        }

        let result: serde_json::Value = resp.json().await?;
        let webhook_id = result["id"].as_str().unwrap_or("unknown").to_string();
        Ok(webhook_id)
    }

    /// Send a text message to a Webex room.
    async fn api_send_message(
        &self,
        room_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/messages", WEBEX_API_BASE);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "roomId": room_id,
                "text": chunk,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.bot_token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Webex API error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }

    /// Send a direct message to a person by email or person ID.
    #[allow(dead_code)]
    async fn api_send_direct(
        &self,
        person_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/messages", WEBEX_API_BASE);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = if person_id.contains('@') {
                serde_json::json!({
                    "toPersonEmail": person_id,
                    "text": chunk,
                })
            } else {
                serde_json::json!({
                    "toPersonId": person_id,
                    "text": chunk,
                })
            };

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.bot_token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Webex direct message error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }

    /// Check if a room ID is in the allowed list.
    #[allow(dead_code)]
    fn is_allowed_room(&self, room_id: &str) -> bool {
        self.allowed_rooms.is_empty() || self.allowed_rooms.iter().any(|r| r == room_id)
    }
}

#[async_trait]
impl ChannelAdapter for WebexAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "webex"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("webex".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials and get bot identity
        let (bot_id, bot_name) = self.validate().await?;
        info!("Webex adapter authenticated as {bot_name} ({bot_id})");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let bot_token = self.bot_token.clone();
        let allowed_rooms = self.allowed_rooms.clone();
        let client = self.client.clone();
        let own_bot_id = bot_id;
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                // Attempt WebSocket connection to Mercury
                let mut request =
                    match tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(WEBEX_WS_URL) {
                        Ok(r) => r,
                        Err(e) => {
                            warn!("Webex: failed to build WS request: {e}");
                            return;
                        }
                    };

                request.headers_mut().insert(
                    "Authorization",
                    format!("Bearer {}", bot_token.as_str()).parse().unwrap(),
                );

                let ws_stream = match tokio_tungstenite::connect_async(request).await {
                    Ok((stream, _resp)) => stream,
                    Err(e) => {
                        warn!("Webex: WebSocket connection failed: {e}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                        continue;
                    }
                };

                info!("Webex Mercury WebSocket connected");
                backoff = Duration::from_secs(1);

                use futures::StreamExt;
                let (_write, mut read) = ws_stream.split();

                let should_reconnect = loop {
                    let msg = tokio::select! {
                        _ = shutdown_rx.changed() => {
                            info!("Webex adapter shutting down");
                            return;
                        }
                        msg = read.next() => msg,
                    };

                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            warn!("Webex WS read error: {e}");
                            break true;
                        }
                        None => {
                            info!("Webex WS stream ended");
                            break true;
                        }
                    };

                    let text = match msg {
                        tokio_tungstenite::tungstenite::Message::Text(t) => t,
                        tokio_tungstenite::tungstenite::Message::Close(_) => {
                            break true;
                        }
                        _ => continue,
                    };

                    let event: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Mercury events have a data.activity structure
                    let activity = &event["data"]["activity"];
                    let verb = activity["verb"].as_str().unwrap_or("");

                    // Only process "post" activities (new messages)
                    if verb != "post" {
                        continue;
                    }

                    let actor_id = activity["actor"]["id"].as_str().unwrap_or("");
                    // Skip messages from the bot itself
                    if actor_id == own_bot_id {
                        continue;
                    }

                    let message_id = activity["object"]["id"].as_str().unwrap_or("");
                    if message_id.is_empty() {
                        continue;
                    }

                    let room_id = activity["target"]["id"].as_str().unwrap_or("").to_string();

                    // Filter by room if configured
                    if !allowed_rooms.is_empty() && !allowed_rooms.iter().any(|r| r == &room_id) {
                        continue;
                    }

                    // Fetch full message content via REST API
                    let msg_url = format!("{}/messages/{}", WEBEX_API_BASE, message_id);
                    let full_msg = match client
                        .get(&msg_url)
                        .bearer_auth(bot_token.as_str())
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            if !resp.status().is_success() {
                                warn!("Webex: failed to fetch message {message_id}");
                                continue;
                            }
                            resp.json::<serde_json::Value>().await.unwrap_or_default()
                        }
                        Err(e) => {
                            warn!("Webex: message fetch error: {e}");
                            continue;
                        }
                    };

                    let msg_text = full_msg["text"].as_str().unwrap_or("");
                    if msg_text.is_empty() {
                        continue;
                    }

                    let sender_email = full_msg["personEmail"].as_str().unwrap_or("unknown");
                    let sender_id = full_msg["personId"].as_str().unwrap_or("").to_string();
                    let full_room_id = full_msg["roomId"].as_str().unwrap_or(&room_id).to_string();
                    let room_type = full_msg["roomType"].as_str().unwrap_or("group");
                    let is_group = room_type == "group";

                    let msg_content = if msg_text.starts_with('/') {
                        let parts: Vec<&str> = msg_text.splitn(2, ' ').collect();
                        let cmd = parts[0].trim_start_matches('/');
                        let args: Vec<String> = parts
                            .get(1)
                            .map(|a| a.split_whitespace().map(String::from).collect())
                            .unwrap_or_default();
                        ChannelContent::Command {
                            name: cmd.to_string(),
                            args,
                        }
                    } else {
                        ChannelContent::Text(msg_text.to_string())
                    };

                    let channel_msg = ChannelMessage {
                        channel: ChannelType::Custom("webex".to_string()),
                        platform_message_id: message_id.to_string(),
                        sender: ChannelUser {
                            platform_id: full_room_id,
                            display_name: sender_email.to_string(),
                            openfang_user: None,
                        },
                        content: msg_content,
                        target_agent: None,
                        timestamp: Utc::now(),
                        is_group,
                        thread_id: None,
                        metadata: {
                            let mut m = HashMap::new();
                            m.insert(
                                "sender_id".to_string(),
                                serde_json::Value::String(sender_id),
                            );
                            m.insert(
                                "sender_email".to_string(),
                                serde_json::Value::String(sender_email.to_string()),
                            );
                            m
                        },
                    };

                    if tx.send(channel_msg).await.is_err() {
                        return;
                    }
                };

                if !should_reconnect || *shutdown_rx.borrow() {
                    break;
                }

                warn!("Webex: reconnecting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }

            info!("Webex WebSocket loop stopped");
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(&user.platform_id, &text).await?;
            }
            _ => {
                self.api_send_message(&user.platform_id, "(Unsupported content type)")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Webex does not expose a public typing indicator API for bots
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webex_adapter_creation() {
        let adapter = WebexAdapter::new("test-bot-token".to_string(), vec!["room1".to_string()]);
        assert_eq!(adapter.name(), "webex");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("webex".to_string())
        );
    }

    #[test]
    fn test_webex_allowed_rooms() {
        let adapter = WebexAdapter::new(
            "tok".to_string(),
            vec!["room-a".to_string(), "room-b".to_string()],
        );
        assert!(adapter.is_allowed_room("room-a"));
        assert!(adapter.is_allowed_room("room-b"));
        assert!(!adapter.is_allowed_room("room-c"));

        let open = WebexAdapter::new("tok".to_string(), vec![]);
        assert!(open.is_allowed_room("any-room"));
    }

    #[test]
    fn test_webex_token_zeroized() {
        let adapter = WebexAdapter::new("my-secret-bot-token".to_string(), vec![]);
        assert_eq!(adapter.bot_token.as_str(), "my-secret-bot-token");
    }

    #[test]
    fn test_webex_message_length_limit() {
        assert_eq!(MAX_MESSAGE_LEN, 7439);
    }

    #[test]
    fn test_webex_constants() {
        assert!(WEBEX_API_BASE.starts_with("https://"));
        assert!(WEBEX_WS_URL.starts_with("wss://"));
    }
}
