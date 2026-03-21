//! LinkedIn Messaging channel adapter.
//!
//! Integrates with the LinkedIn Organization Messaging API using OAuth2
//! Bearer token authentication. Polls for new messages and sends replies
//! via the REST API.

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

const POLL_INTERVAL_SECS: u64 = 10;
const MAX_MESSAGE_LEN: usize = 3000;
const LINKEDIN_API_BASE: &str = "https://api.linkedin.com/v2";

/// LinkedIn Messaging channel adapter.
///
/// Polls the LinkedIn Organization Messaging API for new inbound messages
/// and sends replies via the same API. Requires a valid OAuth2 access token
/// with `r_organization_social` and `w_organization_social` scopes.
pub struct LinkedInAdapter {
    /// SECURITY: OAuth2 access token is zeroized on drop.
    access_token: Zeroizing<String>,
    /// LinkedIn organization URN (e.g., "urn:li:organization:12345").
    organization_id: String,
    /// HTTP client.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Last seen message timestamp for incremental polling (epoch millis).
    last_seen_ts: Arc<RwLock<i64>>,
}

impl LinkedInAdapter {
    /// Create a new LinkedIn adapter.
    ///
    /// # Arguments
    /// * `access_token` - OAuth2 Bearer token with messaging permissions.
    /// * `organization_id` - LinkedIn organization URN or numeric ID.
    pub fn new(access_token: String, organization_id: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        // Normalize organization_id to URN format
        let organization_id = if organization_id.starts_with("urn:") {
            organization_id
        } else {
            format!("urn:li:organization:{}", organization_id)
        };
        Self {
            access_token: Zeroizing::new(access_token),
            organization_id,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            last_seen_ts: Arc::new(RwLock::new(0)),
        }
    }

    /// Build an authenticated request builder.
    fn auth_request(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .bearer_auth(self.access_token.as_str())
            .header("X-Restli-Protocol-Version", "2.0.0")
            .header("LinkedIn-Version", "202401")
    }

    /// Validate credentials by fetching the organization info.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/organizations/{}",
            LINKEDIN_API_BASE,
            self.organization_id
                .strip_prefix("urn:li:organization:")
                .unwrap_or(&self.organization_id)
        );
        let resp = self.auth_request(self.client.get(&url)).send().await?;

        if !resp.status().is_success() {
            return Err(format!("LinkedIn auth failed (HTTP {})", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        let name = body["localizedName"]
            .as_str()
            .unwrap_or("LinkedIn Org")
            .to_string();
        Ok(name)
    }

    /// Fetch new messages from the organization messaging inbox.
    async fn fetch_messages(
        client: &reqwest::Client,
        access_token: &str,
        organization_id: &str,
        after_ts: i64,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/organizationMessages?q=organization&organization={}&count=50",
            LINKEDIN_API_BASE,
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("org", organization_id)
                .finish()
                .split('=')
                .nth(1)
                .unwrap_or(organization_id)
        );

        let resp = client
            .get(&url)
            .bearer_auth(access_token)
            .header("X-Restli-Protocol-Version", "2.0.0")
            .header("LinkedIn-Version", "202401")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("LinkedIn: HTTP {}", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        let elements = body["elements"].as_array().cloned().unwrap_or_default();

        // Filter to messages after the given timestamp
        let filtered: Vec<serde_json::Value> = elements
            .into_iter()
            .filter(|msg| {
                let created = msg["createdAt"].as_i64().unwrap_or(0);
                created > after_ts
            })
            .collect();

        Ok(filtered)
    }

    /// Send a message via the LinkedIn Organization Messaging API.
    async fn api_send_message(
        &self,
        recipient_urn: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/organizationMessages", LINKEDIN_API_BASE);
        let chunks = split_message(text, MAX_MESSAGE_LEN);
        let num_chunks = chunks.len();

        for chunk in chunks {
            let body = serde_json::json!({
                "recipients": [recipient_urn],
                "organization": self.organization_id,
                "body": {
                    "text": chunk,
                },
                "messageType": "MEMBER_TO_MEMBER",
            });

            let resp = self
                .auth_request(self.client.post(&url))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("LinkedIn API error {status}: {err_body}").into());
            }

            // LinkedIn rate limit: max 100 requests per day for messaging
            // Small delay between chunks to be respectful
            if num_chunks > 1 {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        Ok(())
    }

    /// Parse a LinkedIn message element into usable fields.
    fn parse_message_element(
        element: &serde_json::Value,
    ) -> Option<(String, String, String, String, i64)> {
        let id = element["id"].as_str()?.to_string();
        let body_text = element["body"]["text"].as_str()?.to_string();
        if body_text.is_empty() {
            return None;
        }

        let sender_urn = element["from"].as_str().unwrap_or("unknown").to_string();
        let sender_name = element["fromName"]
            .as_str()
            .or_else(|| element["senderName"].as_str())
            .unwrap_or("LinkedIn User")
            .to_string();
        let created_at = element["createdAt"].as_i64().unwrap_or(0);

        Some((id, body_text, sender_urn, sender_name, created_at))
    }

    /// Get the numeric organization ID.
    pub fn org_numeric_id(&self) -> &str {
        self.organization_id
            .strip_prefix("urn:li:organization:")
            .unwrap_or(&self.organization_id)
    }
}

#[async_trait]
impl ChannelAdapter for LinkedInAdapter {
    fn name(&self) -> &str {
        "linkedin"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("linkedin".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let org_name = self.validate().await?;
        info!("LinkedIn adapter authenticated for org: {org_name}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let access_token = self.access_token.clone();
        let organization_id = self.organization_id.clone();
        let client = self.client.clone();
        let last_seen_ts = Arc::clone(&self.last_seen_ts);
        let mut shutdown_rx = self.shutdown_rx.clone();

        // Initialize last_seen_ts to now so we only get new messages
        {
            *last_seen_ts.write().await = Utc::now().timestamp_millis();
        }

        let poll_interval = Duration::from_secs(POLL_INTERVAL_SECS);

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("LinkedIn adapter shutting down");
                            break;
                        }
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                let after_ts = *last_seen_ts.read().await;

                let poll_result =
                    Self::fetch_messages(&client, &access_token, &organization_id, after_ts)
                        .await
                        .map_err(|e| e.to_string());

                let messages = match poll_result {
                    Ok(m) => {
                        backoff = Duration::from_secs(1);
                        m
                    }
                    Err(msg) => {
                        warn!("LinkedIn: poll error: {msg}, backing off {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(300));
                        continue;
                    }
                };

                let mut max_ts = after_ts;

                for element in &messages {
                    let (id, body_text, sender_urn, sender_name, created_at) =
                        match Self::parse_message_element(element) {
                            Some(parsed) => parsed,
                            None => continue,
                        };

                    // Skip messages from own organization
                    if sender_urn.contains(&organization_id) {
                        continue;
                    }

                    if created_at > max_ts {
                        max_ts = created_at;
                    }

                    let thread_id = element["conversationId"]
                        .as_str()
                        .or_else(|| element["threadId"].as_str())
                        .map(String::from);

                    let content = if body_text.starts_with('/') {
                        let parts: Vec<&str> = body_text.splitn(2, ' ').collect();
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
                        ChannelContent::Text(body_text)
                    };

                    let msg = ChannelMessage {
                        channel: ChannelType::Custom("linkedin".to_string()),
                        platform_message_id: id,
                        sender: ChannelUser {
                            platform_id: sender_urn.clone(),
                            display_name: sender_name,
                            openfang_user: None,
                            reply_url: None,
                        },
                        content,
                        target_agent: None,
                        timestamp: Utc::now(),
                        is_group: false,
                        thread_id,
                        metadata: {
                            let mut m = HashMap::new();
                            m.insert(
                                "sender_urn".to_string(),
                                serde_json::Value::String(sender_urn),
                            );
                            m.insert(
                                "organization_id".to_string(),
                                serde_json::Value::String(organization_id.clone()),
                            );
                            m
                        },
                    };

                    if tx.send(msg).await.is_err() {
                        return;
                    }
                }

                if max_ts > after_ts {
                    *last_seen_ts.write().await = max_ts;
                }
            }

            info!("LinkedIn polling loop stopped");
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(t) => t,
            _ => "(Unsupported content type)".to_string(),
        };

        // user.platform_id should be the recipient's LinkedIn URN
        self.api_send_message(&user.platform_id, &text).await
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // LinkedIn Messaging API does not support typing indicators.
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
    fn test_linkedin_adapter_creation() {
        let adapter = LinkedInAdapter::new("test-token".to_string(), "12345".to_string());
        assert_eq!(adapter.name(), "linkedin");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("linkedin".to_string())
        );
    }

    #[test]
    fn test_linkedin_organization_id_normalization() {
        let adapter = LinkedInAdapter::new("tok".to_string(), "12345".to_string());
        assert_eq!(adapter.organization_id, "urn:li:organization:12345");

        let adapter2 =
            LinkedInAdapter::new("tok".to_string(), "urn:li:organization:67890".to_string());
        assert_eq!(adapter2.organization_id, "urn:li:organization:67890");
    }

    #[test]
    fn test_linkedin_org_numeric_id() {
        let adapter = LinkedInAdapter::new("tok".to_string(), "12345".to_string());
        assert_eq!(adapter.org_numeric_id(), "12345");
    }

    #[test]
    fn test_linkedin_auth_headers() {
        let adapter = LinkedInAdapter::new("my-oauth-token".to_string(), "12345".to_string());
        let builder = adapter.client.get("https://api.linkedin.com/v2/me");
        let builder = adapter.auth_request(builder);
        let request = builder.build().unwrap();
        assert!(request.headers().contains_key("authorization"));
        assert_eq!(
            request.headers().get("X-Restli-Protocol-Version").unwrap(),
            "2.0.0"
        );
        assert_eq!(request.headers().get("LinkedIn-Version").unwrap(), "202401");
    }

    #[test]
    fn test_linkedin_parse_message_element() {
        let element = serde_json::json!({
            "id": "msg-001",
            "body": { "text": "Hello from LinkedIn" },
            "from": "urn:li:person:abc123",
            "fromName": "Jane Doe",
            "createdAt": 1700000000000_i64,
        });
        let result = LinkedInAdapter::parse_message_element(&element);
        assert!(result.is_some());
        let (id, body, from, name, ts) = result.unwrap();
        assert_eq!(id, "msg-001");
        assert_eq!(body, "Hello from LinkedIn");
        assert_eq!(from, "urn:li:person:abc123");
        assert_eq!(name, "Jane Doe");
        assert_eq!(ts, 1700000000000);
    }

    #[test]
    fn test_linkedin_parse_message_empty_body() {
        let element = serde_json::json!({
            "id": "msg-002",
            "body": { "text": "" },
            "from": "urn:li:person:xyz",
        });
        assert!(LinkedInAdapter::parse_message_element(&element).is_none());
    }

    #[test]
    fn test_linkedin_parse_message_missing_body() {
        let element = serde_json::json!({
            "id": "msg-003",
            "from": "urn:li:person:xyz",
        });
        assert!(LinkedInAdapter::parse_message_element(&element).is_none());
    }

    #[test]
    fn test_linkedin_parse_message_defaults() {
        let element = serde_json::json!({
            "id": "msg-004",
            "body": { "text": "Hi" },
        });
        let result = LinkedInAdapter::parse_message_element(&element);
        assert!(result.is_some());
        let (_, _, from, name, _) = result.unwrap();
        assert_eq!(from, "unknown");
        assert_eq!(name, "LinkedIn User");
    }
}
