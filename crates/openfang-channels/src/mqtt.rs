//! MQTT pub/sub channel adapter for the OpenFang channel bridge.
//!
//! MQTT is a lightweight messaging protocol ideal for:
//! - IoT devices and sensors
//! - Low-bandwidth/high-latency networks
//! - Real-time pub/sub messaging
//! - Integration with existing MQTT infrastructure
//!
//! This adapter subscribes to a topic for incoming messages and publishes
//! responses to a reply topic.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use openfang_types::config::MqttConfig;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};
use zeroize::Zeroizing;

/// Maximum backoff duration on connection failures.
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Initial backoff duration on connection failures.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
/// Maximum message size for MQTT (most brokers have ~256KB limit).
const MAX_MQTT_MESSAGE_SIZE: usize = 250 * 1024;

/// MQTT channel adapter using rumqttc.
pub struct MqttAdapter {
    config: MqttConfig,
    /// SECURITY: Password is zeroized on drop.
    username: Option<String>,
    /// SECURITY: Password is zeroized on drop.
    password: Option<Zeroizing<String>>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl MqttAdapter {
    /// Create a new MQTT adapter from configuration.
    ///
    /// Reads credentials from environment variables specified in the config.
    pub fn from_config(config: MqttConfig) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        // Read credentials from env (both optional)
        let username = if config.username_env.is_empty() {
            None
        } else {
            std::env::var(&config.username_env)
                .ok()
                .filter(|s| !s.is_empty())
        };

        let password = if config.password_env.is_empty() {
            None
        } else {
            std::env::var(&config.password_env)
                .ok()
                .filter(|s| !s.is_empty())
                .map(Zeroizing::new)
        };

        Self {
            config,
            username,
            password,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Generate a unique client ID if not specified.
    fn effective_client_id(&self) -> String {
        if self.config.client_id.is_empty() {
            format!("openfang-{}", uuid::Uuid::new_v4().simple())
        } else {
            self.config.client_id.clone()
        }
    }

    /// Parse broker URL into host and port.
    fn parse_broker_url(&self) -> Result<(String, u16), Box<dyn std::error::Error>> {
        let url = self.config.broker_url.trim();

        // Handle various URL formats
        if url.starts_with("tcp://") {
            let addr = url.trim_start_matches("tcp://");
            parse_host_port(addr, 1883)
        } else if url.starts_with("ssl://") || url.starts_with("mqtts://") {
            let addr = url
                .trim_start_matches("ssl://")
                .trim_start_matches("mqtts://");
            parse_host_port(addr, 8883)
        } else if url.starts_with("mqtt://") {
            let addr = url.trim_start_matches("mqtt://");
            parse_host_port(addr, 1883)
        } else {
            // Plain host:port or just host
            parse_host_port(url, 1883)
        }
    }

    /// Build rumqttc MqttOptions from configuration.
    fn build_mqtt_options(&self) -> Result<rumqttc::MqttOptions, Box<dyn std::error::Error>> {
        let (host, port) = self.parse_broker_url()?;
        let client_id = self.effective_client_id();

        let mut options = rumqttc::MqttOptions::new(client_id, host, port);
        options.set_keep_alive(Duration::from_secs(self.config.keep_alive_secs as u64));

        // Set credentials if provided
        if let Some(ref username) = self.username {
            if let Some(ref password) = self.password {
                options.set_credentials(username, password.as_str());
            } else {
                options.set_credentials(username, "");
            }
        }

        // Configure TLS if enabled
        if self.config.use_tls {
            // Use TLS with default configuration (no client auth, no ALPN)
            // ca: empty vec means use system's default CA certificates
            let transport = rumqttc::Transport::Tls(rumqttc::TlsConfiguration::Simple {
                ca: Vec::new(),
                alpn: None,
                client_auth: None,
            });
            options.set_transport(transport);
        }

        Ok(options)
    }

    /// Convert QoS config value to rumqttc QoS.
    fn qos(&self) -> rumqttc::QoS {
        match self.config.qos {
            0 => rumqttc::QoS::AtMostOnce,
            1 => rumqttc::QoS::AtLeastOnce,
            2 => rumqttc::QoS::ExactlyOnce,
            _ => rumqttc::QoS::AtLeastOnce, // Default to QoS 1
        }
    }
}

/// Parse host:port string with default port fallback.
fn parse_host_port(
    s: &str,
    default_port: u16,
) -> Result<(String, u16), Box<dyn std::error::Error>> {
    let s = s.trim();

    // Handle [IPv6]:port format
    if s.starts_with('[') {
        if let Some(close_bracket) = s.find(']') {
            let host = s[1..close_bracket].to_string();
            let rest = &s[close_bracket + 1..];
            let port = if let Some(stripped) = rest.strip_prefix(':') {
                stripped.parse()?
            } else {
                default_port
            };
            return Ok((host, port));
        }
    }

    // Handle host:port format
    if let Some(colon_pos) = s.rfind(':') {
        let host = s[..colon_pos].to_string();
        let port = s[colon_pos + 1..].parse()?;
        Ok((host, port))
    } else {
        Ok((s.to_string(), default_port))
    }
}

#[async_trait]
impl ChannelAdapter for MqttAdapter {
    fn name(&self) -> &str {
        "mqtt"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("mqtt".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let options = self.build_mqtt_options()?;
        let subscribe_topic = self.config.subscribe_topic.clone();
        let qos = self.qos();

        info!(
            "MQTT connecting to {} with client_id={}",
            self.config.broker_url,
            self.effective_client_id()
        );

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);

        let mut shutdown = self.shutdown_rx.clone();
        let publish_topic_prefix = self.config.publish_topic_prefix.clone();

        tokio::spawn(async move {
            let mut backoff = INITIAL_BACKOFF;

            loop {
                // Check shutdown before attempting connection
                if *shutdown.borrow() {
                    info!("MQTT adapter shutting down");
                    break;
                }

                // Create new client for each connection attempt
                let (client, mut eventloop) = rumqttc::AsyncClient::new(options.clone(), 10);

                // Subscribe to the topic
                if let Err(e) = client.subscribe(&subscribe_topic, qos).await {
                    error!("MQTT subscribe failed: {e}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    continue;
                }

                info!("MQTT subscribed to topic: {}", subscribe_topic);
                backoff = INITIAL_BACKOFF;

                // Event loop
                loop {
                    tokio::select! {
                        // Check for shutdown
                        _ = shutdown.changed() => {
                            if *shutdown.borrow() {
                                let _ = client.disconnect().await;
                                info!("MQTT disconnected");
                                return;
                            }
                        }

                        // Handle MQTT events
                        notification = eventloop.poll() => {
                            match notification {
                                Ok(rumqttc::Event::Incoming(rumqttc::Incoming::Publish(publish))) => {
                                    // Parse incoming message
                                    if let Some(msg) = parse_mqtt_message(&publish, &publish_topic_prefix) {
                                        debug!("MQTT message from topic {}: {:?}", publish.topic, msg.content);

                                        if tx.send(msg).await.is_err() {
                                            info!("MQTT receiver dropped, stopping");
                                            return;
                                        }
                                    }
                                }
                                Ok(rumqttc::Event::Incoming(rumqttc::Incoming::Disconnect)) => {
                                    warn!("MQTT broker sent disconnect");
                                    break;
                                }
                                Err(e) => {
                                    error!("MQTT connection error: {e}, reconnecting in {backoff:?}");
                                    tokio::time::sleep(backoff).await;
                                    backoff = (backoff * 2).min(MAX_BACKOFF);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // For MQTT, we publish to a topic based on the user's platform_id
        // The platform_id in this case is the reply topic

        let reply_topic = if user.platform_id.starts_with("topic:") {
            // Direct topic reference
            user.platform_id[6..].to_string()
        } else {
            // Use the publish topic prefix + client identifier
            format!("{}/{}", self.config.publish_topic_prefix, user.platform_id)
        };

        let payload = match content {
            ChannelContent::Text(text) => {
                // Split large messages
                let chunks = split_message(&text, MAX_MQTT_MESSAGE_SIZE);
                chunks.join("\n---\n")
            }
            ChannelContent::Image { url, caption } => {
                let cap = caption.unwrap_or_default();
                format!("📷 Image: {url}\n{cap}")
            }
            ChannelContent::File { url, filename } => {
                format!("📎 File: {filename}\n{url}")
            }
            ChannelContent::Voice {
                url,
                duration_seconds,
            } => {
                format!("🎙️ Voice ({duration_seconds}s): {url}")
            }
            ChannelContent::Location { lat, lon } => {
                format!("📍 Location: {lat}, {lon}")
            }
            ChannelContent::Command { name, args } => {
                format!("/{name} {}", args.join(" "))
            }
        };

        // Create a temporary client for publishing
        let options = self.build_mqtt_options()?;
        let (client, mut eventloop) = rumqttc::AsyncClient::new(options, 10);

        // Publish the message
        client
            .publish(&reply_topic, self.qos(), false, payload.as_bytes())
            .await?;

        // Wait for confirmation (briefly)
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Incoming::PubAck(_))) => break,
                    Ok(rumqttc::Event::Incoming(rumqttc::Incoming::PubComp(_))) => break,
                    Err(_) => break,
                    _ => {}
                }
            }
        })
        .await;

        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        info!("MQTT adapter stop signal sent");
        Ok(())
    }
}

/// Parse an MQTT publish message into a ChannelMessage.
fn parse_mqtt_message(
    publish: &rumqttc::Publish,
    _publish_topic_prefix: &str,
) -> Option<ChannelMessage> {
    // Extract payload as string
    let payload = String::from_utf8_lossy(&publish.payload);

    if payload.is_empty() {
        return None;
    }

    // Use the topic as the platform_id (for routing responses)
    // Format: topic:{original_topic} so we can reply to the same topic
    let platform_id = format!("topic:{}", publish.topic);

    // Try to extract sender info from topic or payload
    // Common patterns:
    // - device/{device_id}/status -> sender is device_id
    // - {prefix}/{client_id}/request -> sender is client_id
    let display_name =
        extract_sender_from_topic(&publish.topic).unwrap_or_else(|| "mqtt-client".to_string());

    // Parse payload - could be JSON with metadata or plain text
    let (content, metadata) = parse_payload(&payload);

    Some(ChannelMessage {
        channel: ChannelType::Custom("mqtt".to_string()),
        platform_message_id: format!("{}-{}", publish.pkid, chrono::Utc::now().timestamp_millis()),
        sender: ChannelUser {
            platform_id,
            display_name,
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group: false, // MQTT is inherently point-to-point
        thread_id: Some(publish.topic.clone()),
        metadata,
    })
}

/// Extract sender identifier from MQTT topic path.
fn extract_sender_from_topic(topic: &str) -> Option<String> {
    // Try common patterns
    let parts: Vec<&str> = topic.split('/').collect();

    // Pattern: device/{device_id}/...
    if parts.len() >= 2 && parts[0] == "device" {
        return Some(parts[1].to_string());
    }

    // Pattern: .../client/{client_id}
    for i in 0..parts.len().saturating_sub(1) {
        if parts[i] == "client" || parts[i] == "user" || parts[i] == "device" {
            return Some(parts[i + 1].to_string());
        }
    }

    // Use last segment as fallback
    parts.last().map(|s| s.to_string())
}

/// Parse payload into content and metadata.
fn parse_payload(payload: &str) -> (ChannelContent, HashMap<String, serde_json::Value>) {
    // Try to parse as JSON first
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        // If it's a JSON object, extract fields
        if let Some(obj) = json.as_object() {
            // Extract message text
            let text = obj
                .get("message")
                .or_else(|| obj.get("text"))
                .or_else(|| obj.get("payload"))
                .and_then(|v| v.as_str())
                .unwrap_or(payload);

            // Extract metadata
            let metadata: HashMap<String, serde_json::Value> = obj
                .iter()
                .filter(|(k, _)| !["message", "text", "payload"].contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            return (ChannelContent::Text(text.to_string()), metadata);
        }
    }

    // Plain text
    (ChannelContent::Text(payload.to_string()), HashMap::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_host_port() {
        assert_eq!(
            parse_host_port("localhost:1883", 1883).unwrap(),
            ("localhost".to_string(), 1883)
        );
        assert_eq!(
            parse_host_port("broker.hivemq.com", 1883).unwrap(),
            ("broker.hivemq.com".to_string(), 1883)
        );
        assert_eq!(
            parse_host_port("192.168.1.1:8883", 1883).unwrap(),
            ("192.168.1.1".to_string(), 8883)
        );
    }

    #[test]
    fn test_parse_host_port_ipv6() {
        assert_eq!(
            parse_host_port("[::1]:1883", 1883).unwrap(),
            ("::1".to_string(), 1883)
        );
        assert_eq!(
            parse_host_port("[2001:db8::1]:8883", 1883).unwrap(),
            ("2001:db8::1".to_string(), 8883)
        );
    }

    #[test]
    fn test_extract_sender_from_topic() {
        assert_eq!(
            extract_sender_from_topic("device/sensor-01/status"),
            Some("sensor-01".to_string())
        );
        assert_eq!(
            extract_sender_from_topic("home/livingroom/temperature"),
            Some("temperature".to_string())
        );
        assert_eq!(
            extract_sender_from_topic("client/robot-123/command"),
            Some("robot-123".to_string())
        );
    }

    #[test]
    fn test_parse_payload_plain_text() {
        let (content, metadata) = parse_payload("Hello, world!");
        assert!(matches!(content, ChannelContent::Text(ref t) if t == "Hello, world!"));
        assert!(metadata.is_empty());
    }

    #[test]
    fn test_parse_payload_json() {
        let json = r#"{"message": "Hello", "device_id": "sensor-01", "temperature": 25.5}"#;
        let (content, metadata) = parse_payload(json);

        assert!(matches!(content, ChannelContent::Text(ref t) if t == "Hello"));
        assert_eq!(
            metadata.get("device_id").and_then(|v| v.as_str()),
            Some("sensor-01")
        );
        assert_eq!(
            metadata.get("temperature").and_then(|v| v.as_f64()),
            Some(25.5)
        );
    }

    #[test]
    fn test_mqtt_config_default() {
        let config = MqttConfig::default();
        assert_eq!(config.broker_url, "tcp://localhost:1883");
        assert_eq!(config.subscribe_topic, "openfang/inbox");
        assert_eq!(config.qos, 1);
        assert!(!config.use_tls);
    }

    #[test]
    fn test_qos_conversion() {
        let config = MqttConfig {
            qos: 0,
            ..Default::default()
        };
        let adapter = MqttAdapter::from_config(config);
        assert!(matches!(adapter.qos(), rumqttc::QoS::AtMostOnce));

        let config = MqttConfig {
            qos: 1,
            ..Default::default()
        };
        let adapter = MqttAdapter::from_config(config);
        assert!(matches!(adapter.qos(), rumqttc::QoS::AtLeastOnce));

        let config = MqttConfig {
            qos: 2,
            ..Default::default()
        };
        let adapter = MqttAdapter::from_config(config);
        assert!(matches!(adapter.qos(), rumqttc::QoS::ExactlyOnce));

        let config = MqttConfig {
            qos: 99,
            ..Default::default()
        };
        let adapter = MqttAdapter::from_config(config);
        assert!(matches!(adapter.qos(), rumqttc::QoS::AtLeastOnce));
    }
}
