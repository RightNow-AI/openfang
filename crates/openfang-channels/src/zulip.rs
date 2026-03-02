//! Zulip channel adapter.
//!
//! Uses the Zulip REST API with HTTP Basic authentication (bot email + API key).
//! Receives messages via Zulip's event queue system (register + long-poll) and
//! sends messages via the `/api/v1/messages` endpoint.

use crate::supervisor::{self, SupervisorConfig};
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

const MAX_MESSAGE_LEN: usize = 10000;
const POLL_TIMEOUT_SECS: u64 = 60;

/// Zulip channel adapter using REST API with event queue long-polling.
pub struct ZulipAdapter {
    /// Zulip server URL (e.g., `"https://myorg.zulipchat.com"`).
    server_url: String,
    /// Bot email address for HTTP Basic auth.
    bot_email: String,
    /// SECURITY: API key is zeroized on drop.
    api_key: Zeroizing<String>,
    /// Stream names to listen on (empty = all).
    streams: Vec<String>,
    /// HTTP client.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Current event queue ID for resuming polls.
    queue_id: Arc<RwLock<Option<String>>>,
}

impl ZulipAdapter {
    /// Create a new Zulip adapter.
    ///
    /// # Arguments
    /// * `server_url` - Base URL of the Zulip server.
    /// * `bot_email` - Email address of the Zulip bot.
    /// * `api_key` - API key for the bot.
    /// * `streams` - Stream names to subscribe to (empty = all public streams).
    pub fn new(
        server_url: String,
        bot_email: String,
        api_key: String,
        streams: Vec<String>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let server_url = server_url.trim_end_matches('/').to_string();
        Self {
            server_url,
            bot_email,
            api_key: Zeroizing::new(api_key),
            streams,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            queue_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Register an event queue with the Zulip server.
    async fn register_queue(&self) -> Result<(String, i64), Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/register", self.server_url);

        let mut params = vec![("event_types", r#"["message"]"#.to_string())];

        // If specific streams are configured, narrow to those
        if !self.streams.is_empty() {
            let narrow: Vec<serde_json::Value> = self
                .streams
                .iter()
                .map(|s| serde_json::json!(["stream", s]))
                .collect();
            params.push(("narrow", serde_json::to_string(&narrow)?));
        }

        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.bot_email, Some(self.api_key.as_str()))
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Zulip register failed {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;

        let queue_id = body["queue_id"]
            .as_str()
            .ok_or("Missing queue_id in register response")?
            .to_string();
        let last_event_id = body["last_event_id"]
            .as_i64()
            .ok_or("Missing last_event_id in register response")?;

        Ok((queue_id, last_event_id))
    }

    /// Validate credentials by fetching the bot's own profile.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/users/me", self.server_url);
        let resp = self
            .client
            .get(&url)
            .basic_auth(&self.bot_email, Some(self.api_key.as_str()))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Zulip authentication failed".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let full_name = body["full_name"].as_str().unwrap_or("unknown").to_string();
        Ok(full_name)
    }

    /// Send a message to a Zulip stream or direct message.
    async fn api_send_message(
        &self,
        msg_type: &str,
        to: &str,
        topic: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/messages", self.server_url);
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let mut params = vec![
                ("type", msg_type.to_string()),
                ("to", to.to_string()),
                ("content", chunk.to_string()),
            ];

            if msg_type == "stream" {
                params.push(("topic", topic.to_string()));
            }

            let resp = self
                .client
                .post(&url)
                .basic_auth(&self.bot_email, Some(self.api_key.as_str()))
                .form(&params)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Zulip send error {status}: {body}").into());
            }
        }

        Ok(())
    }

    /// Check if a stream name is in the allowed list.
    #[allow(dead_code)]
    fn is_allowed_stream(&self, stream: &str) -> bool {
        self.streams.is_empty() || self.streams.iter().any(|s| s == stream)
    }
}

/// Run the Zulip event queue long-polling loop.
async fn run_zulip_events(
    server_url: &str,
    bot_email: &str,
    api_key: &Zeroizing<String>,
    streams: &[String],
    client: &reqwest::Client,
    queue_id_lock: &Arc<RwLock<Option<String>>>,
    tx: &mpsc::Sender<ChannelMessage>,
    shutdown_rx: &mut watch::Receiver<bool>,
    current_queue_id: &Arc<RwLock<String>>,
    last_event_id: &Arc<RwLock<i64>>,
) -> Result<bool, String> {
    loop {
        let qid = current_queue_id.read().await.clone();
        let eid = *last_event_id.read().await;
        let url = format!(
            "{}/api/v1/events?queue_id={}&last_event_id={}&dont_block=false",
            server_url, qid, eid
        );

        let resp = tokio::select! {
            _ = shutdown_rx.changed() => {
                info!("Zulip adapter shutting down");
                return Ok(false);
            }
            result = client
                .get(&url)
                .basic_auth(bot_email, Some(api_key.as_str()))
                .timeout(Duration::from_secs(POLL_TIMEOUT_SECS + 10))
                .send() => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(format!("Zulip poll error: {e}"));
                    }
                }
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            warn!("Zulip poll returned {status}");

            // If the queue is expired (BAD_EVENT_QUEUE_ID), re-register
            if status == reqwest::StatusCode::BAD_REQUEST {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                if body["code"].as_str() == Some("BAD_EVENT_QUEUE_ID") {
                    info!("Zulip: event queue expired, re-registering");
                    let register_url = format!("{}/api/v1/register", server_url);

                    let mut params = vec![("event_types", r#"["message"]"#.to_string())];
                    if !streams.is_empty() {
                        let narrow: Vec<serde_json::Value> = streams
                            .iter()
                            .map(|s| serde_json::json!(["stream", s]))
                            .collect();
                        if let Ok(narrow_str) = serde_json::to_string(&narrow) {
                            params.push(("narrow", narrow_str));
                        }
                    }

                    match client
                        .post(&register_url)
                        .basic_auth(bot_email, Some(api_key.as_str()))
                        .form(&params)
                        .send()
                        .await
                    {
                        Ok(reg_resp) => {
                            let reg_body: serde_json::Value =
                                reg_resp.json().await.unwrap_or_default();
                            if let (Some(new_qid), Some(lid)) = (
                                reg_body["queue_id"].as_str(),
                                reg_body["last_event_id"].as_i64(),
                            ) {
                                *current_queue_id.write().await = new_qid.to_string();
                                *last_event_id.write().await = lid;
                                *queue_id_lock.write().await =
                                    Some(new_qid.to_string());
                                info!("Zulip: re-registered queue {new_qid}");
                                continue;
                            }
                        }
                        Err(e) => {
                            warn!("Zulip: re-register failed: {e}");
                        }
                    }
                }
            }

            return Err(format!("Zulip poll returned {status}"));
        }

        let body: serde_json::Value = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                warn!("Zulip: failed to parse events: {e}");
                continue;
            }
        };

        let events = match body["events"].as_array() {
            Some(arr) => arr,
            None => continue,
        };

        for event in events {
            // Update last_event_id
            if let Some(event_id) = event["id"].as_i64() {
                let mut eid_guard = last_event_id.write().await;
                if event_id > *eid_guard {
                    *eid_guard = event_id;
                }
            }

            let event_type = event["type"].as_str().unwrap_or("");
            if event_type != "message" {
                continue;
            }

            let message = &event["message"];
            let msg_type = message["type"].as_str().unwrap_or("");

            // Filter by stream if configured
            let stream_name = message["display_recipient"].as_str().unwrap_or("");
            if msg_type == "stream"
                && !streams.is_empty()
                && !streams.iter().any(|s| s == stream_name)
            {
                continue;
            }

            // Skip messages from the bot itself
            let sender_email = message["sender_email"].as_str().unwrap_or("");
            if sender_email == bot_email {
                continue;
            }

            let content = message["content"].as_str().unwrap_or("");
            if content.is_empty() {
                continue;
            }

            let sender_name = message["sender_full_name"].as_str().unwrap_or("unknown");
            let sender_id = message["sender_id"]
                .as_i64()
                .map(|id| id.to_string())
                .unwrap_or_default();
            let msg_id = message["id"]
                .as_i64()
                .map(|id| id.to_string())
                .unwrap_or_default();
            let topic = message["subject"].as_str().unwrap_or("").to_string();
            let is_group = msg_type == "stream";

            let platform_id = if is_group {
                stream_name.to_string()
            } else {
                sender_email.to_string()
            };

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
                channel: ChannelType::Custom("zulip".to_string()),
                platform_message_id: msg_id,
                sender: ChannelUser {
                    platform_id,
                    display_name: sender_name.to_string(),
                    openfang_user: None,
                },
                content: msg_content,
                target_agent: None,
                timestamp: Utc::now(),
                is_group,
                thread_id: if !topic.is_empty() { Some(topic) } else { None },
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
                return Ok(false);
            }
        }
    }
}

#[async_trait]
impl ChannelAdapter for ZulipAdapter {
    fn name(&self) -> &str {
        "zulip"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("zulip".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let bot_name = self.validate().await?;
        info!("Zulip adapter authenticated as {bot_name}");

        // Register event queue
        let (initial_queue_id, initial_last_id) = self.register_queue().await?;
        info!("Zulip event queue registered: {initial_queue_id}");
        *self.queue_id.write().await = Some(initial_queue_id.clone());

        let (tx, rx) = mpsc::channel::<ChannelMessage>(supervisor::DEFAULT_CHANNEL_BUFFER);
        let server_url = self.server_url.clone();
        let bot_email = self.bot_email.clone();
        let api_key = self.api_key.clone();
        let streams = self.streams.clone();
        let client = self.client.clone();
        let queue_id_lock = Arc::clone(&self.queue_id);
        let shutdown_rx = self.shutdown_rx.clone();

        let current_queue_id = Arc::new(RwLock::new(initial_queue_id));
        let last_event_id = Arc::new(RwLock::new(initial_last_id));

        tokio::spawn(async move {
            supervisor::run_supervised_loop_reset_on_connect(
                SupervisorConfig::new("Zulip"),
                shutdown_rx.clone(),
                || {
                    let server_url = server_url.clone();
                    let bot_email = bot_email.clone();
                    let api_key = api_key.clone();
                    let streams = streams.clone();
                    let client = client.clone();
                    let queue_id_lock = queue_id_lock.clone();
                    let tx = tx.clone();
                    let mut shutdown_rx = shutdown_rx.clone();
                    let current_queue_id = current_queue_id.clone();
                    let last_event_id = last_event_id.clone();
                    async move {
                        run_zulip_events(
                            &server_url, &bot_email, &api_key, &streams,
                            &client, &queue_id_lock, &tx, &mut shutdown_rx,
                            &current_queue_id, &last_event_id,
                        ).await
                    }
                },
            ).await;
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

        // Determine message type based on platform_id format
        // If it looks like an email, send as direct; otherwise as stream message
        if user.platform_id.contains('@') {
            self.api_send_message("direct", &user.platform_id, "", &text)
                .await?;
        } else {
            // Use the thread_id (topic) if available, otherwise default topic
            let topic = "OpenFang";
            self.api_send_message("stream", &user.platform_id, topic, &text)
                .await?;
        }

        Ok(())
    }

    async fn send_in_thread(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(text) => text,
            _ => "(Unsupported content type)".to_string(),
        };

        // thread_id maps to Zulip "topic"
        self.api_send_message("stream", &user.platform_id, thread_id, &text)
            .await?;
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
    fn test_zulip_adapter_creation() {
        let adapter = ZulipAdapter::new(
            "https://myorg.zulipchat.com".to_string(),
            "bot@myorg.zulipchat.com".to_string(),
            "test-api-key".to_string(),
            vec!["general".to_string()],
        );
        assert_eq!(adapter.name(), "zulip");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("zulip".to_string())
        );
    }

    #[test]
    fn test_zulip_server_url_normalization() {
        let adapter = ZulipAdapter::new(
            "https://myorg.zulipchat.com/".to_string(),
            "bot@example.com".to_string(),
            "key".to_string(),
            vec![],
        );
        assert_eq!(adapter.server_url, "https://myorg.zulipchat.com");
    }

    #[test]
    fn test_zulip_allowed_streams() {
        let adapter = ZulipAdapter::new(
            "https://zulip.example.com".to_string(),
            "bot@example.com".to_string(),
            "key".to_string(),
            vec!["general".to_string(), "dev".to_string()],
        );
        assert!(adapter.is_allowed_stream("general"));
        assert!(adapter.is_allowed_stream("dev"));
        assert!(!adapter.is_allowed_stream("random"));

        let open = ZulipAdapter::new(
            "https://zulip.example.com".to_string(),
            "bot@example.com".to_string(),
            "key".to_string(),
            vec![],
        );
        assert!(open.is_allowed_stream("any-stream"));
    }

    #[test]
    fn test_zulip_bot_email_stored() {
        let adapter = ZulipAdapter::new(
            "https://zulip.example.com".to_string(),
            "mybot@zulip.example.com".to_string(),
            "secret-key".to_string(),
            vec![],
        );
        assert_eq!(adapter.bot_email, "mybot@zulip.example.com");
    }

    #[test]
    fn test_zulip_api_key_zeroized() {
        let adapter = ZulipAdapter::new(
            "https://zulip.example.com".to_string(),
            "bot@example.com".to_string(),
            "my-secret-api-key".to_string(),
            vec![],
        );
        // Verify the key is accessible (it will be zeroized on drop)
        assert_eq!(adapter.api_key.as_str(), "my-secret-api-key");
    }
}
