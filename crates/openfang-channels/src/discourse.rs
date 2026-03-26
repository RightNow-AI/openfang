//! Discourse channel adapter.
//!
//! Integrates with the Discourse forum REST API. Uses long-polling on
//! `posts.json` to receive new posts and creates replies via the same API.
//! Authentication uses the `Api-Key` and `Api-Username` headers.

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
const MAX_MESSAGE_LEN: usize = 32000;

/// Discourse forum channel adapter.
///
/// Polls the Discourse `/posts.json` endpoint for new posts and creates
/// replies via `POST /posts.json`. Filters posts by category if configured.
pub struct DiscourseAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// Base URL of the Discourse instance (e.g., `"https://forum.example.com"`).
    base_url: String,
    /// SECURITY: API key is zeroized on drop.
    api_key: Zeroizing<String>,
    /// Username associated with the API key.
    api_username: String,
    /// Category slugs to filter (empty = all categories).
    categories: Vec<String>,
    /// HTTP client.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Last seen post ID (for incremental polling).
    last_post_id: Arc<RwLock<u64>>,
}

impl DiscourseAdapter {
    /// Create a new Discourse adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `base_url` - Base URL of the Discourse instance.
    /// * `api_key` - Discourse API key (admin or user-scoped).
    /// * `api_username` - Username for the API key (usually "system" or a bot account).
    /// * `categories` - Category slugs to listen to (empty = all).
    pub fn new(
        id: String,
        base_url: String,
        api_key: String,
        api_username: String,
        categories: Vec<String>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            id,
            base_url,
            api_key: Zeroizing::new(api_key),
            api_username,
            categories,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            last_post_id: Arc::new(RwLock::new(0)),
        }
    }

    /// Add Discourse API auth headers to a request builder.
    fn auth_headers(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder
            .header("Api-Key", self.api_key.as_str())
            .header("Api-Username", &self.api_username)
    }

    /// Validate credentials by calling `/session/current.json`.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let url = format!("{}/session/current.json", self.base_url);
        let resp = self.auth_headers(self.client.get(&url)).send().await?;

        if !resp.status().is_success() {
            return Err(format!("Discourse auth failed (HTTP {})", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        let username = body["current_user"]["username"]
            .as_str()
            .unwrap_or(&self.api_username)
            .to_string();
        Ok(username)
    }

    /// Fetch the latest posts since `before_id`.
    async fn fetch_latest_posts(
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        api_username: &str,
        before_id: u64,
    ) -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        let url = if before_id > 0 {
            format!("{}/posts.json?before={}", base_url, before_id)
        } else {
            format!("{}/posts.json", base_url)
        };

        let resp = client
            .get(&url)
            .header("Api-Key", api_key)
            .header("Api-Username", api_username)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(format!("Discourse: HTTP {}", resp.status()).into());
        }

        let body: serde_json::Value = resp.json().await?;
        let posts = body["latest_posts"].as_array().cloned().unwrap_or_default();
        Ok(posts)
    }

    /// Create a reply to a topic.
    async fn create_post(
        &self,
        topic_id: u64,
        raw: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/posts.json", self.base_url);
        let chunks = split_message(raw, MAX_MESSAGE_LEN);

        for chunk in chunks {
            let body = serde_json::json!({
                "topic_id": topic_id,
                "raw": chunk,
            });

            let resp = self
                .auth_headers(self.client.post(&url))
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("Discourse API error {status}: {err_body}").into());
            }
        }

        Ok(())
    }

    /// Check if a category slug matches the filter.
    #[allow(dead_code)]
    fn matches_category(&self, category_slug: &str) -> bool {
        self.categories.is_empty() || self.categories.iter().any(|c| c == category_slug)
    }
}

#[async_trait]
impl ChannelAdapter for DiscourseAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "discourse"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("discourse".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let own_username = self.validate().await?;
        info!("Discourse adapter authenticated as {own_username}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let base_url = self.base_url.clone();
        let api_key = self.api_key.clone();
        let api_username = self.api_username.clone();
        let categories = self.categories.clone();
        let client = self.client.clone();
        let last_post_id = Arc::clone(&self.last_post_id);
        let mut shutdown_rx = self.shutdown_rx.clone();

        // Initialize last_post_id to skip historical posts
        {
            let posts = Self::fetch_latest_posts(&client, &base_url, &api_key, &api_username, 0)
                .await
                .unwrap_or_default();

            if let Some(latest) = posts.first() {
                let id = latest["id"].as_u64().unwrap_or(0);
                *last_post_id.write().await = id;
            }
        }

        let poll_interval = Duration::from_secs(POLL_INTERVAL_SECS);

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("Discourse adapter shutting down");
                            break;
                        }
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                let current_last = *last_post_id.read().await;

                let poll_result =
                    Self::fetch_latest_posts(&client, &base_url, &api_key, &api_username, 0)
                        .await
                        .map_err(|e| e.to_string());

                let posts = match poll_result {
                    Ok(p) => {
                        backoff = Duration::from_secs(1);
                        p
                    }
                    Err(msg) => {
                        warn!("Discourse: poll error: {msg}, backing off {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(120));
                        continue;
                    }
                };

                let mut max_id = current_last;

                // Process posts in chronological order (API returns newest first)
                for post in posts.iter().rev() {
                    let post_id = post["id"].as_u64().unwrap_or(0);
                    if post_id <= current_last {
                        continue;
                    }

                    let username = post["username"].as_str().unwrap_or("unknown");
                    // Skip own posts
                    if username == own_username || username == api_username {
                        continue;
                    }

                    let raw = post["raw"].as_str().unwrap_or("");
                    if raw.is_empty() {
                        continue;
                    }

                    // Category filter
                    let category_slug = post["category_slug"].as_str().unwrap_or("");
                    if !categories.is_empty() && !categories.iter().any(|c| c == category_slug) {
                        continue;
                    }

                    let topic_id = post["topic_id"].as_u64().unwrap_or(0);
                    let topic_slug = post["topic_slug"].as_str().unwrap_or("").to_string();
                    let post_number = post["post_number"].as_u64().unwrap_or(0);
                    let display_name = post["display_username"]
                        .as_str()
                        .unwrap_or(username)
                        .to_string();

                    if post_id > max_id {
                        max_id = post_id;
                    }

                    let content = if raw.starts_with('/') {
                        let parts: Vec<&str> = raw.splitn(2, ' ').collect();
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
                        ChannelContent::Text(raw.to_string())
                    };

                    let msg = ChannelMessage {
                        channel: ChannelType::Custom("discourse".to_string()),
                        platform_message_id: format!("discourse-post-{}", post_id),
                        sender: ChannelUser {
                            platform_id: username.to_string(),
                            display_name,
                            openfang_user: None,
                        },
                        content,
                        target_agent: None,
                        timestamp: Utc::now(),
                        is_group: true,
                        thread_id: Some(format!("topic-{}", topic_id)),
                        metadata: {
                            let mut m = HashMap::new();
                            m.insert(
                                "topic_id".to_string(),
                                serde_json::Value::Number(topic_id.into()),
                            );
                            m.insert(
                                "topic_slug".to_string(),
                                serde_json::Value::String(topic_slug),
                            );
                            m.insert(
                                "post_number".to_string(),
                                serde_json::Value::Number(post_number.into()),
                            );
                            m.insert(
                                "category".to_string(),
                                serde_json::Value::String(category_slug.to_string()),
                            );
                            m
                        },
                    };

                    if tx.send(msg).await.is_err() {
                        return;
                    }
                }

                if max_id > current_last {
                    *last_post_id.write().await = max_id;
                }
            }

            info!("Discourse polling loop stopped");
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

        // Extract topic_id from user.platform_id or metadata
        // Convention: platform_id holds the topic_id for replies
        let topic_id: u64 = user.platform_id.parse().unwrap_or(0);

        if topic_id == 0 {
            return Err("Discourse: cannot send without topic_id in platform_id".into());
        }

        self.create_post(topic_id, &text).await
    }

    async fn send_in_thread(
        &self,
        _user: &ChannelUser,
        content: ChannelContent,
        thread_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(t) => t,
            _ => "(Unsupported content type)".to_string(),
        };

        // thread_id format: "topic-{id}"
        let topic_id: u64 = thread_id
            .strip_prefix("topic-")
            .unwrap_or(thread_id)
            .parse()
            .map_err(|_| "Discourse: invalid thread_id format")?;

        self.create_post(topic_id, &text).await
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Discourse does not have typing indicators.
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
    fn test_discourse_adapter_creation() {
        let adapter = DiscourseAdapter::new(
            "https://forum.example.com".to_string(),
            "api-key-123".to_string(),
            "system".to_string(),
            vec!["general".to_string()],
        );
        assert_eq!(adapter.name(), "discourse");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("discourse".to_string())
        );
    }

    #[test]
    fn test_discourse_url_normalization() {
        let adapter = DiscourseAdapter::new(
            "https://forum.example.com/".to_string(),
            "key".to_string(),
            "bot".to_string(),
            vec![],
        );
        assert_eq!(adapter.base_url, "https://forum.example.com");
    }

    #[test]
    fn test_discourse_category_filter() {
        let adapter = DiscourseAdapter::new(
            "https://forum.example.com".to_string(),
            "key".to_string(),
            "bot".to_string(),
            vec!["dev".to_string(), "support".to_string()],
        );
        assert!(adapter.matches_category("dev"));
        assert!(adapter.matches_category("support"));
        assert!(!adapter.matches_category("random"));
    }

    #[test]
    fn test_discourse_category_filter_empty_allows_all() {
        let adapter = DiscourseAdapter::new(
            "https://forum.example.com".to_string(),
            "key".to_string(),
            "bot".to_string(),
            vec![],
        );
        assert!(adapter.matches_category("anything"));
    }

    #[test]
    fn test_discourse_auth_headers() {
        let adapter = DiscourseAdapter::new(
            "https://forum.example.com".to_string(),
            "my-api-key".to_string(),
            "bot-user".to_string(),
            vec![],
        );
        let builder = adapter.client.get("https://example.com");
        let builder = adapter.auth_headers(builder);
        let request = builder.build().unwrap();
        assert_eq!(request.headers().get("Api-Key").unwrap(), "my-api-key");
        assert_eq!(request.headers().get("Api-Username").unwrap(), "bot-user");
    }
}
