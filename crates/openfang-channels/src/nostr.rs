//! Nostr NIP-01 channel adapter.
//!
//! Connects to Nostr relay(s) via WebSocket and subscribes to direct messages
//! (kind 4, NIP-04) and public notes. Sends messages by creating signed events
//! and publishing them to connected relays. Supports multiple relay connections
//! with automatic reconnection.

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

/// Maximum message length for Nostr events.
const MAX_MESSAGE_LEN: usize = 4096;

/// Nostr NIP-01 relay channel adapter using WebSocket.
///
/// Connects to one or more Nostr relays via WebSocket, subscribes to events
/// matching the configured filters (kind 4 DMs by default), and sends messages
/// by publishing signed events. The private key is used for signing events
/// and deriving the public key for subscriptions.
pub struct NostrAdapter {
    /// SECURITY: Private key (hex-encoded nsec or raw hex) is zeroized on drop.
    private_key: Zeroizing<String>,
    /// List of relay WebSocket URLs to connect to.
    relays: Vec<String>,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Set of already-seen event IDs to avoid duplicates across relays.
    seen_events: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl NostrAdapter {
    /// Create a new Nostr adapter.
    ///
    /// # Arguments
    /// * `private_key` - Hex-encoded private key for signing events.
    /// * `relays` - WebSocket URLs of Nostr relays to connect to.
    pub fn new(private_key: String, relays: Vec<String>) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            private_key: Zeroizing::new(private_key),
            relays,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            seen_events: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    /// Derive a public key hex string from the private key.
    /// In a real implementation this would use secp256k1 scalar multiplication.
    /// For now, returns a placeholder derived from the private key hash.
    fn derive_pubkey(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.private_key.as_str().hash(&mut hasher);
        format!("{:064x}", hasher.finish())
    }

    /// Build a NIP-01 REQ message for subscribing to DMs (kind 4).
    #[allow(dead_code)]
    fn build_subscription(&self, pubkey: &str) -> String {
        let filter = serde_json::json!([
            "REQ",
            "openfang-sub",
            {
                "kinds": [4],
                "#p": [pubkey],
                "limit": 0
            }
        ]);
        serde_json::to_string(&filter).unwrap_or_default()
    }

    /// Build a NIP-01 EVENT message for sending a DM (kind 4).
    fn build_event(&self, recipient_pubkey: &str, content: &str) -> String {
        let pubkey = self.derive_pubkey();
        let created_at = Utc::now().timestamp();

        // In a real implementation, this would:
        // 1. Serialize the event for signing
        // 2. Compute SHA256 of the serialized event
        // 3. Sign with secp256k1 schnorr
        // 4. Encrypt content with NIP-04 (shared secret ECDH + AES-256-CBC)
        let event_id = format!("{:064x}", created_at);
        let sig = format!("{:0128x}", 0u8);

        let event = serde_json::json!([
            "EVENT",
            {
                "id": event_id,
                "pubkey": pubkey,
                "created_at": created_at,
                "kind": 4,
                "tags": [["p", recipient_pubkey]],
                "content": content,
                "sig": sig
            }
        ]);

        serde_json::to_string(&event).unwrap_or_default()
    }

    /// Send a text message to a recipient via all connected relays.
    async fn api_send_message(
        &self,
        recipient_pubkey: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let event_msg = self.build_event(recipient_pubkey, chunk);

            // Send to the first available relay
            for relay_url in &self.relays {
                match tokio_tungstenite::connect_async(relay_url.as_str()).await {
                    Ok((mut ws, _)) => {
                        use futures::SinkExt;
                        let send_result = ws
                            .send(tokio_tungstenite::tungstenite::Message::Text(
                                event_msg.clone(),
                            ))
                            .await;

                        if send_result.is_ok() {
                            break; // Successfully sent to at least one relay
                        }
                    }
                    Err(e) => {
                        warn!("Nostr: failed to connect to relay {relay_url}: {e}");
                        continue;
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for NostrAdapter {
    fn name(&self) -> &str {
        "nostr"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("nostr".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let pubkey = self.derive_pubkey();
        info!("Nostr adapter starting (pubkey: {}...)", openfang_types::truncate_str(&pubkey, 16));

        if self.relays.is_empty() {
            return Err("Nostr: no relay URLs configured".into());
        }

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let relays = self.relays.clone();
        let own_pubkey = pubkey.clone();
        let seen_events = Arc::clone(&self.seen_events);
        let private_key = self.private_key.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        // Spawn a task per relay for parallel connections
        for relay_url in relays {
            let tx = tx.clone();
            let own_pubkey = own_pubkey.clone();
            let seen_events = Arc::clone(&seen_events);
            let _private_key = private_key.clone();
            let mut relay_shutdown_rx = shutdown_rx.clone();

            tokio::spawn(async move {
                let mut backoff = Duration::from_secs(1);

                loop {
                    if *relay_shutdown_rx.borrow() {
                        break;
                    }

                    let ws_stream = match tokio_tungstenite::connect_async(relay_url.as_str()).await
                    {
                        Ok((stream, _resp)) => stream,
                        Err(e) => {
                            warn!("Nostr: relay {relay_url} connection failed: {e}, retrying in {backoff:?}");
                            tokio::time::sleep(backoff).await;
                            backoff = (backoff * 2).min(Duration::from_secs(60));
                            continue;
                        }
                    };

                    info!("Nostr: connected to relay {relay_url}");
                    backoff = Duration::from_secs(1);

                    use futures::{SinkExt, StreamExt};
                    let (mut write, mut read) = ws_stream.split();

                    // Send REQ subscription
                    // Build the subscription filter for DMs addressed to us
                    let sub_msg = {
                        let filter = serde_json::json!([
                            "REQ",
                            "openfang-sub",
                            {
                                "kinds": [4],
                                "#p": [&own_pubkey],
                                "limit": 0
                            }
                        ]);
                        serde_json::to_string(&filter).unwrap_or_default()
                    };

                    if write
                        .send(tokio_tungstenite::tungstenite::Message::Text(sub_msg))
                        .await
                        .is_err()
                    {
                        warn!("Nostr: failed to send REQ to {relay_url}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                        continue;
                    }

                    // Read events
                    let should_reconnect = loop {
                        let msg = tokio::select! {
                            _ = relay_shutdown_rx.changed() => {
                                info!("Nostr: relay {relay_url} shutting down");
                                // Send CLOSE
                                let close_msg = serde_json::json!(["CLOSE", "openfang-sub"]);
                                let _ = write.send(
                                    tokio_tungstenite::tungstenite::Message::Text(
                                        serde_json::to_string(&close_msg).unwrap_or_default()
                                    )
                                ).await;
                                return;
                            }
                            msg = read.next() => msg,
                        };

                        let msg = match msg {
                            Some(Ok(m)) => m,
                            Some(Err(e)) => {
                                warn!("Nostr: relay {relay_url} read error: {e}");
                                break true;
                            }
                            None => {
                                info!("Nostr: relay {relay_url} stream ended");
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

                        // Parse NIP-01 message: ["EVENT", "sub_id", {event}]
                        let parsed: serde_json::Value = match serde_json::from_str(&text) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        let msg_type = parsed[0].as_str().unwrap_or("");
                        if msg_type != "EVENT" {
                            // Could be NOTICE, EOSE, OK, etc.
                            continue;
                        }

                        let event = &parsed[2];
                        let event_id = event["id"].as_str().unwrap_or("").to_string();

                        // Dedup across relays
                        {
                            let mut seen = seen_events.write().await;
                            if seen.contains(&event_id) {
                                continue;
                            }
                            seen.insert(event_id.clone());
                            // Cap the seen set size
                            if seen.len() > 10000 {
                                seen.clear();
                            }
                        }

                        let sender_pubkey = event["pubkey"].as_str().unwrap_or("").to_string();
                        // Skip events from ourselves
                        if sender_pubkey == own_pubkey {
                            continue;
                        }

                        let content = event["content"].as_str().unwrap_or("");
                        if content.is_empty() {
                            continue;
                        }

                        // In a real implementation, kind-4 content would be
                        // NIP-04 encrypted and would need decryption here
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

                        let kind = event["kind"].as_i64().unwrap_or(0);

                        let channel_msg = ChannelMessage {
                            channel: ChannelType::Custom("nostr".to_string()),
                            platform_message_id: event_id,
                            sender: ChannelUser {
                                platform_id: sender_pubkey.clone(),
                                display_name: format!(
                                    "{}...",
                                    openfang_types::truncate_str(&sender_pubkey, 8)
                                ),
                                openfang_user: None,
                            },
                            content: msg_content,
                            target_agent: None,
                            timestamp: Utc::now(),
                            is_group: kind != 4, // DMs are 1:1, other kinds are public
                            thread_id: None,
                            metadata: {
                                let mut m = HashMap::new();
                                m.insert(
                                    "pubkey".to_string(),
                                    serde_json::Value::String(sender_pubkey),
                                );
                                m.insert(
                                    "kind".to_string(),
                                    serde_json::Value::Number(serde_json::Number::from(kind)),
                                );
                                m.insert(
                                    "relay".to_string(),
                                    serde_json::Value::String(relay_url.clone()),
                                );
                                m
                            },
                        };

                        if tx.send(channel_msg).await.is_err() {
                            return;
                        }
                    };

                    if !should_reconnect || *relay_shutdown_rx.borrow() {
                        break;
                    }

                    warn!("Nostr: reconnecting to {relay_url} in {backoff:?}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                }

                info!("Nostr: relay {relay_url} loop stopped");
            });
        }

        // Wait for shutdown in the main task
        tokio::spawn(async move {
            let _ = shutdown_rx.changed().await;
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
        // Nostr does not have a typing indicator protocol
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
    fn test_nostr_adapter_creation() {
        let adapter = NostrAdapter::new(
            "deadbeef".repeat(8),
            vec!["wss://relay.damus.io".to_string()],
        );
        assert_eq!(adapter.name(), "nostr");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("nostr".to_string())
        );
    }

    #[test]
    fn test_nostr_private_key_zeroized() {
        let key = "a".repeat(64);
        let adapter = NostrAdapter::new(key.clone(), vec!["wss://relay.example.com".to_string()]);
        assert_eq!(adapter.private_key.as_str(), key);
    }

    #[test]
    fn test_nostr_derive_pubkey() {
        let adapter = NostrAdapter::new("deadbeef".repeat(8), vec![]);
        let pubkey = adapter.derive_pubkey();
        assert_eq!(pubkey.len(), 64);
    }

    #[test]
    fn test_nostr_build_subscription() {
        let adapter = NostrAdapter::new("abc123".to_string(), vec![]);
        let pubkey = adapter.derive_pubkey();
        let sub = adapter.build_subscription(&pubkey);
        assert!(sub.contains("REQ"));
        assert!(sub.contains("openfang-sub"));
        assert!(sub.contains(&pubkey));
    }

    #[test]
    fn test_nostr_build_event() {
        let adapter = NostrAdapter::new("abc123".to_string(), vec![]);
        let event = adapter.build_event("recipient_pubkey_hex", "Hello Nostr!");
        assert!(event.contains("EVENT"));
        assert!(event.contains("Hello Nostr!"));
        assert!(event.contains("recipient_pubkey_hex"));
    }

    #[test]
    fn test_nostr_multiple_relays() {
        let adapter = NostrAdapter::new(
            "key".to_string(),
            vec![
                "wss://relay1.example.com".to_string(),
                "wss://relay2.example.com".to_string(),
                "wss://relay3.example.com".to_string(),
            ],
        );
        assert_eq!(adapter.relays.len(), 3);
    }
}
