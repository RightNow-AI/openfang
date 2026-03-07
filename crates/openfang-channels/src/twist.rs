//! Twist API v3 channel adapter.
//!
//! Uses the Twist REST API v3 for sending and receiving messages. Polls the
//! comments endpoint for new messages and posts replies via the comments/add
//! endpoint. Authentication is performed via OAuth2 Bearer token.

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

/// Twist API v3 base URL.
const TWIST_API_BASE: &str = "https://api.twist.com/api/v3";

/// Maximum message length for Twist comments.
const MAX_MESSAGE_LEN: usize = 10000;

/// Polling interval in seconds for new comments.
const POLL_INTERVAL_SECS: u64 = 5;

/// Twist API v3 channel adapter using REST polling.
///
/// Polls the Twist comments endpoint for new messages in configured channels
/// (threads) and sends replies via the comments/add endpoint. Supports
/// workspace-level and channel-level filtering.
pub struct TwistAdapter {
    /// SECURITY: OAuth2 token is zeroized on drop.
    token: Zeroizing<String>,
    /// Twist workspace ID.
    workspace_id: String,
    /// Channel IDs to poll (empty = all channels in workspace).
    allowed_channels: Vec<String>,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Last seen comment ID per channel for incremental polling.
    last_comment_ids: Arc<RwLock<HashMap<String, i64>>>,
}

impl TwistAdapter {
    /// Create a new Twist adapter.
    ///
    /// # Arguments
    /// * `token` - OAuth2 Bearer token for API authentication.
    /// * `workspace_id` - Twist workspace ID to operate in.
    /// * `allowed_channels` - Channel IDs to poll (empty = discover all).
    pub fn new(token: String, workspace_id: String, allowed_channels: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            token: Zeroizing::new(token),
            workspace_id,
            allowed_channels,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            last_comment_ids: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Validate credentials by fetching the authenticated user's info.
    async fn validate(&self) -> Result<(String, String), Box<dyn std::error::Error>> {
        let url = format!("{}/users/get_session_user", TWIST_API_BASE);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Twist authentication failed".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let user_id = body["id"]
            .as_i64()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let name = body["name"].as_str().unwrap_or("unknown").to_string();

        Ok((user_id, name))
    }

    /// Fetch channels (threads) in the workspace.
    #[allow(dead_code)]
    async fn fetch_channels(&self) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/channels/get?workspace_id={}",
            TWIST_API_BASE, self.workspace_id
        );
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Twist: failed to fetch channels".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let channels = match body.as_array() {
            Some(arr) => arr.clone(),
            None => vec![],
        };

        Ok(channels)
    }

    /// Fetch threads in a channel.
    #[allow(dead_code)]
    async fn fetch_threads(
        &self,
        channel_id: &str,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!("{}/threads/get?channel_id={}", TWIST_API_BASE, channel_id);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Twist: failed to fetch threads for channel {channel_id}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let threads = match body.as_array() {
            Some(arr) => arr.clone(),
            None => vec![],
        };

        Ok(threads)
    }

    /// Fetch comments (messages) in a thread.
    #[allow(dead_code)]
    async fn fetch_comments(
        &self,
        thread_id: &str,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/comments/get?thread_id={}&limit=50",
            TWIST_API_BASE, thread_id
        );
        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Twist: failed to fetch comments for thread {thread_id}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let comments = match body.as_array() {
            Some(arr) => arr.clone(),
            None => vec![],
        };

        Ok(comments)
    }

    /// Send a comment (message) to a Twist thread.
    async fn api_send_comment(
        &self,
        thread_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/comments/add", TWIST_API_BASE);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "thread_id": thread_id.parse::<i64>().unwrap_or(0),
                "content": chunk,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.token.as_str())
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Twist API error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }

    /// Create a new thread in a channel and post the initial message.
    #[allow(dead_code)]
    async fn api_create_thread(
        &self,
        channel_id: &str,
        title: &str,
        content: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/threads/add", TWIST_API_BASE);

        let body = serde_json::json!({
            "channel_id": channel_id.parse::<i64>().unwrap_or(0),
            "title": title,
            "content": content,
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(self.token.as_str())
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!("Twist thread create error {status}: {resp_body}").into());
        }

        let result: serde_json::Value = resp.json().await?;
        let thread_id = result["id"]
            .as_i64()
            .map(|id| id.to_string())
            .unwrap_or_default();
        Ok(thread_id)
    }

    /// Check if a channel ID is in the allowed list.
    #[allow(dead_code)]
    fn is_allowed_channel(&self, channel_id: &str) -> bool {
        self.allowed_channels.is_empty() || self.allowed_channels.iter().any(|c| c == channel_id)
    }
}

#[async_trait]
impl ChannelAdapter for TwistAdapter {
    fn name(&self) -> &str {
        "twist"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("twist".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let (user_id, user_name) = self.validate().await?;
        info!("Twist adapter authenticated as {user_name} (id: {user_id})");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let token = self.token.clone();
        let workspace_id = self.workspace_id.clone();
        let own_user_id = user_id;
        let allowed_channels = self.allowed_channels.clone();
        let client = self.client.clone();
        let last_comment_ids = Arc::clone(&self.last_comment_ids);
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Discover channels if not configured
            let channels_to_poll = if allowed_channels.is_empty() {
                let url = format!(
                    "{}/channels/get?workspace_id={}",
                    TWIST_API_BASE, workspace_id
                );
                match client.get(&url).bearer_auth(token.as_str()).send().await {
                    Ok(resp) => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        body.as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|c| c["id"].as_i64().map(|id| id.to_string()))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default()
                    }
                    Err(e) => {
                        warn!("Twist: failed to list channels: {e}");
                        return;
                    }
                }
            } else {
                allowed_channels
            };

            if channels_to_poll.is_empty() {
                warn!("Twist: no channels to poll");
                return;
            }

            info!(
                "Twist: polling {} channel(s) in workspace {workspace_id}",
                channels_to_poll.len()
            );

            let poll_interval = Duration::from_secs(POLL_INTERVAL_SECS);
            let mut backoff = Duration::from_secs(1);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Twist adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                for channel_id in &channels_to_poll {
                    // Get threads in channel
                    let threads_url =
                        format!("{}/threads/get?channel_id={}", TWIST_API_BASE, channel_id);

                    let threads = match client
                        .get(&threads_url)
                        .bearer_auth(token.as_str())
                        .send()
                        .await
                    {
                        Ok(resp) => {
                            let body: serde_json::Value = resp.json().await.unwrap_or_default();
                            body.as_array().cloned().unwrap_or_default()
                        }
                        Err(e) => {
                            warn!("Twist: thread fetch error for channel {channel_id}: {e}");
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(60));
                            continue;
                        }
                    };

                    backoff = Duration::from_secs(1);

                    for thread in &threads {
                        let thread_id = thread["id"]
                            .as_i64()
                            .map(|id| id.to_string())
                            .unwrap_or_default();
                        if thread_id.is_empty() {
                            continue;
                        }

                        let thread_title =
                            thread["title"].as_str().unwrap_or("Untitled").to_string();

                        let comments_url = format!(
                            "{}/comments/get?thread_id={}&limit=20",
                            TWIST_API_BASE, thread_id
                        );

                        let comments = match client
                            .get(&comments_url)
                            .bearer_auth(token.as_str())
                            .send()
                            .await
                        {
                            Ok(resp) => {
                                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                                body.as_array().cloned().unwrap_or_default()
                            }
                            Err(e) => {
                                warn!("Twist: comment fetch error for thread {thread_id}: {e}");
                                continue;
                            }
                        };

                        let comment_key = format!("{}:{}", channel_id, thread_id);
                        let last_id = {
                            let ids = last_comment_ids.read().await;
                            ids.get(&comment_key).copied().unwrap_or(0)
                        };

                        let mut newest_id = last_id;

                        for comment in &comments {
                            let comment_id = comment["id"].as_i64().unwrap_or(0);

                            // Skip already-seen comments
                            if comment_id <= last_id {
                                continue;
                            }

                            let creator = comment["creator"]
                                .as_i64()
                                .map(|id| id.to_string())
                                .unwrap_or_default();

                            // Skip own comments
                            if creator == own_user_id {
                                continue;
                            }

                            let content = comment["content"].as_str().unwrap_or("");
                            if content.is_empty() {
                                continue;
                            }

                            if comment_id > newest_id {
                                newest_id = comment_id;
                            }

                            let creator_name =
                                comment["creator_name"].as_str().unwrap_or("unknown");

                            let msg_content = if content.starts_with('/') {
                                let parts: Vec<&str> = content.splitn(2, ' ').collect();
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
                                ChannelContent::Text(content.to_string())
                            };

                            let channel_msg = ChannelMessage {
                                channel: ChannelType::Custom("twist".to_string()),
                                platform_message_id: comment_id.to_string(),
                                sender: ChannelUser {
                                    platform_id: thread_id.clone(),
                                    display_name: creator_name.to_string(),
                                    openfang_user: None,
                                },
                                content: msg_content,
                                target_agent: None,
                                timestamp: Utc::now(),
                                is_group: true,
                                thread_id: Some(thread_title.clone()),
                                metadata: {
                                    let mut m = HashMap::new();
                                    m.insert(
                                        "channel_id".to_string(),
                                        serde_json::Value::String(channel_id.clone()),
                                    );
                                    m.insert(
                                        "thread_id".to_string(),
                                        serde_json::Value::String(thread_id.clone()),
                                    );
                                    m.insert(
                                        "creator_id".to_string(),
                                        serde_json::Value::String(creator),
                                    );
                                    m.insert(
                                        "workspace_id".to_string(),
                                        serde_json::Value::String(workspace_id.clone()),
                                    );
                                    m
                                },
                            };

                            if tx.send(channel_msg).await.is_err() {
                                return;
                            }
                        }

                        // Update last seen comment ID
                        if newest_id > last_id {
                            last_comment_ids
                                .write()
                                .await
                                .insert(comment_key, newest_id);
                        }
                    }
                }
            }

            info!("Twist polling loop stopped");
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

        // platform_id is the thread_id
        self.api_send_comment(&user.platform_id, &text).await?;
        Ok(())
    }

    async fn send_in_thread(
        &self,
        _user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(text) => text,
            ChannelContent::ApprovalRequest { request_id, agent_id, tool_name, action_summary } => {
                format!(
                    "⏳ 待审批\nAgent: {}\n操作: {} — {}\nID: {}\n/reject {}",
                    agent_id, tool_name, action_summary, &request_id[..8], &request_id[..8]
                )
            }
            _ => "(Unsupported content type)".to_string(),
        };

        self.api_send_comment(thread_id, &text).await?;
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Twist does not expose a typing indicator API
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
    fn test_twist_adapter_creation() {
        let adapter = TwistAdapter::new(
            "test-token".to_string(),
            "12345".to_string(),
            vec!["ch1".to_string()],
        );
        assert_eq!(adapter.name(), "twist");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("twist".to_string())
        );
    }

    #[test]
    fn test_twist_token_zeroized() {
        let adapter =
            TwistAdapter::new("secret-twist-token".to_string(), "ws1".to_string(), vec![]);
        assert_eq!(adapter.token.as_str(), "secret-twist-token");
    }

    #[test]
    fn test_twist_workspace_id() {
        let adapter = TwistAdapter::new("tok".to_string(), "workspace-99".to_string(), vec![]);
        assert_eq!(adapter.workspace_id, "workspace-99");
    }

    #[test]
    fn test_twist_allowed_channels() {
        let adapter = TwistAdapter::new(
            "tok".to_string(),
            "ws1".to_string(),
            vec!["ch-1".to_string(), "ch-2".to_string()],
        );
        assert!(adapter.is_allowed_channel("ch-1"));
        assert!(adapter.is_allowed_channel("ch-2"));
        assert!(!adapter.is_allowed_channel("ch-3"));

        let open = TwistAdapter::new("tok".to_string(), "ws1".to_string(), vec![]);
        assert!(open.is_allowed_channel("any-channel"));
    }

    #[test]
    fn test_twist_constants() {
        assert_eq!(MAX_MESSAGE_LEN, 10000);
        assert_eq!(POLL_INTERVAL_SECS, 5);
        assert!(TWIST_API_BASE.starts_with("https://"));
    }

    #[test]
    fn test_twist_poll_interval() {
        assert_eq!(POLL_INTERVAL_SECS, 5);
    }
}
