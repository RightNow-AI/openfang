//! Nextcloud Talk channel adapter.
//!
//! Uses the Nextcloud Talk REST API (OCS v2) for sending and receiving messages.
//! Polls the chat endpoint with `lookIntoFuture=1` for near-real-time message
//! delivery. Authentication is performed via Bearer token with OCS-specific
//! headers.

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

/// Maximum message length for Nextcloud Talk messages.
const MAX_MESSAGE_LEN: usize = 32000;

/// Polling interval in seconds for the chat endpoint.
const POLL_INTERVAL_SECS: u64 = 3;

/// Nextcloud Talk channel adapter using OCS REST API with polling.
///
/// Polls the Nextcloud Talk chat endpoint for new messages and sends replies
/// via the same REST API. Supports multiple room tokens for simultaneous
/// monitoring.
pub struct NextcloudAdapter {
    /// Nextcloud server URL (e.g., `"https://cloud.example.com"`).
    server_url: String,
    /// SECURITY: Authentication token is zeroized on drop.
    token: Zeroizing<String>,
    /// Room tokens to poll (empty = discover from server).
    allowed_rooms: Vec<String>,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Last known message ID per room for incremental polling.
    last_known_ids: Arc<RwLock<HashMap<String, i64>>>,
}

impl NextcloudAdapter {
    /// Create a new Nextcloud Talk adapter.
    ///
    /// # Arguments
    /// * `server_url` - Base URL of the Nextcloud instance.
    /// * `token` - Authentication token (app password or OAuth2 token).
    /// * `allowed_rooms` - Room tokens to listen on (empty = discover joined rooms).
    pub fn new(server_url: String, token: String, allowed_rooms: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let server_url = server_url.trim_end_matches('/').to_string();
        Self {
            server_url,
            token: Zeroizing::new(token),
            allowed_rooms,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            last_known_ids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add OCS and authorization headers to a request builder.
    fn ocs_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("Authorization", format!("Bearer {}", self.token.as_str()))
            .header("OCS-APIRequest", "true")
            .header("Accept", "application/json")
    }

    /// Validate credentials by fetching the user's own status.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/ocs/v2.php/cloud/user?format=json", self.server_url);
        let resp = self.ocs_headers(self.client.get(&url)).send().await?;

        if !resp.status().is_success() {
            return Err("Nextcloud authentication failed".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let user_id = body["ocs"]["data"]["id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(user_id)
    }

    /// Fetch the list of joined rooms from the Nextcloud Talk API.
    #[allow(dead_code)]
    async fn fetch_rooms(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/ocs/v2.php/apps/spreed/api/v4/room?format=json",
            self.server_url
        );
        let resp = self.ocs_headers(self.client.get(&url)).send().await?;

        if !resp.status().is_success() {
            return Err("Nextcloud: failed to fetch rooms".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let rooms = body["ocs"]["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| r["token"].as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Ok(rooms)
    }

    /// Send a text message to a Nextcloud Talk room.
    async fn api_send_message(
        &self,
        room_token: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/ocs/v2.php/apps/spreed/api/v1/chat/{}",
            self.server_url, room_token
        );
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "message": chunk,
            });

            let resp = self
                .ocs_headers(self.client.post(&url))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Nextcloud Talk API error {status}: {body}").into());
            }
        }

        Ok(())
    }

    /// Check if a room token is in the allowed list.
    #[allow(dead_code)]
    fn is_allowed_room(&self, room_token: &str) -> bool {
        self.allowed_rooms.is_empty() || self.allowed_rooms.iter().any(|r| r == room_token)
    }
}

#[async_trait]
impl ChannelAdapter for NextcloudAdapter {
    fn name(&self) -> &str {
        "nextcloud"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("nextcloud".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let username = self.validate().await?;
        info!("Nextcloud Talk adapter authenticated as {username}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let server_url = self.server_url.clone();
        let token = self.token.clone();
        let own_user = username;
        let allowed_rooms = self.allowed_rooms.clone();
        let client = self.client.clone();
        let last_known_ids = Arc::clone(&self.last_known_ids);
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Determine rooms to poll
            let rooms_to_poll = if allowed_rooms.is_empty() {
                let url = format!(
                    "{}/ocs/v2.php/apps/spreed/api/v4/room?format=json",
                    server_url
                );
                match client
                    .get(&url)
                    .header("Authorization", format!("Bearer {}", token.as_str()))
                    .header("OCS-APIRequest", "true")
                    .header("Accept", "application/json")
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        body["ocs"]["data"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|r| r["token"].as_str().map(String::from))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    }
                    Err(e) => {
                        warn!("Nextcloud: failed to list rooms: {e}");
                        return;
                    }
                }
            } else {
                allowed_rooms
            };

            if rooms_to_poll.is_empty() {
                warn!("Nextcloud Talk: no rooms to poll");
                return;
            }

            info!("Nextcloud Talk: polling {} room(s)", rooms_to_poll.len());

            // Initialize last known IDs to 0 (server returns newest first,
            // we use lookIntoFuture to get only new messages)
            {
                let mut ids = last_known_ids.write().await;
                for room in &rooms_to_poll {
                    ids.entry(room.clone()).or_insert(0);
                }
            }

            let poll_interval = Duration::from_secs(POLL_INTERVAL_SECS);
            let mut backoff = Duration::from_secs(1);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Nextcloud Talk adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                for room_token in &rooms_to_poll {
                    let last_id = {
                        let ids = last_known_ids.read().await;
                        ids.get(room_token).copied().unwrap_or(0)
                    };

                    // Use lookIntoFuture=1 and lastKnownMessageId for incremental polling
                    let url = format!(
                        "{}/ocs/v2.php/apps/spreed/api/v4/room/{}/chat?format=json&lookIntoFuture=1&limit=100&lastKnownMessageId={}",
                        server_url, room_token, last_id
                    );

                    let resp = match client
                        .get(&url)
                        .header("Authorization", format!("Bearer {}", token.as_str()))
                        .header("OCS-APIRequest", "true")
                        .header("Accept", "application/json")
                        .timeout(Duration::from_secs(30))
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            warn!("Nextcloud: poll error for room {room_token}: {e}");
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(60));
                            continue;
                        }
                    };

                    // 304 Not Modified = no new messages
                    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
                        backoff = Duration::from_secs(1);
                        continue;
                    }

                    if !resp.status().is_success() {
                        warn!(
                            "Nextcloud: chat poll returned {} for room {room_token}",
                            resp.status()
                        );
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                        continue;
                    }

                    backoff = Duration::from_secs(1);

                    let body: serde_json::Value = match resp.json().await {
                        Ok(b) => b,
                        Err(e) => {
                            warn!("Nextcloud: failed to parse chat response: {e}");
                            continue;
                        }
                    };

                    let messages = match body["ocs"]["data"].as_array() {
                        Some(arr) => arr,
                        None => continue,
                    };

                    let mut newest_id = last_id;

                    for msg in messages {
                        // Only handle user messages (not system/command messages)
                        let msg_type = msg["messageType"].as_str().unwrap_or("comment");
                        if msg_type == "system" {
                            continue;
                        }

                        let actor_id = msg["actorId"].as_str().unwrap_or("");
                        // Skip own messages
                        if actor_id == own_user {
                            continue;
                        }

                        let text = msg["message"].as_str().unwrap_or("");
                        if text.is_empty() {
                            continue;
                        }

                        let msg_id = msg["id"].as_i64().unwrap_or(0);
                        let actor_display = msg["actorDisplayName"].as_str().unwrap_or("unknown");
                        let reference_id = msg["referenceId"].as_str().map(String::from);

                        // Track newest message ID
                        if msg_id > newest_id {
                            newest_id = msg_id;
                        }

                        let msg_content = if text.starts_with('/') {
                            let parts: Vec<&str> = text.splitn(2, ' ').collect();
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
                            ChannelContent::Text(text.to_string())
                        };

                        let channel_msg = ChannelMessage {
                            channel: ChannelType::Custom("nextcloud".to_string()),
                            platform_message_id: msg_id.to_string(),
                            sender: ChannelUser {
                                platform_id: room_token.clone(),
                                display_name: actor_display.to_string(),
                                openfang_user: None,
                            },
                            content: msg_content,
                            target_agent: None,
                            timestamp: Utc::now(),
                            is_group: true,
                            thread_id: reference_id,
                            metadata: {
                                let mut m = HashMap::new();
                                m.insert(
                                    "actor_id".to_string(),
                                    serde_json::Value::String(actor_id.to_string()),
                                );
                                m.insert(
                                    "room_token".to_string(),
                                    serde_json::Value::String(room_token.clone()),
                                );
                                m
                            },
                        };

                        if tx.send(channel_msg).await.is_err() {
                            return;
                        }
                    }

                    // Update last known message ID for this room
                    if newest_id > last_id {
                        last_known_ids
                            .write()
                            .await
                            .insert(room_token.clone(), newest_id);
                    }
                }
            }

            info!("Nextcloud Talk polling loop stopped");
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
            ChannelContent::ApprovalRequest { request_id, agent_id, tool_name, action_summary } => {
                let text = format!(
                    "⏳ 待审批\nAgent: {}\n操作: {} — {}\nID: {}\n/reject {}",
                    agent_id, tool_name, action_summary, &request_id[..8], &request_id[..8]
                );
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
        // Nextcloud Talk does not have a public typing indicator REST endpoint
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
    fn test_nextcloud_adapter_creation() {
        let adapter = NextcloudAdapter::new(
            "https://cloud.example.com".to_string(),
            "test-token".to_string(),
            vec!["room1".to_string()],
        );
        assert_eq!(adapter.name(), "nextcloud");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("nextcloud".to_string())
        );
    }

    #[test]
    fn test_nextcloud_server_url_normalization() {
        let adapter = NextcloudAdapter::new(
            "https://cloud.example.com/".to_string(),
            "tok".to_string(),
            vec![],
        );
        assert_eq!(adapter.server_url, "https://cloud.example.com");
    }

    #[test]
    fn test_nextcloud_allowed_rooms() {
        let adapter = NextcloudAdapter::new(
            "https://cloud.example.com".to_string(),
            "tok".to_string(),
            vec!["room1".to_string(), "room2".to_string()],
        );
        assert!(adapter.is_allowed_room("room1"));
        assert!(adapter.is_allowed_room("room2"));
        assert!(!adapter.is_allowed_room("room3"));

        let open = NextcloudAdapter::new(
            "https://cloud.example.com".to_string(),
            "tok".to_string(),
            vec![],
        );
        assert!(open.is_allowed_room("any-room"));
    }

    #[test]
    fn test_nextcloud_ocs_headers() {
        let adapter = NextcloudAdapter::new(
            "https://cloud.example.com".to_string(),
            "my-token".to_string(),
            vec![],
        );
        let builder = adapter.client.get("https://example.com");
        let builder = adapter.ocs_headers(builder);
        let request = builder.build().unwrap();
        assert_eq!(request.headers().get("OCS-APIRequest").unwrap(), "true");
        assert_eq!(
            request.headers().get("Authorization").unwrap(),
            "Bearer my-token"
        );
    }

    #[test]
    fn test_nextcloud_token_zeroized() {
        let adapter = NextcloudAdapter::new(
            "https://cloud.example.com".to_string(),
            "secret-token-value".to_string(),
            vec![],
        );
        assert_eq!(adapter.token.as_str(), "secret-token-value");
    }
}
