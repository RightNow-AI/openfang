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
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(90))
                .connect_timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
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
            let mut sync_iteration: u64 = 0;
            // Track recently seen event IDs to prevent duplicate processing
            // on sync token races or reconnects.
            let mut seen_events: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            const MAX_SEEN: usize = 500;

            info!("Matrix sync loop started");

            loop {
                sync_iteration += 1;

                // Log every 10th iteration to confirm loop is alive
                if sync_iteration % 10 == 1 {
                    info!("Matrix sync loop alive, iteration={sync_iteration}");
                }

                // Check if shutdown was already signaled before entering select
                if *shutdown_rx.borrow() {
                    info!("Matrix sync loop: shutdown already signaled, exiting");
                    break;
                }

                // Build /sync URL
                let since = since_token.read().await.clone();
                let mut url = format!(
                    "{}/_matrix/client/v3/sync?timeout={}&filter={{\"room\":{{\"timeline\":{{\"limit\":10}}}}}}",
                    homeserver, SYNC_TIMEOUT_MS
                );
                if let Some(ref token) = since {
                    url.push_str(&format!("&since={token}"));
                }

                debug!("Matrix sync request iter={sync_iteration} has_since={}", since.is_some());

                let resp = tokio::select! {
                    result = shutdown_rx.changed() => {
                        match result {
                            Ok(()) => {
                                info!("Matrix sync loop: shutdown signal received, exiting");
                            }
                            Err(e) => {
                                warn!("Matrix sync loop: shutdown channel error (sender dropped): {e}");
                            }
                        }
                        break;
                    }
                    result = client.get(&url).bearer_auth(access_token.as_str()).send() => {
                        match result {
                            Ok(r) => r,
                            Err(e) => {
                                warn!("Matrix sync error (iter={sync_iteration}): {e}");
                                tokio::time::sleep(backoff).await;
                                backoff = (backoff * 2).min(Duration::from_secs(60));
                                continue;
                            }
                        }
                    }
                };

                if !resp.status().is_success() {
                    warn!("Matrix sync returned {} (iter={sync_iteration})", resp.status());
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                    continue;
                }

                backoff = Duration::from_secs(1);

                let body: serde_json::Value = match resp.json().await {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Matrix sync parse error (iter={sync_iteration}): {e}");
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

                                // Detect @mentions: check body, formatted_body, and m.mentions
                                let mut metadata = HashMap::new();
                                let mentioned_in_body = content.contains(&user_id);
                                let mentioned_in_html = event["content"]["formatted_body"]
                                    .as_str()
                                    .map(|html| html.contains(&user_id))
                                    .unwrap_or(false);
                                let mentioned_in_m_mentions = event["content"]["m.mentions"]["user_ids"]
                                    .as_array()
                                    .map(|ids| ids.iter().any(|id| id.as_str() == Some(&user_id)))
                                    .unwrap_or(false);
                                if mentioned_in_body || mentioned_in_html || mentioned_in_m_mentions {
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

                                info!("Matrix: dispatching message from {sender} in {room_id} (iter={sync_iteration})");
                                if tx.send(channel_msg).await.is_err() {
                                    warn!("Matrix sync loop: tx.send failed (receiver dropped), exiting (iter={sync_iteration})");
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            info!("Matrix sync loop exited (iter={sync_iteration})");
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
    use futures::StreamExt;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path, path_regex};

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

    /// Helper: create a Matrix sync response with a message in a room
    fn sync_response_with_message(
        next_batch: &str,
        room_id: &str,
        sender: &str,
        event_id: &str,
        body: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "next_batch": next_batch,
            "rooms": {
                "join": {
                    room_id: {
                        "timeline": {
                            "events": [{
                                "type": "m.room.message",
                                "sender": sender,
                                "event_id": event_id,
                                "origin_server_ts": 1234567890,
                                "content": {
                                    "msgtype": "m.text",
                                    "body": body
                                }
                            }]
                        }
                    }
                },
                "invite": {}
            }
        })
    }

    /// Helper: empty sync response (no new messages)
    fn sync_response_empty(next_batch: &str) -> serde_json::Value {
        serde_json::json!({
            "next_batch": next_batch,
            "rooms": {
                "join": {},
                "invite": {}
            }
        })
    }

    /// Helper: whoami response
    fn whoami_response(user_id: &str) -> serde_json::Value {
        serde_json::json!({
            "user_id": user_id,
            "is_guest": false,
            "device_id": "TESTDEVICE"
        })
    }

    /// Helper: joined_members response
    fn joined_members_response(count: usize) -> serde_json::Value {
        let mut members = serde_json::Map::new();
        for i in 0..count {
            members.insert(
                format!("@user{i}:test.org"),
                serde_json::json!({"display_name": format!("User {i}")}),
            );
        }
        serde_json::json!({ "joined": members })
    }

    // ─── Test 1: Adapter authenticates and starts sync ───

    #[tokio::test]
    async fn test_adapter_auth_and_start() {
        let server = MockServer::start().await;

        // Mock /whoami
        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whoami_response("@bot:test.org")))
            .mount(&server)
            .await;

        // Mock initial /sync (timeout=0)
        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/sync"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(sync_response_empty("batch_0"))
            )
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "test_token".to_string(),
            vec![],
            false,
        );

        let stream_result = adapter.start().await;
        assert!(stream_result.is_ok(), "Adapter should start successfully");

        // Clean shutdown
        adapter.stop().await.unwrap();
    }

    // ─── Test 2: Auth failure returns error ───

    #[tokio::test]
    async fn test_adapter_auth_failure() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "errcode": "M_UNKNOWN_TOKEN",
                "error": "Invalid token"
            })))
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "bad_token".to_string(),
            vec![],
            false,
        );

        let result = adapter.start().await;
        assert!(result.is_err(), "Should fail with bad token");
        let err = result.err().unwrap();
        assert!(
            err.to_string().contains("authentication failed"),
            "Error should mention auth failure, got: {err}"
        );
    }

    // ─── Test 3: Sync loop receives and dispatches a message ───

    #[tokio::test]
    async fn test_sync_receives_message() {
        let server = MockServer::start().await;

        // Mock /whoami
        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whoami_response("@bot:test.org")))
            .mount(&server)
            .await;

        // Mock /joined_members (DM = 2 members)
        Mock::given(method("GET"))
            .and(path_regex(r"/_matrix/client/v3/rooms/.*/joined_members"))
            .respond_with(ResponseTemplate::new(200).set_body_json(joined_members_response(2)))
            .mount(&server)
            .await;

        // Sync call counter via closure
        let sync_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        // Mock /sync: first call returns initial batch, second returns a message
        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/sync"))
            .respond_with(move |req: &wiremock::Request| {
                let count = sync_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let query = req.url.query().unwrap_or_default();

                if !query.contains("since=") || count == 0 {
                    // Initial sync (timeout=0)
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_empty("batch_1"))
                } else {
                    // Subsequent sync: return a message
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_2",
                            "!testroom:test.org",
                            "@alice:test.org",
                            "$event1",
                            "Hello OpenFang!",
                        ))
                }
            })
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "test_token".to_string(),
            vec![],
            false,
        );

        let mut stream = adapter.start().await.unwrap();

        // Wait for a message with timeout
        let msg = tokio::time::timeout(Duration::from_secs(10), stream.next()).await;
        assert!(msg.is_ok(), "Should receive message within timeout");

        let msg = msg.unwrap();
        assert!(msg.is_some(), "Stream should produce a message");

        let msg = msg.unwrap();
        assert_eq!(msg.sender.display_name, "@alice:test.org");
        assert_eq!(msg.sender.platform_id, "!testroom:test.org");
        match &msg.content {
            ChannelContent::Text(text) => assert_eq!(text, "Hello OpenFang!"),
            _ => panic!("Expected text message"),
        }

        adapter.stop().await.unwrap();
    }

    // ─── Test 4: Own messages are skipped ───

    #[tokio::test]
    async fn test_sync_skips_own_messages() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whoami_response("@bot:test.org")))
            .mount(&server)
            .await;

        let sync_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/sync"))
            .respond_with(move |req: &wiremock::Request| {
                let count = sync_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let query = req.url.query().unwrap_or_default();

                if !query.contains("since=") || count == 0 {
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_empty("batch_1"))
                } else if count == 1 {
                    // Bot's own message — should be skipped
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_2",
                            "!room:test.org",
                            "@bot:test.org", // ← own message
                            "$own_event",
                            "I am the bot",
                        ))
                } else {
                    // Real user message
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_3",
                            "!room:test.org",
                            "@human:test.org",
                            "$human_event",
                            "This should arrive",
                        ))
                }
            })
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"/_matrix/client/v3/rooms/.*/joined_members"))
            .respond_with(ResponseTemplate::new(200).set_body_json(joined_members_response(2)))
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "test_token".to_string(),
            vec![],
            false,
        );

        let mut stream = adapter.start().await.unwrap();

        let msg = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await
            .expect("Should get message within timeout")
            .expect("Stream should not end");

        // The first message we get should be from the human, not the bot
        assert_eq!(msg.sender.display_name, "@human:test.org");
        match &msg.content {
            ChannelContent::Text(text) => assert_eq!(text, "This should arrive"),
            _ => panic!("Expected text"),
        }

        adapter.stop().await.unwrap();
    }

    // ─── Test 5: Sync loop survives errors and retries ───

    #[tokio::test]
    async fn test_sync_retries_on_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whoami_response("@bot:test.org")))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"/_matrix/client/v3/rooms/.*/joined_members"))
            .respond_with(ResponseTemplate::new(200).set_body_json(joined_members_response(2)))
            .mount(&server)
            .await;

        let sync_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/sync"))
            .respond_with(move |req: &wiremock::Request| {
                let count = sync_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let query = req.url.query().unwrap_or_default();

                if !query.contains("since=") || count == 0 {
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_empty("batch_1"))
                } else if count <= 2 {
                    // Return 500 errors — sync should retry
                    ResponseTemplate::new(500)
                        .set_body_json(serde_json::json!({"error": "Internal Server Error"}))
                } else {
                    // After retries, return a valid message
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_2",
                            "!room:test.org",
                            "@user:test.org",
                            "$after_error",
                            "Message after errors",
                        ))
                }
            })
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "test_token".to_string(),
            vec![],
            false,
        );

        let mut stream = adapter.start().await.unwrap();

        // Should eventually get the message despite errors (with backoff retries)
        let msg = tokio::time::timeout(Duration::from_secs(15), stream.next())
            .await
            .expect("Should recover from errors")
            .expect("Stream should produce message");

        match &msg.content {
            ChannelContent::Text(text) => assert_eq!(text, "Message after errors"),
            _ => panic!("Expected text"),
        }

        adapter.stop().await.unwrap();
    }

    // ─── Test 6: Shutdown stops the sync loop cleanly ───

    #[tokio::test]
    async fn test_shutdown_stops_sync() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whoami_response("@bot:test.org")))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/sync"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(sync_response_empty("batch_1"))
                    // Simulate long-poll delay
                    .set_delay(Duration::from_secs(30))
            )
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "test_token".to_string(),
            vec![],
            false,
        );

        let mut stream = adapter.start().await.unwrap();

        // Shutdown after a brief delay
        tokio::time::sleep(Duration::from_millis(200)).await;
        adapter.stop().await.unwrap();

        // Stream should end (return None) after shutdown
        let result = tokio::time::timeout(Duration::from_secs(5), stream.next()).await;
        assert!(result.is_ok(), "Stream should end promptly after shutdown");
        assert!(result.unwrap().is_none(), "Stream should return None after shutdown");
    }

    // ─── Test 7: Commands are parsed correctly ───

    #[tokio::test]
    async fn test_command_parsing() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whoami_response("@bot:test.org")))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"/_matrix/client/v3/rooms/.*/joined_members"))
            .respond_with(ResponseTemplate::new(200).set_body_json(joined_members_response(2)))
            .mount(&server)
            .await;

        let sync_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/sync"))
            .respond_with(move |req: &wiremock::Request| {
                let count = sync_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let query = req.url.query().unwrap_or_default();

                if !query.contains("since=") || count == 0 {
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_empty("batch_1"))
                } else {
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_2",
                            "!room:test.org",
                            "@user:test.org",
                            "$cmd_event",
                            "/help me now",
                        ))
                }
            })
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "test_token".to_string(),
            vec![],
            false,
        );

        let mut stream = adapter.start().await.unwrap();

        let msg = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await
            .unwrap()
            .unwrap();

        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "help");
                assert_eq!(args, &["me", "now"]);
            }
            _ => panic!("Expected command, got {:?}", msg.content),
        }

        adapter.stop().await.unwrap();
    }

    // ─── Test 8: Duplicate events are deduplicated ───

    #[tokio::test]
    async fn test_dedup_events() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whoami_response("@bot:test.org")))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"/_matrix/client/v3/rooms/.*/joined_members"))
            .respond_with(ResponseTemplate::new(200).set_body_json(joined_members_response(2)))
            .mount(&server)
            .await;

        let sync_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/sync"))
            .respond_with(move |req: &wiremock::Request| {
                let count = sync_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let query = req.url.query().unwrap_or_default();

                if !query.contains("since=") || count == 0 {
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_empty("batch_1"))
                } else if count <= 2 {
                    // Same event ID returned twice
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_2",
                            "!room:test.org",
                            "@user:test.org",
                            "$same_event", // ← same ID
                            "Duplicate message",
                        ))
                } else {
                    // New unique event
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_3",
                            "!room:test.org",
                            "@user:test.org",
                            "$unique_event",
                            "Unique message",
                        ))
                }
            })
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "test_token".to_string(),
            vec![],
            false,
        );

        let mut stream = adapter.start().await.unwrap();

        // First message should be the duplicate (first occurrence)
        let msg1 = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await.unwrap().unwrap();
        assert!(matches!(&msg1.content, ChannelContent::Text(t) if t == "Duplicate message"));

        // Second message should be the unique one (duplicate was skipped)
        let msg2 = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await.unwrap().unwrap();
        assert!(matches!(&msg2.content, ChannelContent::Text(t) if t == "Unique message"));

        adapter.stop().await.unwrap();
    }

    // ─── Test 9: Allowed rooms filter works ───

    #[tokio::test]
    async fn test_allowed_rooms_filter() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/account/whoami"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whoami_response("@bot:test.org")))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path_regex(r"/_matrix/client/v3/rooms/.*/joined_members"))
            .respond_with(ResponseTemplate::new(200).set_body_json(joined_members_response(2)))
            .mount(&server)
            .await;

        let sync_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let sync_count_clone = sync_count.clone();

        Mock::given(method("GET"))
            .and(path("/_matrix/client/v3/sync"))
            .respond_with(move |req: &wiremock::Request| {
                let count = sync_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let query = req.url.query().unwrap_or_default();

                if !query.contains("since=") || count == 0 {
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_empty("batch_1"))
                } else if count == 1 {
                    // Message in blocked room
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_2",
                            "!blocked:test.org",
                            "@user:test.org",
                            "$blocked",
                            "Should be filtered",
                        ))
                } else {
                    // Message in allowed room
                    ResponseTemplate::new(200)
                        .set_body_json(sync_response_with_message(
                            "batch_3",
                            "!allowed:test.org",
                            "@user:test.org",
                            "$allowed",
                            "Should pass through",
                        ))
                }
            })
            .mount(&server)
            .await;

        let adapter = MatrixAdapter::new(
            server.uri(),
            "@bot:test.org".to_string(),
            "test_token".to_string(),
            vec!["!allowed:test.org".to_string()], // ← only this room
            false,
        );

        let mut stream = adapter.start().await.unwrap();

        let msg = tokio::time::timeout(Duration::from_secs(10), stream.next())
            .await.unwrap().unwrap();

        // Should only get the allowed room message
        assert_eq!(msg.sender.platform_id, "!allowed:test.org");
        assert!(matches!(&msg.content, ChannelContent::Text(t) if t == "Should pass through"));

        adapter.stop().await.unwrap();
    }
}
