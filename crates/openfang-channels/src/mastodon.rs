//! Mastodon Streaming API channel adapter.
//!
//! Uses the Mastodon REST API v1 for sending statuses (toots) and the Streaming
//! API (Server-Sent Events) for real-time notification reception. Authentication
//! is performed via `Authorization: Bearer {access_token}` on all API calls.
//! Mentions/notifications are received via the SSE user stream endpoint.

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

/// Maximum Mastodon status length (default server limit).
const MAX_MESSAGE_LEN: usize = 500;

/// SSE reconnect delay on error.
const SSE_RECONNECT_DELAY_SECS: u64 = 5;

/// Maximum backoff for SSE reconnection.
const MAX_BACKOFF_SECS: u64 = 60;

/// Mastodon Streaming API adapter.
///
/// Inbound mentions are received via Server-Sent Events (SSE) from the
/// Mastodon streaming user endpoint. Outbound replies are posted as new
/// statuses with `in_reply_to_id` set to the original status ID.
pub struct MastodonAdapter {
    /// Mastodon instance URL (e.g., `"https://mastodon.social"`).
    instance_url: String,
    /// SECURITY: Access token (OAuth2 bearer token), zeroized on drop.
    access_token: Zeroizing<String>,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Bot's own account ID (populated after verification).
    own_account_id: Arc<RwLock<Option<String>>>,
}

impl MastodonAdapter {
    /// Create a new Mastodon adapter.
    ///
    /// # Arguments
    /// * `instance_url` - Base URL of the Mastodon instance (no trailing slash).
    /// * `access_token` - OAuth2 access token with `read` and `write` scopes.
    pub fn new(instance_url: String, access_token: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let instance_url = instance_url.trim_end_matches('/').to_string();
        Self {
            instance_url,
            access_token: Zeroizing::new(access_token),
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            own_account_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Validate the access token by calling `/api/v1/accounts/verify_credentials`.
    async fn validate(&self) -> Result<(String, String), Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/accounts/verify_credentials", self.instance_url);

        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.access_token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Mastodon authentication failed {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let account_id = body["id"].as_str().unwrap_or("").to_string();
        let username = body["username"].as_str().unwrap_or("unknown").to_string();

        // Store own account ID
        *self.own_account_id.write().await = Some(account_id.clone());

        Ok((account_id, username))
    }

    /// Post a status (toot), optionally as a reply.
    async fn api_post_status(
        &self,
        text: &str,
        in_reply_to_id: Option<&str>,
        visibility: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/v1/statuses", self.instance_url);

        let chunks = split_message(text, MAX_MESSAGE_LEN);

        let mut reply_id = in_reply_to_id.map(|s| s.to_string());

        for chunk in chunks {
            let mut params: HashMap<&str, &str> = HashMap::new();
            params.insert("status", chunk);
            params.insert("visibility", visibility);

            if let Some(ref rid) = reply_id {
                params.insert("in_reply_to_id", rid);
            }

            let resp = self
                .client
                .post(&url)
                .bearer_auth(self.access_token.as_str())
                .form(&params)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let resp_body = resp.text().await.unwrap_or_default();
                return Err(format!("Mastodon post status error {status}: {resp_body}").into());
            }

            // If we're posting a thread, chain replies
            let resp_body: serde_json::Value = resp.json().await?;
            reply_id = resp_body["id"].as_str().map(|s| s.to_string());
        }

        Ok(())
    }

    /// Fetch notifications (mentions) since a given ID.
    #[allow(dead_code)]
    async fn fetch_notifications(
        &self,
        since_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let mut url = format!(
            "{}/api/v1/notifications?types[]=mention&limit=30",
            self.instance_url
        );

        if let Some(sid) = since_id {
            url.push_str(&format!("&since_id={}", sid));
        }

        let resp = self
            .client
            .get(&url)
            .bearer_auth(self.access_token.as_str())
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Failed to fetch Mastodon notifications".into());
        }

        let notifications: Vec<serde_json::Value> = resp.json().await?;
        Ok(notifications)
    }
}

/// Parse a Mastodon notification (mention) into a `ChannelMessage`.
fn parse_mastodon_notification(
    notification: &serde_json::Value,
    own_account_id: &str,
) -> Option<ChannelMessage> {
    let notif_type = notification["type"].as_str().unwrap_or("");
    if notif_type != "mention" {
        return None;
    }

    let status = notification.get("status")?;
    let account = notification.get("account")?;

    let account_id = account["id"].as_str().unwrap_or("");
    // Skip own mentions (shouldn't happen but guard)
    if account_id == own_account_id {
        return None;
    }

    // Extract text content (strip HTML tags for plain text)
    let content_html = status["content"].as_str().unwrap_or("");
    let text = strip_html_tags(content_html);
    if text.is_empty() {
        return None;
    }

    let status_id = status["id"].as_str().unwrap_or("").to_string();
    let notif_id = notification["id"].as_str().unwrap_or("").to_string();
    let username = account["username"].as_str().unwrap_or("").to_string();
    let display_name = account["display_name"]
        .as_str()
        .unwrap_or(&username)
        .to_string();
    let acct = account["acct"].as_str().unwrap_or("").to_string();
    let visibility = status["visibility"]
        .as_str()
        .unwrap_or("public")
        .to_string();
    let in_reply_to = status["in_reply_to_id"].as_str().map(|s| s.to_string());

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
        ChannelContent::Text(text)
    };

    let mut metadata = HashMap::new();
    metadata.insert(
        "status_id".to_string(),
        serde_json::Value::String(status_id.clone()),
    );
    metadata.insert(
        "notification_id".to_string(),
        serde_json::Value::String(notif_id),
    );
    metadata.insert("acct".to_string(), serde_json::Value::String(acct));
    metadata.insert(
        "visibility".to_string(),
        serde_json::Value::String(visibility),
    );
    if let Some(ref reply_to) = in_reply_to {
        metadata.insert(
            "in_reply_to_id".to_string(),
            serde_json::Value::String(reply_to.clone()),
        );
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("mastodon".to_string()),
        platform_message_id: status_id,
        sender: ChannelUser {
            platform_id: account_id.to_string(),
            display_name,
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group: false, // Mentions are treated as DM-like interactions
        thread_id: in_reply_to,
        metadata,
    })
}

/// Simple HTML tag stripper for Mastodon status content.
///
/// Mastodon returns HTML in status content. This strips tags and decodes
/// common HTML entities. For production, consider a proper HTML sanitizer.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut tag_buf = String::new();

    for ch in html.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag_buf.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                // Insert newline for block-level closing tags
                let tag_lower = tag_buf.to_lowercase();
                if tag_lower.starts_with("br")
                    || tag_lower.starts_with("/p")
                    || tag_lower.starts_with("/div")
                    || tag_lower.starts_with("/li")
                {
                    result.push('\n');
                }
                tag_buf.clear();
            }
            _ if in_tag => {
                tag_buf.push(ch);
            }
            _ => {
                result.push(ch);
            }
        }
    }

    // Decode HTML entities (handles named, decimal, and hex entities)
    let decoded = html_escape::decode_html_entities(&result);
    decoded.trim().to_string()
}

#[async_trait]
impl ChannelAdapter for MastodonAdapter {
    fn name(&self) -> &str {
        "mastodon"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("mastodon".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let (account_id, username) = self.validate().await?;
        info!("Mastodon adapter authenticated as @{username} (id: {account_id})");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let instance_url = self.instance_url.clone();
        let access_token = self.access_token.clone();
        let own_account_id = account_id;
        let client = self.client.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let poll_interval = Duration::from_secs(SSE_RECONNECT_DELAY_SECS);
            let mut backoff = Duration::from_secs(1);
            let mut last_notification_id: Option<String> = None;
            let mut use_streaming = true;

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Mastodon adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                if use_streaming {
                    // Attempt SSE connection to streaming API
                    let stream_url = format!("{}/api/v1/streaming/user", instance_url);

                    match client
                        .get(&stream_url)
                        .bearer_auth(access_token.as_str())
                        .header("Accept", "text/event-stream")
                        .timeout(Duration::from_secs(5))
                        .send()
                        .await
                    {
                        Ok(r) if r.status().is_success() => {
                            info!("Mastodon: connected to SSE stream");
                            backoff = Duration::from_secs(1);

                            use futures::StreamExt;
                            let mut bytes_stream = r.bytes_stream();
                            let mut event_type = String::new();

                            while let Some(chunk_result) = bytes_stream.next().await {
                                if *shutdown_rx.borrow_and_update() {
                                    return;
                                }

                                let chunk = match chunk_result {
                                    Ok(c) => c,
                                    Err(e) => {
                                        warn!("Mastodon SSE stream error: {e}");
                                        break;
                                    }
                                };

                                let text = String::from_utf8_lossy(&chunk);
                                for line in text.lines() {
                                    if let Some(ev) = line.strip_prefix("event: ") {
                                        event_type = ev.trim().to_string();
                                    } else if let Some(data) = line.strip_prefix("data: ") {
                                        if event_type == "notification" {
                                            if let Ok(notif) =
                                                serde_json::from_str::<serde_json::Value>(data)
                                            {
                                                if let Some(msg) = parse_mastodon_notification(
                                                    &notif,
                                                    &own_account_id,
                                                ) {
                                                    let _ = tx.send(msg).await;
                                                }
                                            }
                                        }
                                        event_type.clear();
                                    }
                                }
                            }

                            // Stream ended, will reconnect
                        }
                        Ok(r) => {
                            warn!(
                                "Mastodon SSE: non-success status {}, falling back to polling",
                                r.status()
                            );
                            use_streaming = false;
                        }
                        Err(e) => {
                            warn!("Mastodon SSE connection failed: {e}, falling back to polling");
                            use_streaming = false;
                        }
                    }

                    // Backoff before reconnect attempt
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(MAX_BACKOFF_SECS));
                    continue;
                }

                // Polling fallback: fetch notifications via REST
                let mut url = format!(
                    "{}/api/v1/notifications?types[]=mention&limit=30",
                    instance_url
                );
                if let Some(ref sid) = last_notification_id {
                    url.push_str(&format!("&since_id={}", sid));
                }

                let poll_resp = match client
                    .get(&url)
                    .bearer_auth(access_token.as_str())
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("Mastodon: notification poll error: {e}");
                        continue;
                    }
                };

                if !poll_resp.status().is_success() {
                    warn!(
                        "Mastodon: notification poll returned {}",
                        poll_resp.status()
                    );
                    continue;
                }

                let notifications: Vec<serde_json::Value> =
                    poll_resp.json().await.unwrap_or_default();

                // Mastodon returns notifications newest-first. Record the first
                // (highest) ID before processing so we never re-fetch these on
                // the next poll. Updating inside the loop would leave us with
                // the oldest ID, causing every previously seen notification to
                // be re-delivered and re-processed.
                if let Some(newest) = notifications.first() {
                    if let Some(nid) = newest["id"].as_str() {
                        last_notification_id = Some(nid.to_string());
                    }
                }

                for notif in &notifications {
                    if let Some(msg) = parse_mastodon_notification(notif, &own_account_id) {
                        if tx.send(msg).await.is_err() {
                            return;
                        }
                    }
                }

                backoff = Duration::from_secs(1);
            }

            info!("Mastodon polling loop stopped");
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
                // _user.platform_id is the account_id; we use status_id from metadata for reply
                self.api_post_status(&text, None, "unlisted").await?;
            }
            _ => {
                self.api_post_status("(Unsupported content type)", None, "unlisted")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_in_thread(
        &self,
        _user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match content {
            ChannelContent::Text(text) => {
                self.api_post_status(&text, Some(thread_id), "unlisted")
                    .await?;
            }
            _ => {
                self.api_post_status("(Unsupported content type)", Some(thread_id), "unlisted")
                    .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Mastodon does not support typing indicators
        Ok(())
    }

    fn suppress_error_responses(&self) -> bool {
        true
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
    fn test_mastodon_adapter_creation() {
        let adapter = MastodonAdapter::new(
            "https://mastodon.social".to_string(),
            "access-token-123".to_string(),
        );
        assert_eq!(adapter.name(), "mastodon");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("mastodon".to_string())
        );
    }

    #[test]
    fn test_mastodon_url_normalization() {
        let adapter =
            MastodonAdapter::new("https://mastodon.social/".to_string(), "tok".to_string());
        assert_eq!(adapter.instance_url, "https://mastodon.social");
    }

    #[test]
    fn test_mastodon_custom_instance() {
        let adapter =
            MastodonAdapter::new("https://infosec.exchange".to_string(), "tok".to_string());
        assert_eq!(adapter.instance_url, "https://infosec.exchange");
    }

    #[test]
    fn test_strip_html_tags_basic() {
        assert_eq!(
            strip_html_tags("<p>Hello <strong>world</strong></p>"),
            "Hello world"
        );
    }

    #[test]
    fn test_strip_html_tags_entities() {
        assert_eq!(strip_html_tags("a &amp; b &lt; c"), "a & b < c");
    }

    #[test]
    fn test_strip_html_tags_empty() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn test_strip_html_tags_no_tags() {
        assert_eq!(strip_html_tags("plain text"), "plain text");
    }

    #[test]
    fn test_strip_html_tags_emoji() {
        assert_eq!(
            strip_html_tags("<p>Hello 🦀🔥 world</p>"),
            "Hello 🦀🔥 world"
        );
    }

    #[test]
    fn test_strip_html_tags_cjk() {
        assert_eq!(
            strip_html_tags("<p>你好 <strong>世界</strong></p>"),
            "你好 世界"
        );
    }

    #[test]
    fn test_strip_html_tags_numeric_entities() {
        assert_eq!(strip_html_tags("&#39;hello&#39;"), "'hello'");
    }

    #[test]
    fn test_strip_html_tags_div_newline() {
        assert_eq!(
            strip_html_tags("<div>one</div><div>two</div>").trim(),
            "one\ntwo"
        );
    }

    #[test]
    fn test_parse_mastodon_notification_mention() {
        let notif = serde_json::json!({
            "id": "notif-1",
            "type": "mention",
            "account": {
                "id": "acct-123",
                "username": "alice",
                "display_name": "Alice",
                "acct": "alice@mastodon.social"
            },
            "status": {
                "id": "status-456",
                "content": "<p>@bot Hello!</p>",
                "visibility": "public",
                "in_reply_to_id": null
            }
        });

        let msg = parse_mastodon_notification(&notif, "acct-999").unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("mastodon".to_string()));
        assert_eq!(msg.sender.display_name, "Alice");
        assert_eq!(msg.platform_message_id, "status-456");
    }

    #[test]
    fn test_parse_mastodon_notification_non_mention() {
        let notif = serde_json::json!({
            "id": "notif-1",
            "type": "favourite",
            "account": {
                "id": "acct-123",
                "username": "alice"
            },
            "status": {
                "id": "status-456",
                "content": "<p>liked</p>"
            }
        });

        assert!(parse_mastodon_notification(&notif, "acct-999").is_none());
    }

    #[test]
    fn test_parse_mastodon_notification_own_mention() {
        let notif = serde_json::json!({
            "id": "notif-1",
            "type": "mention",
            "account": {
                "id": "acct-999",
                "username": "bot"
            },
            "status": {
                "id": "status-1",
                "content": "<p>self mention</p>",
                "visibility": "public"
            }
        });

        assert!(parse_mastodon_notification(&notif, "acct-999").is_none());
    }

    #[test]
    fn test_parse_mastodon_notification_visibility() {
        let notif = serde_json::json!({
            "id": "notif-1",
            "type": "mention",
            "account": {
                "id": "acct-123",
                "username": "alice",
                "display_name": "Alice",
                "acct": "alice"
            },
            "status": {
                "id": "status-1",
                "content": "<p>DM to bot</p>",
                "visibility": "direct",
                "in_reply_to_id": null
            }
        });

        let msg = parse_mastodon_notification(&notif, "acct-999").unwrap();
        assert_eq!(
            msg.metadata.get("visibility").and_then(|v| v.as_str()),
            Some("direct")
        );
    }
}
