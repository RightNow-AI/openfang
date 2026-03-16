//! Signal channel adapter.
//!
//! Uses signal-cli's JSON-RPC daemon mode for sending/receiving messages.
//! Requires signal-cli to be installed and registered with a phone number.

use crate::types::{ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{debug, info};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Signal adapter via signal-cli REST API.
pub struct SignalAdapter {
    /// URL of signal-cli REST API (e.g., "http://localhost:8080").
    api_url: String,
    /// Registered phone number.
    phone_number: String,
    /// HTTP client.
    client: reqwest::Client,
    /// Allowed phone numbers (empty = allow all).
    allowed_users: Vec<String>,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl SignalAdapter {
    /// Create a new Signal adapter.
    pub fn new(api_url: String, phone_number: String, allowed_users: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            api_url,
            phone_number,
            client: reqwest::Client::new(),
            allowed_users,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Send a message via signal-cli REST API.
    async fn api_send_message(
        &self,
        recipient: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/v2/send", self.api_url);

        let body = serde_json::json!({
            "message": text,
            "number": self.phone_number,
            "recipients": [recipient],
        });

        let resp = self.client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Signal API error {status}: {body}").into());
        }

        Ok(())
    }

    /// Receive messages from signal-cli REST API.
    #[allow(dead_code)]
    async fn receive_messages(&self) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!("{}/v1/receive/{}", self.api_url, self.phone_number);

        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let messages: Vec<serde_json::Value> = resp.json().await.unwrap_or_default();
        Ok(messages)
    }

    #[allow(dead_code)]
    fn is_allowed(&self, phone: &str) -> bool {
        self.allowed_users.is_empty() || self.allowed_users.iter().any(|u| u == phone)
    }
}

#[async_trait]
impl ChannelAdapter for SignalAdapter {
    fn name(&self) -> &str {
        "signal"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Signal
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let api_url = self.api_url.clone();
        let phone_number = self.phone_number.clone();
        let allowed_users = self.allowed_users.clone();
        let client = self.client.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        info!(
            "Starting Signal adapter (polling {} every {:?})",
            api_url, POLL_INTERVAL
        );

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Signal adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(POLL_INTERVAL) => {}
                }

                // Poll for new messages
                let url = format!("{}/v1/receive/{}", api_url, phone_number);
                let resp = match client.get(&url).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        debug!("Signal poll error: {e}");
                        continue;
                    }
                };

                if !resp.status().is_success() {
                    continue;
                }

                let messages: Vec<serde_json::Value> = match resp.json().await {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                for msg in messages {
                    let envelope = msg.get("envelope").unwrap_or(&msg);

                    let source = envelope["source"].as_str().unwrap_or("").to_string();

                    if source.is_empty() || source == phone_number {
                        continue;
                    }

                    if !allowed_users.is_empty() && !allowed_users.iter().any(|u| u == &source) {
                        continue;
                    }

                    // Extract text from dataMessage
                    let text = envelope["dataMessage"]["message"].as_str().unwrap_or("");

                    if text.is_empty() {
                        continue;
                    }

                    let source_name = envelope["sourceName"]
                        .as_str()
                        .unwrap_or(&source)
                        .to_string();

                    let content = if text.starts_with('/') {
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
                        channel: ChannelType::Signal,
                        platform_message_id: envelope["timestamp"]
                            .as_u64()
                            .unwrap_or(0)
                            .to_string(),
                        sender: ChannelUser {
                            platform_id: source.clone(),
                            display_name: source_name,
                            openfang_user: None,
            metadata: None,
                        },
                        content,
                        target_agent: None,
                        timestamp: Utc::now(),
                        is_group: false,
                        thread_id: None,
                        metadata: HashMap::new(),
                    };

                    if tx.send(channel_msg).await.is_err() {
                        break;
                    }
                }
            }
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

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_adapter_creation() {
        let adapter = SignalAdapter::new(
            "http://localhost:8080".to_string(),
            "+1234567890".to_string(),
            vec![],
        );
        assert_eq!(adapter.name(), "signal");
        assert_eq!(adapter.channel_type(), ChannelType::Signal);
    }

    #[test]
    fn test_signal_allowed_check() {
        let adapter = SignalAdapter::new(
            "http://localhost:8080".to_string(),
            "+1234567890".to_string(),
            vec!["+9876543210".to_string()],
        );
        assert!(adapter.is_allowed("+9876543210"));
        assert!(!adapter.is_allowed("+1111111111"));
    }
}
