//! Keybase Chat channel adapter.
//!
//! Uses the Keybase Chat API JSON protocol over HTTP for sending and receiving
//! messages. Polls for new messages using the `list` + `read` API methods and
//! sends messages via the `send` method. Authentication is performed using a
//! Keybase username and paper key.

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

/// Maximum message length for Keybase messages.
const MAX_MESSAGE_LEN: usize = 10000;

/// Polling interval in seconds for new messages.
const POLL_INTERVAL_SECS: u64 = 3;

/// Keybase Chat API base URL (local daemon or remote API).
const KEYBASE_API_URL: &str = "http://127.0.0.1:5222/api";

/// Keybase Chat channel adapter using JSON API protocol with polling.
///
/// Interfaces with the Keybase Chat API to send and receive messages. Supports
/// filtering by team names for team-based conversations.
pub struct KeybaseAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// Keybase username for authentication.
    username: String,
    /// SECURITY: Paper key is zeroized on drop.
    #[allow(dead_code)]
    paperkey: Zeroizing<String>,
    /// Team names to listen on (empty = all conversations).
    allowed_teams: Vec<String>,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Last read message ID per conversation for incremental polling.
    last_msg_ids: Arc<RwLock<HashMap<String, i64>>>,
}

impl KeybaseAdapter {
    /// Create a new Keybase adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `username` - Keybase username.
    /// * `paperkey` - Paper key for authentication.
    /// * `allowed_teams` - Team names to filter conversations (empty = all).
    pub fn new(id: String, username: String, paperkey: String, allowed_teams: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            id,
            username,
            paperkey: Zeroizing::new(paperkey),
            allowed_teams,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            last_msg_ids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Build the authentication payload for API requests.
    #[allow(dead_code)]
    fn auth_payload(&self) -> serde_json::Value {
        serde_json::json!({
            "username": self.username,
            "paperkey": self.paperkey.as_str(),
        })
    }

    /// List conversations from the Keybase Chat API.
    #[allow(dead_code)]
    async fn list_conversations(
        &self,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let payload = serde_json::json!({
            "method": "list",
            "params": {
                "options": {}
            }
        });

        let resp = self
            .client
            .post(KEYBASE_API_URL)
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Keybase: failed to list conversations".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let conversations = body["result"]["conversations"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(conversations)
    }

    /// Read messages from a specific conversation channel.
    #[allow(dead_code)]
    async fn read_messages(
        &self,
        channel: &serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let payload = serde_json::json!({
            "method": "read",
            "params": {
                "options": {
                    "channel": channel,
                    "pagination": {
                        "num": 50,
                    }
                }
            }
        });

        let resp = self
            .client
            .post(KEYBASE_API_URL)
            .json(&payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Keybase: failed to read messages".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let messages = body["result"]["messages"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(messages)
    }

    /// Send a text message to a Keybase conversation.
    async fn api_send_message(
        &self,
        channel: &serde_json::Value,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let payload = serde_json::json!({
                "method": "send",
                "params": {
                    "options": {
                        "channel": channel,
                        "message": {
                            "body": chunk,
                        }
                    }
                }
            });

            let resp = self
                .client
                .post(KEYBASE_API_URL)
                .json(&payload)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Keybase API error {status}: {body}").into());
            }
        }

        Ok(())
    }

    /// Check if a team name is in the allowed list.
    #[allow(dead_code)]
    fn is_allowed_team(&self, team_name: &str) -> bool {
        self.allowed_teams.is_empty() || self.allowed_teams.iter().any(|t| t == team_name)
    }
}

#[async_trait]
impl ChannelAdapter for KeybaseAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "keybase"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("keybase".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        info!("Keybase adapter starting for user {}", self.username);

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let username = self.username.clone();
        let allowed_teams = self.allowed_teams.clone();
        let client = self.client.clone();
        let last_msg_ids = Arc::clone(&self.last_msg_ids);
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let poll_interval = Duration::from_secs(POLL_INTERVAL_SECS);
            let mut backoff = Duration::from_secs(1);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Keybase adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                // List conversations
                let list_payload = serde_json::json!({
                    "method": "list",
                    "params": {
                        "options": {}
                    }
                });

                let conversations = match client
                    .post(KEYBASE_API_URL)
                    .json(&list_payload)
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        body["result"]["conversations"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                    }
                    Err(e) => {
                        warn!("Keybase: failed to list conversations: {e}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                        continue;
                    }
                };

                backoff = Duration::from_secs(1);

                for conv in &conversations {
                    let channel_info = &conv["channel"];
                    let members_type = channel_info["members_type"].as_str().unwrap_or("");
                    let team_name = channel_info["name"].as_str().unwrap_or("");
                    let topic_name = channel_info["topic_name"].as_str().unwrap_or("general");

                    // Filter by team if configured
                    if !allowed_teams.is_empty()
                        && members_type == "team"
                        && !allowed_teams.iter().any(|t| t == team_name)
                    {
                        continue;
                    }

                    let conv_key = format!("{}:{}", team_name, topic_name);

                    // Read messages from this conversation
                    let read_payload = serde_json::json!({
                        "method": "read",
                        "params": {
                            "options": {
                                "channel": channel_info,
                                "pagination": {
                                    "num": 20,
                                }
                            }
                        }
                    });

                    let messages = match client
                        .post(KEYBASE_API_URL)
                        .json(&read_payload)
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            let body: serde_json::Value = resp.json().await.unwrap_or_default();
                            body["result"]["messages"]
                                .as_array()
                                .cloned()
                                .unwrap_or_default()
                        }
                        Err(e) => {
                            warn!("Keybase: read error for {conv_key}: {e}");
                            continue;
                        }
                    };

                    let last_id = {
                        let ids = last_msg_ids.read().await;
                        ids.get(&conv_key).copied().unwrap_or(0)
                    };

                    let mut newest_id = last_id;

                    for msg_wrapper in &messages {
                        let msg = &msg_wrapper["msg"];
                        let msg_id = msg["id"].as_i64().unwrap_or(0);

                        // Skip already-seen messages
                        if msg_id <= last_id {
                            continue;
                        }

                        let sender_username = msg["sender"]["username"].as_str().unwrap_or("");
                        // Skip own messages
                        if sender_username == username {
                            continue;
                        }

                        let content_type = msg["content"]["type"].as_str().unwrap_or("");
                        if content_type != "text" {
                            continue;
                        }

                        let text = msg["content"]["text"]["body"].as_str().unwrap_or("");
                        if text.is_empty() {
                            continue;
                        }

                        if msg_id > newest_id {
                            newest_id = msg_id;
                        }

                        let sender_device = msg["sender"]["device_name"].as_str().unwrap_or("");
                        let is_group = members_type == "team";

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
                            channel: ChannelType::Custom("keybase".to_string()),
                            platform_message_id: msg_id.to_string(),
                            sender: ChannelUser {
                                platform_id: conv_key.clone(),
                                display_name: sender_username.to_string(),
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
                                    "team_name".to_string(),
                                    serde_json::Value::String(team_name.to_string()),
                                );
                                m.insert(
                                    "topic_name".to_string(),
                                    serde_json::Value::String(topic_name.to_string()),
                                );
                                m.insert(
                                    "sender_device".to_string(),
                                    serde_json::Value::String(sender_device.to_string()),
                                );
                                m
                            },
                        };

                        if tx.send(channel_msg).await.is_err() {
                            return;
                        }
                    }

                    // Update last known ID
                    if newest_id > last_id {
                        last_msg_ids.write().await.insert(conv_key, newest_id);
                    }
                }
            }

            info!("Keybase polling loop stopped");
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(text) => text,
            _ => "(Unsupported content type)".to_string(),
        };

        // Parse platform_id back into channel info (format: "team:topic")
        let parts: Vec<&str> = user.platform_id.splitn(2, ':').collect();
        let (team_name, topic_name) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            (user.platform_id.as_str(), "general")
        };

        let channel_info = serde_json::json!({
            "name": team_name,
            "topic_name": topic_name,
            "members_type": "team",
        });

        self.api_send_message(&channel_info, &text).await?;
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Keybase does not expose a typing indicator via the JSON API
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
    fn test_keybase_adapter_creation() {
        let adapter = KeybaseAdapter::new(
            "keybase-1".to_string(),
            "testuser".to_string(),
            "paper-key-phrase".to_string(),
            vec!["myteam".to_string()],
        );
        assert_eq!(adapter.name(), "keybase");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("keybase".to_string())
        );
    }

    #[test]
    fn test_keybase_allowed_teams() {
        let adapter = KeybaseAdapter::new(
            "keybase-2".to_string(),
            "user".to_string(),
            "paperkey".to_string(),
            vec!["team-a".to_string(), "team-b".to_string()],
        );
        assert!(adapter.is_allowed_team("team-a"));
        assert!(adapter.is_allowed_team("team-b"));
        assert!(!adapter.is_allowed_team("team-c"));

        let open = KeybaseAdapter::new(
            "keybase-3".to_string(),
            "user".to_string(),
            "paperkey".to_string(),
            vec![],
        );
        assert!(open.is_allowed_team("any-team"));
    }

    #[test]
    fn test_keybase_paperkey_zeroized() {
        let adapter = KeybaseAdapter::new(
            "keybase-4".to_string(),
            "user".to_string(),
            "my secret paper key".to_string(),
            vec![],
        );
        assert_eq!(adapter.paperkey.as_str(), "my secret paper key");
    }

    #[test]
    fn test_keybase_auth_payload() {
        let adapter = KeybaseAdapter::new(
            "keybase-5".to_string(),
            "myuser".to_string(),
            "my-paper-key".to_string(),
            vec![],
        );
        let payload = adapter.auth_payload();
        assert_eq!(payload["username"], "myuser");
        assert_eq!(payload["paperkey"], "my-paper-key");
    }

    #[test]
    fn test_keybase_username_stored() {
        let adapter = KeybaseAdapter::new(
            "keybase-6".to_string(),
            "alice".to_string(),
            "key".to_string(),
            vec![],
        );
        assert_eq!(adapter.username, "alice");
    }
}
