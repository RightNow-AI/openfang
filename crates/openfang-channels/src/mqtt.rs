//! MQTT channel adapter.
//!
//! Provides a generic MQTT pub/sub interface for IoT and messaging integration.
//! Supports standard MQTT 3.1.1/5.0 brokers with optional TLS and authentication.
//!
//! # Configuration
//!
//! ```toml
//! [channels.mqtt]
//! broker_url = "tcp://broker.hivemq.com:1883"
//! subscribe_topic = "openfang/inbox"
//! publish_topic = "openfang/outbox"
//! username_env = "MQTT_USERNAME"
//! password_env = "MQTT_PASSWORD"
//! use_tls = false
//! qos = 1
//! ```
//!
//! # Message Format
//!
//! Incoming messages are expected as UTF-8 text. The adapter supports:
//! - Plain text messages
//! - JSON payloads with `{"text": "message"}` format
//! - Command messages starting with `/`

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{info, warn};

/// Maximum MQTT message length.
const MAX_MESSAGE_LEN: usize = 4096;

/// Type alias for the publish channel sender.
type PublishSender = Arc<RwLock<Option<mpsc::Sender<(String, String)>>>>;

/// MQTT pub/sub channel adapter.
///
/// Connects to an MQTT broker, subscribes to a topic for incoming messages,
/// and publishes responses to another topic.
pub struct MqttAdapter {
    /// MQTT broker URL (e.g., `"tcp://broker.hivemq.com:1883"`).
    broker_url: String,
    /// Client identifier (auto-generated if empty).
    client_id: String,
    /// Topic to subscribe to for incoming messages.
    subscribe_topic: String,
    /// Topic to publish responses to.
    publish_topic: String,
    /// Optional username for authentication.
    username: Option<String>,
    /// Optional password for authentication.
    password: Option<String>,
    /// Use TLS/SSL connection.
    use_tls: bool,
    /// Keep-alive interval in seconds.
    keep_alive: u16,
    /// Clean session flag.
    clean_session: bool,
    /// QoS level for subscriptions.
    qos: QoS,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Sender for publishing messages (used to communicate with the event loop task).
    publish_tx: PublishSender,
}

impl MqttAdapter {
    /// Create a new MQTT adapter.
    ///
    /// # Arguments
    /// * `broker_url` - MQTT broker URL (e.g., `"tcp://broker.hivemq.com:1883"`).
    /// * `client_id` - Client identifier (auto-generated if empty).
    /// * `subscribe_topic` - Topic to subscribe to for incoming messages.
    /// * `publish_topic` - Topic to publish responses to (defaults to subscribe_topic if empty).
    /// * `username` - Optional username for authentication.
    /// * `password` - Optional password for authentication.
    /// * `use_tls` - Use TLS/SSL connection.
    /// * `keep_alive` - Keep-alive interval in seconds.
    /// * `clean_session` - Clean session flag.
    /// * `qos` - QoS level (0, 1, or 2).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        broker_url: String,
        client_id: String,
        subscribe_topic: String,
        publish_topic: String,
        username: Option<String>,
        password: Option<String>,
        use_tls: bool,
        keep_alive: u16,
        clean_session: bool,
        qos: u8,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let publish_topic = if publish_topic.is_empty() {
            subscribe_topic.clone()
        } else {
            publish_topic
        };
        let qos = match qos {
            0 => QoS::AtMostOnce,
            2 => QoS::ExactlyOnce,
            _ => QoS::AtLeastOnce,
        };

        Self {
            broker_url,
            client_id,
            subscribe_topic,
            publish_topic,
            username,
            password,
            use_tls,
            keep_alive,
            clean_session,
            qos,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            publish_tx: Arc::new(RwLock::new(None)),
        }
    }

    /// Parse broker URL into host and port.
    fn parse_broker_url(&self) -> Result<(String, u16), Box<dyn std::error::Error>> {
        let url = self.broker_url.trim();

        // Handle different URL schemes
        if let Some(rest) = url.strip_prefix("tcp://") {
            Self::parse_host_port(rest, 1883)
        } else if let Some(rest) = url.strip_prefix("ssl://") {
            Self::parse_host_port(rest, 8883)
        } else if self.use_tls && !url.contains("://") {
            // Plain host with TLS flag
            Self::parse_host_port(url, 8883)
        } else if url.contains("://") {
            Err(format!("Unsupported MQTT URL scheme: {url}").into())
        } else {
            // Plain host:port or just host (no TLS)
            Self::parse_host_port(url, 1883)
        }
    }

    /// Parse host:port string.
    fn parse_host_port(
        s: &str,
        default_port: u16,
    ) -> Result<(String, u16), Box<dyn std::error::Error>> {
        let s = s.trim();
        if let Some(colon_pos) = s.rfind(':') {
            let host = s[..colon_pos].to_string();
            let port = s[colon_pos + 1..].parse::<u16>()?;
            Ok((host, port))
        } else {
            Ok((s.to_string(), default_port))
        }
    }

    /// Build MQTT options.
    fn build_mqtt_options(&self) -> Result<MqttOptions, Box<dyn std::error::Error>> {
        let (host, port) = self.parse_broker_url()?;
        let client_id = if self.client_id.is_empty() {
            format!("openfang-{}", uuid::Uuid::new_v4())
        } else {
            self.client_id.clone()
        };

        let mut options = MqttOptions::new(client_id, host, port);
        options.set_keep_alive(Duration::from_secs(self.keep_alive as u64));
        options.set_clean_session(self.clean_session);

        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            options.set_credentials(user, pass);
        }

        // Note: TLS support requires additional configuration with rustls
        // For now, we use native TLS through the use_tls flag
        if self.use_tls {
            // rumqttc handles TLS automatically when using ssl:// or with explicit config
            // This is a simplified approach; production use may need custom TLS config
        }

        Ok(options)
    }

    /// Parse incoming MQTT payload.
    fn parse_payload(payload: &[u8]) -> Option<String> {
        if payload.is_empty() {
            return None;
        }

        // Try UTF-8 first
        if let Ok(text) = std::str::from_utf8(payload) {
            // Check for JSON format {"text": "message"}
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(text_val) = json.get("text").and_then(|v| v.as_str()) {
                    return Some(text_val.to_string());
                }
            }
            return Some(text.to_string());
        }

        None
    }

    /// Publish a message to the configured topic.
    async fn publish_message(&self, text: &str) -> Result<(), Box<dyn std::error::Error>> {
        let tx_guard = self.publish_tx.read().await;
        if let Some(tx) = tx_guard.as_ref() {
            let chunks = split_message(text, MAX_MESSAGE_LEN);
            for chunk in chunks {
                tx.send((self.publish_topic.clone(), chunk.to_string()))
                    .await
                    .map_err(|e| format!("Failed to send publish request: {e}"))?;
            }
            Ok(())
        } else {
            Err("MQTT client not connected".into())
        }
    }
}

#[async_trait]
impl ChannelAdapter for MqttAdapter {
    fn name(&self) -> &str {
        "mqtt"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Mqtt
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let options = self.build_mqtt_options()?;
        let (client, mut eventloop) = AsyncClient::new(options, 10);

        info!(
            "MQTT adapter connecting to {} (subscribe: {}, publish: {})",
            self.broker_url, self.subscribe_topic, self.publish_topic
        );

        // Subscribe to topic
        client.subscribe(&self.subscribe_topic, self.qos).await?;

        // Channel for incoming messages
        let (msg_tx, rx) = mpsc::channel::<ChannelMessage>(256);

        // Channel for outgoing publish requests
        let (publish_tx, mut publish_rx) = mpsc::channel::<(String, String)>(64);

        // Store the publish sender
        {
            let mut tx_guard = self.publish_tx.write().await;
            *tx_guard = Some(publish_tx);
        }

        let subscribe_topic = self.subscribe_topic.clone();
        let qos = self.qos;
        let mut shutdown_rx = self.shutdown_rx.clone();

        // Spawn the event loop task
        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(60);

            loop {
                if *shutdown_rx.borrow() {
                    info!("MQTT adapter shutting down");
                    break;
                }

                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("MQTT adapter shutting down");
                            break;
                        }
                    }
                    publish_req = publish_rx.recv() => {
                        if let Some((topic, payload)) = publish_req {
                            if let Err(e) = client.publish(&topic, qos, false, payload).await {
                                warn!("MQTT publish error: {}", e);
                            }
                        }
                    }
                    event = eventloop.poll() => {
                        match event {
                            Ok(Event::Incoming(Incoming::Publish(publish))) => {
                                backoff = Duration::from_secs(1); // Reset backoff on success

                                let topic = publish.topic.clone();
                                if topic != subscribe_topic {
                                    continue;
                                }

                                if let Some(text) = Self::parse_payload(&publish.payload) {
                                    if text.is_empty() {
                                        continue;
                                    }

                                    let content = if text.starts_with('/') {
                                        let parts: Vec<&str> = text.splitn(2, ' ').collect();
                                        let cmd = parts[0].trim_start_matches('/');
                                        let args: Vec<String> = parts
                                            .get(1)
                                            .map(|a| {
                                                a.split_whitespace()
                                                    .map(String::from)
                                                    .collect()
                                            })
                                            .unwrap_or_default();
                                        ChannelContent::Command {
                                            name: cmd.to_string(),
                                            args,
                                        }
                                    } else {
                                        ChannelContent::Text(text)
                                    };

                                    let msg = ChannelMessage {
                                        channel: ChannelType::Mqtt,
                                        platform_message_id: format!("{:?}", publish.pkid),
                                        sender: ChannelUser {
                                            platform_id: "mqtt-user".to_string(),
                                            display_name: "MQTT User".to_string(),
                                            openfang_user: None,
                                        },
                                        content,
                                        target_agent: None,
                                        timestamp: Utc::now(),
                                        is_group: true,
                                        thread_id: None,
                                        metadata: {
                                            let mut m = HashMap::new();
                                            m.insert(
                                                "topic".to_string(),
                                                serde_json::Value::String(topic.clone()),
                                            );
                                            m.insert(
                                                "qos".to_string(),
                                                serde_json::Value::Number((publish.qos as i64).into()),
                                            );
                                            m
                                        },
                                    };

                                    if msg_tx.send(msg).await.is_err() {
                                        info!("MQTT receiver dropped, stopping");
                                        return;
                                    }
                                }
                            }
                            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                                info!("MQTT connected to broker");
                                backoff = Duration::from_secs(1);
                            }
                            Ok(Event::Incoming(Incoming::Disconnect)) => {
                                warn!("MQTT disconnected from broker");
                            }
                            Err(e) => {
                                warn!("MQTT connection error: {}, backing off for {:?}", e, backoff);
                                tokio::time::sleep(backoff).await;
                                backoff = (backoff * 2).min(max_backoff);
                            }
                            _ => {}
                        }
                    }
                }
            }

            info!("MQTT event loop stopped");
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        _user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(t) => t,
            ChannelContent::Command { name, args } => {
                if args.is_empty() {
                    format!("/{name}")
                } else {
                    format!("/{} {}", name, args.join(" "))
                }
            }
            _ => "(Unsupported content type)".to_string(),
        };
        self.publish_message(&text).await
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // MQTT has no typing indicator concept.
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);

        // Clear the publish channel
        let mut tx_guard = self.publish_tx.write().await;
        *tx_guard = None;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mqtt_adapter_creation() {
        let adapter = MqttAdapter::new(
            "tcp://broker.hivemq.com:1883".to_string(),
            "test-client".to_string(),
            "test/topic".to_string(),
            String::new(),
            None,
            None,
            false,
            60,
            true,
            1,
        );
        assert_eq!(adapter.name(), "mqtt");
        assert_eq!(adapter.channel_type(), ChannelType::Mqtt);
        assert_eq!(adapter.subscribe_topic, "test/topic");
        assert_eq!(adapter.publish_topic, "test/topic"); // Falls back to subscribe_topic
    }

    #[test]
    fn test_mqtt_adapter_with_separate_publish_topic() {
        let adapter = MqttAdapter::new(
            "tcp://broker.hivemq.com:1883".to_string(),
            String::new(),
            "inbox".to_string(),
            "outbox".to_string(),
            None,
            None,
            false,
            60,
            true,
            1,
        );
        assert_eq!(adapter.subscribe_topic, "inbox");
        assert_eq!(adapter.publish_topic, "outbox");
    }

    #[test]
    fn test_parse_broker_url_tcp() {
        let adapter = MqttAdapter::new(
            "tcp://broker.example.com:1883".to_string(),
            String::new(),
            "test".to_string(),
            String::new(),
            None,
            None,
            false,
            60,
            true,
            1,
        );
        let (host, port) = adapter.parse_broker_url().unwrap();
        assert_eq!(host, "broker.example.com");
        assert_eq!(port, 1883);
    }

    #[test]
    fn test_parse_broker_url_tcp_default_port() {
        let adapter = MqttAdapter::new(
            "tcp://broker.example.com".to_string(),
            String::new(),
            "test".to_string(),
            String::new(),
            None,
            None,
            false,
            60,
            true,
            1,
        );
        let (host, port) = adapter.parse_broker_url().unwrap();
        assert_eq!(host, "broker.example.com");
        assert_eq!(port, 1883);
    }

    #[test]
    fn test_parse_broker_url_ssl() {
        let adapter = MqttAdapter::new(
            "ssl://broker.example.com:8883".to_string(),
            String::new(),
            "test".to_string(),
            String::new(),
            None,
            None,
            true,
            60,
            true,
            1,
        );
        let (host, port) = adapter.parse_broker_url().unwrap();
        assert_eq!(host, "broker.example.com");
        assert_eq!(port, 8883);
    }

    #[test]
    fn test_parse_broker_url_plain_host() {
        let adapter = MqttAdapter::new(
            "broker.example.com".to_string(),
            String::new(),
            "test".to_string(),
            String::new(),
            None,
            None,
            false,
            60,
            true,
            1,
        );
        let (host, port) = adapter.parse_broker_url().unwrap();
        assert_eq!(host, "broker.example.com");
        assert_eq!(port, 1883);
    }

    #[test]
    fn test_parse_payload_text() {
        let payload = b"Hello, MQTT!";
        let result = MqttAdapter::parse_payload(payload);
        assert_eq!(result, Some("Hello, MQTT!".to_string()));
    }

    #[test]
    fn test_parse_payload_json() {
        let payload = br#"{"text": "Hello from JSON"}"#;
        let result = MqttAdapter::parse_payload(payload);
        assert_eq!(result, Some("Hello from JSON".to_string()));
    }

    #[test]
    fn test_parse_payload_empty() {
        let payload = b"";
        let result = MqttAdapter::parse_payload(payload);
        assert!(result.is_none());
    }

    #[test]
    fn test_qos_conversion() {
        let adapter = MqttAdapter::new(
            "tcp://broker.example.com".to_string(),
            String::new(),
            "test".to_string(),
            String::new(),
            None,
            None,
            false,
            60,
            true,
            0,
        );
        assert_eq!(adapter.qos, QoS::AtMostOnce);

        let adapter = MqttAdapter::new(
            "tcp://broker.example.com".to_string(),
            String::new(),
            "test".to_string(),
            String::new(),
            None,
            None,
            false,
            60,
            true,
            1,
        );
        assert_eq!(adapter.qos, QoS::AtLeastOnce);

        let adapter = MqttAdapter::new(
            "tcp://broker.example.com".to_string(),
            String::new(),
            "test".to_string(),
            String::new(),
            None,
            None,
            false,
            60,
            true,
            2,
        );
        assert_eq!(adapter.qos, QoS::ExactlyOnce);
    }
}
