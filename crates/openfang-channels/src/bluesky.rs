//! AT Protocol (Bluesky) channel adapter.
//!
//! Uses the AT Protocol (atproto) XRPC API for authentication, posting, and
//! polling notifications. Session creation uses `com.atproto.server.createSession`
//! with identifier + app password. Posts are created via
//! `com.atproto.repo.createRecord` with the `app.bsky.feed.post` lexicon.

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{info, warn};
use zeroize::Zeroizing;

/// Default Bluesky PDS service URL.
const DEFAULT_SERVICE_URL: &str = "https://bsky.social";

/// Maximum Bluesky post length (grapheme clusters).
const MAX_MESSAGE_LEN: usize = 300;

/// Notification poll interval in seconds.
const POLL_INTERVAL_SECS: u64 = 5;

/// Session refresh buffer — refresh 5 minutes before actual expiry.
const SESSION_REFRESH_BUFFER_SECS: u64 = 300;

/// AT Protocol (Bluesky) adapter.
///
/// Inbound mentions are received by polling the `app.bsky.notification.listNotifications`
/// endpoint. Outbound posts are created via `com.atproto.repo.createRecord` with
/// the `app.bsky.feed.post` record type. Session tokens are cached and refreshed
/// automatically.
pub struct BlueskyAdapter {
    /// AT Protocol identifier (handle or DID, e.g., "alice.bsky.social").
    identifier: String,
    /// SECURITY: App password for session creation, zeroized on drop.
    app_password: Zeroizing<String>,
    /// PDS service URL (default: `"https://bsky.social"`).
    service_url: String,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Cached session (access_jwt, refresh_jwt, did, expiry).
    session: Arc<RwLock<Option<BlueskySession>>>,
}

/// Cached Bluesky session data.
struct BlueskySession {
    /// JWT access token for authenticated requests.
    access_jwt: String,
    /// JWT refresh token for session renewal.
    refresh_jwt: String,
    /// The DID of the authenticated account.
    did: String,
    /// When this session was created (for expiry tracking).
    created_at: Instant,
}

impl BlueskyAdapter {
    /// Create a new Bluesky adapter with the default service URL.
    ///
    /// # Arguments
    /// * `identifier` - AT Protocol handle (e.g., "alice.bsky.social") or DID.
    /// * `app_password` - App password (not the main account password).
    pub fn new(identifier: String, app_password: String) -> Self {
        Self::with_service_url(identifier, app_password, DEFAULT_SERVICE_URL.to_string())
    }

    /// Create a new Bluesky adapter with a custom PDS service URL.
    pub fn with_service_url(identifier: String, app_password: String, service_url: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let service_url = service_url.trim_end_matches('/').to_string();
        Self {
            identifier,
            app_password: Zeroizing::new(app_password),
            service_url,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            session: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new session via `com.atproto.server.createSession`.
    async fn create_session(&self) -> Result<BlueskySession, Box<dyn std::error::Error>> {
        let url = format!("{}/xrpc/com.atproto.server.createSession", self.service_url);

        let body = serde_json::json!({
            "identifier": self.identifier,
            "password": self.app_password.as_str(),
        });

        let resp = self.client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!("Bluesky createSession failed {status}: {resp_body}").into());
        }

        let resp_body: serde_json::Value = resp.json().await?;
        let access_jwt = resp_body["accessJwt"]
            .as_str()
            .ok_or("Missing accessJwt")?
            .to_string();
        let refresh_jwt = resp_body["refreshJwt"]
            .as_str()
            .ok_or("Missing refreshJwt")?
            .to_string();
        let did = resp_body["did"].as_str().ok_or("Missing did")?.to_string();

        Ok(BlueskySession {
            access_jwt,
            refresh_jwt,
            did,
            created_at: Instant::now(),
        })
    }

    /// Refresh an existing session via `com.atproto.server.refreshSession`.
    async fn refresh_session(
        &self,
        refresh_jwt: &str,
    ) -> Result<BlueskySession, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/xrpc/com.atproto.server.refreshSession",
            self.service_url
        );

        let resp = self
            .client
            .post(&url)
            .bearer_auth(refresh_jwt)
            .send()
            .await?;

        if !resp.status().is_success() {
            // Refresh failed, create new session
            return self.create_session().await;
        }

        let resp_body: serde_json::Value = resp.json().await?;
        let access_jwt = resp_body["accessJwt"]
            .as_str()
            .ok_or("Missing accessJwt")?
            .to_string();
        let new_refresh_jwt = resp_body["refreshJwt"]
            .as_str()
            .ok_or("Missing refreshJwt")?
            .to_string();
        let did = resp_body["did"].as_str().ok_or("Missing did")?.to_string();

        Ok(BlueskySession {
            access_jwt,
            refresh_jwt: new_refresh_jwt,
            did,
            created_at: Instant::now(),
        })
    }

    /// Get a valid access JWT, creating or refreshing the session as needed.
    async fn get_token(&self) -> Result<(String, String), Box<dyn std::error::Error>> {
        let guard = self.session.read().await;
        if let Some(ref session) = *guard {
            // Sessions last ~2 hours; refresh if older than 90 minutes
            if session.created_at.elapsed()
                < Duration::from_secs(5400 - SESSION_REFRESH_BUFFER_SECS)
            {
                return Ok((session.access_jwt.clone(), session.did.clone()));
            }
            let refresh_jwt = session.refresh_jwt.clone();
            drop(guard);

            let new_session = self.refresh_session(&refresh_jwt).await?;
            let token = new_session.access_jwt.clone();
            let did = new_session.did.clone();
            *self.session.write().await = Some(new_session);
            return Ok((token, did));
        }
        drop(guard);

        let session = self.create_session().await?;
        let token = session.access_jwt.clone();
        let did = session.did.clone();
        *self.session.write().await = Some(session);
        Ok((token, did))
    }

    /// Validate credentials by creating a session.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let session = self.create_session().await?;
        let did = session.did.clone();
        *self.session.write().await = Some(session);
        Ok(did)
    }

    /// Create a post (skeet) via `com.atproto.repo.createRecord`.
    async fn api_create_post(
        &self,
        text: &str,
        reply_ref: Option<&serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (token, did) = self.get_token().await?;
        let url = format!("{}/xrpc/com.atproto.repo.createRecord", self.service_url);

        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let now = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

            let mut record = serde_json::json!({
                "$type": "app.bsky.feed.post",
                "text": chunk,
                "createdAt": now,
            });

            if let Some(reply) = reply_ref {
                record["reply"] = reply.clone();
            }

            let body = serde_json::json!({
                "repo": did,
                "collection": "app.bsky.feed.post",
                "record": record,
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(&token)
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Bluesky createRecord error {status}: {resp_body}").into());
            }
        }

        Ok(())
    }
}

/// Parse a Bluesky notification into a `ChannelMessage`.
fn parse_bluesky_notification(
    notification: &serde_json::Value,
    own_did: &str,
) -> Option<ChannelMessage> {
    let reason = notification["reason"].as_str().unwrap_or("");
    // We care about mentions and replies
    if reason != "mention" && reason != "reply" {
        return None;
    }

    let author = notification.get("author")?;
    let author_did = author["did"].as_str().unwrap_or("");
    // Skip own notifications
    if author_did == own_did {
        return None;
    }

    let record = notification.get("record")?;
    let text = record["text"].as_str().unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let uri = notification["uri"].as_str().unwrap_or("").to_string();
    let cid = notification["cid"].as_str().unwrap_or("").to_string();
    let handle = author["handle"].as_str().unwrap_or("").to_string();
    let display_name = author["displayName"]
        .as_str()
        .unwrap_or(&handle)
        .to_string();
    let indexed_at = notification["indexedAt"].as_str().unwrap_or("").to_string();

    let content = if text.starts_with('/') {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let cmd_name = parts[0].trim_start_matches('/');
        let args: Vec<String> = parts
            .get(1)
            .map(|a| a.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        ChannelContent::Command {
            name: cmd_name.to_string(),
            args,
        }
    } else {
        ChannelContent::Text(text.to_string())
    };

    let mut metadata = HashMap::new();
    metadata.insert("uri".to_string(), serde_json::Value::String(uri.clone()));
    metadata.insert("cid".to_string(), serde_json::Value::String(cid));
    metadata.insert("handle".to_string(), serde_json::Value::String(handle));
    metadata.insert(
        "reason".to_string(),
        serde_json::Value::String(reason.to_string()),
    );
    metadata.insert(
        "indexed_at".to_string(),
        serde_json::Value::String(indexed_at),
    );

    // Extract reply reference if present
    if let Some(reply) = record.get("reply") {
        metadata.insert("reply_ref".to_string(), reply.clone());
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("bluesky".to_string()),
        platform_message_id: uri,
        sender: ChannelUser {
            platform_id: author_did.to_string(),
            display_name,
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group: false, // Bluesky mentions are treated as direct interactions
        thread_id: None,
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for BlueskyAdapter {
    fn name(&self) -> &str {
        "bluesky"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("bluesky".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let did = self.validate().await?;
        info!("Bluesky adapter authenticated as {did}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let service_url = self.service_url.clone();
        let session = Arc::clone(&self.session);
        let own_did = did;
        let client = self.client.clone();
        let identifier = self.identifier.clone();
        let app_password = self.app_password.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let poll_interval = Duration::from_secs(POLL_INTERVAL_SECS);
            let mut backoff = Duration::from_secs(1);
            let mut last_seen_at: Option<String> = None;

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Bluesky adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                // Get current access token
                let token = {
                    let guard = session.read().await;
                    match &*guard {
                        Some(s) => s.access_jwt.clone(),
                        None => {
                            // Re-create session
                            drop(guard);
                            let url =
                                format!("{}/xrpc/com.atproto.server.createSession", service_url);
                            let body = serde_json::json!({
                                "identifier": identifier,
                                "password": app_password.as_str(),
                            });
                            match client.post(&url).json(&body).send().await {
                                Ok(resp) => {
                                    let resp_body: serde_json::Value =
                                        resp.json().await.unwrap_or_default();
                                    let tok =
                                        resp_body["accessJwt"].as_str().unwrap_or("").to_string();
                                    if tok.is_empty() {
                                        warn!("Bluesky: failed to create session");
                                        backoff = (backoff * 2).min(Duration::from_secs(60));
                                        tokio::time::sleep(backoff).await;
                                        continue;
                                    }
                                    let new_session = BlueskySession {
                                        access_jwt: tok.clone(),
                                        refresh_jwt: resp_body["refreshJwt"]
                                            .as_str()
                                            .unwrap_or("")
                                            .to_string(),
                                        did: resp_body["did"].as_str().unwrap_or("").to_string(),
                                        created_at: Instant::now(),
                                    };
                                    *session.write().await = Some(new_session);
                                    tok
                                }
                                Err(e) => {
                                    warn!("Bluesky: session create error: {e}");
                                    backoff = (backoff * 2).min(Duration::from_secs(60));
                                    tokio::time::sleep(backoff).await;
                                    continue;
                                }
                            }
                        }
                    }
                };

                // Poll notifications
                let mut url = format!(
                    "{}/xrpc/app.bsky.notification.listNotifications?limit=25",
                    service_url
                );
                if let Some(ref seen) = last_seen_at {
                    let encoded: String = url::form_urlencoded::Serializer::new(String::new())
                        .append_pair("seenAt", seen)
                        .finish();
                    url.push('&');
                    url.push_str(&encoded);
                }

                let resp = match client.get(&url).bearer_auth(&token).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("Bluesky: notification fetch error: {e}");
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                        continue;
                    }
                };

                if !resp.status().is_success() {
                    warn!("Bluesky: notification fetch returned {}", resp.status());
                    if resp.status().as_u16() == 401 {
                        // Session expired, clear it so next iteration re-creates
                        *session.write().await = None;
                    }
                    continue;
                }

                let body: serde_json::Value = match resp.json().await {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Bluesky: failed to parse notifications: {e}");
                        continue;
                    }
                };

                let notifications = match body["notifications"].as_array() {
                    Some(arr) => arr,
                    None => continue,
                };

                for notif in notifications {
                    // Track latest indexed_at
                    if let Some(indexed) = notif["indexedAt"].as_str() {
                        if last_seen_at
                            .as_ref()
                            .map(|s| indexed > s.as_str())
                            .unwrap_or(true)
                        {
                            last_seen_at = Some(indexed.to_string());
                        }
                    }

                    if let Some(msg) = parse_bluesky_notification(notif, &own_did) {
                        if tx.send(msg).await.is_err() {
                            return;
                        }
                    }
                }

                // Update seen marker
                if last_seen_at.is_some() {
                    let mark_url = format!("{}/xrpc/app.bsky.notification.updateSeen", service_url);
                    let mark_body = serde_json::json!({
                        "seenAt": Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                    });
                    let _ = client
                        .post(&mark_url)
                        .bearer_auth(&token)
                        .json(&mark_body)
                        .send()
                        .await;
                }

                backoff = Duration::from_secs(1);
            }

            info!("Bluesky polling loop stopped");
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        _user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match content {
            ChannelContent::Text(text) => {
                self.api_create_post(&text, None).await?;
            }
            _ => {
                self.api_create_post("(Unsupported content type)", None)
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Bluesky/AT Protocol does not support typing indicators
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
    fn test_bluesky_adapter_creation() {
        let adapter = BlueskyAdapter::new(
            "alice.bsky.social".to_string(),
            "app-password-123".to_string(),
        );
        assert_eq!(adapter.name(), "bluesky");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("bluesky".to_string())
        );
    }

    #[test]
    fn test_bluesky_default_service_url() {
        let adapter = BlueskyAdapter::new("alice.bsky.social".to_string(), "pwd".to_string());
        assert_eq!(adapter.service_url, "https://bsky.social");
    }

    #[test]
    fn test_bluesky_custom_service_url() {
        let adapter = BlueskyAdapter::with_service_url(
            "alice.example.com".to_string(),
            "pwd".to_string(),
            "https://pds.example.com/".to_string(),
        );
        assert_eq!(adapter.service_url, "https://pds.example.com");
    }

    #[test]
    fn test_bluesky_identifier_stored() {
        let adapter = BlueskyAdapter::new("did:plc:abc123".to_string(), "pwd".to_string());
        assert_eq!(adapter.identifier, "did:plc:abc123");
    }

    #[test]
    fn test_parse_bluesky_notification_mention() {
        let notif = serde_json::json!({
            "uri": "at://did:plc:sender/app.bsky.feed.post/abc123",
            "cid": "bafyrei...",
            "author": {
                "did": "did:plc:sender",
                "handle": "alice.bsky.social",
                "displayName": "Alice"
            },
            "reason": "mention",
            "record": {
                "text": "@bot hello there!",
                "createdAt": "2024-01-01T00:00:00.000Z"
            },
            "indexedAt": "2024-01-01T00:00:01.000Z"
        });

        let msg = parse_bluesky_notification(&notif, "did:plc:bot").unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("bluesky".to_string()));
        assert_eq!(msg.sender.display_name, "Alice");
        assert_eq!(msg.sender.platform_id, "did:plc:sender");
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "@bot hello there!"));
    }

    #[test]
    fn test_parse_bluesky_notification_reply() {
        let notif = serde_json::json!({
            "uri": "at://did:plc:sender/app.bsky.feed.post/def456",
            "cid": "bafyrei...",
            "author": {
                "did": "did:plc:sender",
                "handle": "bob.bsky.social",
                "displayName": "Bob"
            },
            "reason": "reply",
            "record": {
                "text": "Nice post!",
                "createdAt": "2024-01-01T00:00:00.000Z",
                "reply": {
                    "root": { "uri": "at://...", "cid": "..." },
                    "parent": { "uri": "at://...", "cid": "..." }
                }
            },
            "indexedAt": "2024-01-01T00:00:01.000Z"
        });

        let msg = parse_bluesky_notification(&notif, "did:plc:bot").unwrap();
        assert!(msg.metadata.contains_key("reply_ref"));
    }

    #[test]
    fn test_parse_bluesky_notification_skips_own() {
        let notif = serde_json::json!({
            "uri": "at://did:plc:bot/app.bsky.feed.post/abc",
            "cid": "...",
            "author": {
                "did": "did:plc:bot",
                "handle": "bot.bsky.social"
            },
            "reason": "mention",
            "record": {
                "text": "self mention"
            },
            "indexedAt": "2024-01-01T00:00:00.000Z"
        });

        assert!(parse_bluesky_notification(&notif, "did:plc:bot").is_none());
    }

    #[test]
    fn test_parse_bluesky_notification_skips_like() {
        let notif = serde_json::json!({
            "uri": "at://...",
            "cid": "...",
            "author": {
                "did": "did:plc:other",
                "handle": "other.bsky.social"
            },
            "reason": "like",
            "record": {},
            "indexedAt": "2024-01-01T00:00:00.000Z"
        });

        assert!(parse_bluesky_notification(&notif, "did:plc:bot").is_none());
    }

    #[test]
    fn test_parse_bluesky_notification_command() {
        let notif = serde_json::json!({
            "uri": "at://did:plc:sender/app.bsky.feed.post/cmd1",
            "cid": "...",
            "author": {
                "did": "did:plc:sender",
                "handle": "alice.bsky.social",
                "displayName": "Alice"
            },
            "reason": "mention",
            "record": {
                "text": "/status check"
            },
            "indexedAt": "2024-01-01T00:00:00.000Z"
        });

        let msg = parse_bluesky_notification(&notif, "did:plc:bot").unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "status");
                assert_eq!(args, &["check"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }
}
