//! Matrix channel adapter.
//!
//! Uses the Matrix Client-Server API (via reqwest) for sending and receiving messages.
//! Implements /sync long-polling for real-time message reception.

use crate::types::{ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{debug, info, warn};
use zeroize::Zeroizing;

const SYNC_TIMEOUT_MS: u64 = 30000;
const MAX_MESSAGE_LEN: usize = 4096;

/// Matrix channel adapter using the Client-Server API.
pub struct MatrixAdapter {
    /// Matrix homeserver URL (e.g., `"https://matrix.org"`).
    homeserver_url: String,
    /// Bot's user ID (e.g., "@openfang:matrix.org").
    user_id: String,
    /// SECURITY: Access token is zeroized on drop.
    access_token: Zeroizing<String>,
    /// HTTP client.
    client: reqwest::Client,
    /// Allowed room IDs (empty = all joined rooms).
    allowed_rooms: Vec<String>,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Sync token for resuming /sync.
    since_token: Arc<RwLock<Option<String>>>,
    /// Whether to auto-accept room invites.
    auto_accept_invites: bool,
}

impl MatrixAdapter {
    /// Create a new Matrix adapter.
    pub fn new(
        homeserver_url: String,
        user_id: String,
        access_token: String,
        allowed_rooms: Vec<String>,
        auto_accept_invites: bool,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            homeserver_url,
            user_id,
            access_token: Zeroizing::new(access_token),
            client: reqwest::Client::new(),
            allowed_rooms,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            since_token: Arc::new(RwLock::new(None)),
            auto_accept_invites,
        }
    }

    /// Send a text message to a Matrix room.
    async fn api_send_message(
        &self,
        room_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let txn_id = uuid::Uuid::new_v4().to_string();
        let url = format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            self.homeserver_url, room_id, txn_id
        );

        let chunks = crate::types::split_message(text, MAX_MESSAGE_LEN);
        for chunk in chunks {
            let body = serde_json::json!({
                "msgtype": "m.text",
                "body": chunk,
            });

            let resp = self
                .client
                .put(&url)
                .bearer_auth(&*self.access_token)
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("Matrix API error {status}: {body}").into());
            }
        }

        Ok(())
    }

    /// Validate credentials by calling /whoami.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/_matrix/client/v3/account/whoami", self.homeserver_url);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&*self.access_token)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Matrix authentication failed".into());
        }

        let body: serde_json::Value = resp.json().await?;
        let user_id = body["user_id"].as_str().unwrap_or("unknown").to_string();

        Ok(user_id)
    }

    #[cfg(test)]
    fn is_allowed_room(&self, room_id: &str) -> bool {
        self.allowed_rooms.is_empty() || self.allowed_rooms.iter().any(|r| r == room_id)
    }
}

/// Accept a room invite by calling POST /_matrix/client/v3/rooms/{room_id}/join.
async fn accept_invite(
    client: &reqwest::Client,
    homeserver: &str,
    access_token: &str,
    room_id: &str,
) {
    let url = format!("{homeserver}/_matrix/client/v3/rooms/{room_id}/join");
    match client
        .post(&url)
        .bearer_auth(access_token)
        .json(&serde_json::json!({}))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            info!("Matrix: auto-accepted invite to {room_id}");
        }
        Ok(resp) => {
            let status = resp.status();
            warn!("Matrix: failed to accept invite to {room_id}: {status}");
        }
        Err(e) => {
            warn!("Matrix: error accepting invite to {room_id}: {e}");
        }
    }
}

/// Get the number of joined members in a room.
async fn get_room_member_count(
    client: &reqwest::Client,
    homeserver: &str,
    access_token: &str,
    room_id: &str,
) -> Option<usize> {
    let url = format!("{homeserver}/_matrix/client/v3/rooms/{room_id}/joined_members");
    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    body["joined"].as_object().map(|m| m.len())
}

/// Do an initial /sync with timeout=0 to get the since token without processing events.
/// This prevents replaying old messages when the adapter first connects.
async fn initial_sync(
    client: &reqwest::Client,
    homeserver: &str,
    access_token: &str,
) -> Option<String> {
    let url = format!(
        "{homeserver}/_matrix/client/v3/sync?timeout=0&filter={{\"room\":{{\"timeline\":{{\"limit\":0}}}}}}"
    );
    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    body["next_batch"].as_str().map(String::from)
}

#[async_trait]
impl ChannelAdapter for MatrixAdapter {
    fn name(&self) -> &str {
        "matrix"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Matrix
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let validated_user = self.validate().await?;
        info!("Matrix adapter authenticated as {validated_user}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let homeserver = self.homeserver_url.clone();
        let access_token = self.access_token.clone();
        // Use the validated user ID from /whoami instead of the config value.
        // Matrix server delegation or casing differences can cause self.user_id
        // to not match the sender field in timeline events, making the bot
        // process its own replies in an infinite loop (see #757).
        let user_id = validated_user;
        let allowed_rooms = self.allowed_rooms.clone();
        let client = self.client.clone();
        let since_token = Arc::clone(&self.since_token);
        let mut shutdown_rx = self.shutdown_rx.clone();
        let auto_accept = self.auto_accept_invites;

        // FIX #4: Do an initial sync to get the since token, skipping old messages.
        if since_token.read().await.is_none() {
            if let Some(token) = initial_sync(&client, &homeserver, access_token.as_str()).await {
                info!("Matrix: initial sync complete, skipping old messages");
                *since_token.write().await = Some(token);
            }
        }

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            // Track recently seen event IDs to prevent duplicate processing
            // on sync token races or reconnects.
            let mut seen_events: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            const MAX_SEEN: usize = 500;

            loop {
                // Build /sync URL
                let since = since_token.read().await.clone();
                let mut url = format!(
                    "{}/_matrix/client/v3/sync?timeout={}&filter={{\"room\":{{\"timeline\":{{\"limit\":10}}}}}}",
                    homeserver, SYNC_TIMEOUT_MS
                );
                if let Some(ref token) = since {
                    url.push_str(&format!("&since={token}"));
                }

                let resp = tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Matrix adapter shutting down");
                        break;
                    }
                    result = client.get(&url).bearer_auth(access_token.as_str()).send() => {
                        match result {
                            Ok(r) => r,
                            Err(e) => {
                                warn!("Matrix sync error: {e}");
                                tokio::time::sleep(backoff).await;
                                backoff = (backoff * 2).min(Duration::from_secs(60));
                                continue;
                            }
                        }
                    }
                };

                if !resp.status().is_success() {
                    warn!("Matrix sync returned {}", resp.status());
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                    continue;
                }

                backoff = Duration::from_secs(1);

                let body: serde_json::Value = match resp.json().await {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Matrix sync parse error: {e}");
                        continue;
                    }
                };

                // Update since token
                if let Some(next) = body["next_batch"].as_str() {
                    *since_token.write().await = Some(next.to_string());
                }

                // FIX #1: Auto-accept room invites.
                if auto_accept {
                    if let Some(invites) = body["rooms"]["invite"].as_object() {
                        for (room_id, _invite_data) in invites {
                            if !allowed_rooms.is_empty()
                                && !allowed_rooms.iter().any(|r| r == room_id)
                            {
                                debug!(
                                    "Matrix: ignoring invite to {room_id} (not in allowed_rooms)"
                                );
                                continue;
                            }
                            accept_invite(&client, &homeserver, access_token.as_str(), room_id)
                                .await;
                        }
                    }
                }

                // Process room events
                if let Some(rooms) = body["rooms"]["join"].as_object() {
                    for (room_id, room_data) in rooms {
                        if !allowed_rooms.is_empty() && !allowed_rooms.iter().any(|r| r == room_id)
                        {
                            continue;
                        }

                        if let Some(events) = room_data["timeline"]["events"].as_array() {
                            for event in events {
                                let event_type = event["type"].as_str().unwrap_or("");
                                if event_type != "m.room.message" {
                                    continue;
                                }

                                let sender = event["sender"].as_str().unwrap_or("");
                                if sender == user_id {
                                    continue; // Skip own messages
                                }

                                // Dedup: skip events we've already processed.
                                let event_id_str =
                                    event["event_id"].as_str().unwrap_or("").to_string();
                                if !event_id_str.is_empty() {
                                    if seen_events.contains(&event_id_str) {
                                        debug!("Matrix: skipping duplicate event {event_id_str}");
                                        continue;
                                    }
                                    seen_events.insert(event_id_str.clone());
                                    // Prevent unbounded growth
                                    if seen_events.len() > MAX_SEEN {
                                        seen_events.clear();
                                    }
                                }

                                let content = event["content"]["body"].as_str().unwrap_or("");
                                if content.is_empty() {
                                    continue;
                                }

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

                                // FIX #2: Detect @mentions in message text.
                                let mut metadata = HashMap::new();
                                if content.contains(&user_id) {
                                    metadata.insert(
                                        "was_mentioned".to_string(),
                                        serde_json::json!(true),
                                    );
                                }

                                // FIX #3: Determine if room is a DM (2 members) or group.
                                let is_group = get_room_member_count(
                                    &client,
                                    &homeserver,
                                    access_token.as_str(),
                                    room_id,
                                )
                                .await
                                .map(|count| count > 2)
                                .unwrap_or(true);

                                // For DMs, auto-set was_mentioned so dm_policy works.
                                if !is_group {
                                    metadata.insert(
                                        "was_mentioned".to_string(),
                                        serde_json::json!(true),
                                    );
                                    metadata.insert("is_dm".to_string(), serde_json::json!(true));
                                }

                                // FIX #2: Detect @mentions in message text.
                                let mut metadata = HashMap::new();
                                if content.contains(&user_id) {
                                    metadata.insert(
                                        "was_mentioned".to_string(),
                                        serde_json::json!(true),
                                    );
                                }

                                // FIX #3: Determine if room is a DM (2 members) or group.
                                let is_group = get_room_member_count(
                                    &client,
                                    &homeserver,
                                    access_token.as_str(),
                                    room_id,
                                )
                                .await
                                .map(|count| count > 2)
                                .unwrap_or(true);

                                // For DMs, auto-set was_mentioned so dm_policy works.
                                if !is_group {
                                    metadata.insert(
                                        "was_mentioned".to_string(),
                                        serde_json::json!(true),
                                    );
                                    metadata.insert("is_dm".to_string(), serde_json::json!(true));
                                }

                                let channel_msg = ChannelMessage {
                                    channel: ChannelType::Matrix,
                                    platform_message_id: event_id_str,
                                    sender: ChannelUser {
                                        platform_id: room_id.clone(),
                                        display_name: sender.to_string(),
                                        openfang_user: None,
                                    },
                                    content: msg_content,
                                    target_agent: None,
                                    timestamp: Utc::now(),
                                    is_group,
                                    thread_id: None,
                                    metadata,
                                };

                                if tx.send(channel_msg).await.is_err() {
                                    return;
                                }
                            }
                        }
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

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "{}/_matrix/client/v3/rooms/{}/typing/{}",
            self.homeserver_url, user.platform_id, self.user_id
        );

        let body = serde_json::json!({
            "typing": true,
            "timeout": 5000,
        });

        let _ = self
            .client
            .put(&url)
            .bearer_auth(&*self.access_token)
            .json(&body)
            .send()
            .await;

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
    fn test_matrix_adapter_creation() {
        let adapter = MatrixAdapter::new(
            "https://matrix.org".to_string(),
            "@bot:matrix.org".to_string(),
            "access_token".to_string(),
            vec![],
            false,
        );
        assert_eq!(adapter.name(), "matrix");
    }

    #[test]
    fn test_matrix_allowed_rooms() {
        let adapter = MatrixAdapter::new(
            "https://matrix.org".to_string(),
            "@bot:matrix.org".to_string(),
            "token".to_string(),
            vec!["!room1:matrix.org".to_string()],
            false,
        );
        assert!(adapter.is_allowed_room("!room1:matrix.org"));
        assert!(!adapter.is_allowed_room("!room2:matrix.org"));

        let open = MatrixAdapter::new(
            "https://matrix.org".to_string(),
            "@bot:matrix.org".to_string(),
            "token".to_string(),
            vec![],
            false,
        );
        assert!(open.is_allowed_room("!any:matrix.org"));
    }
}
