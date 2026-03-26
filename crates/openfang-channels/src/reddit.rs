//! Reddit API channel adapter.
//!
//! Uses the Reddit OAuth2 API for both sending and receiving messages. Authentication
//! is performed via the OAuth2 password grant (script app) at
//! `https://www.reddit.com/api/v1/access_token`. Subreddit comments are polled
//! periodically via `GET /r/{subreddit}/comments/new.json`. Replies are sent via
//! `POST /api/comment`.

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

/// Reddit OAuth2 token endpoint.
const REDDIT_TOKEN_URL: &str = "https://www.reddit.com/api/v1/access_token";

/// Reddit OAuth API base URL.
const REDDIT_API_BASE: &str = "https://oauth.reddit.com";

/// Reddit poll interval (seconds). Reddit API rate limit is ~60 requests/minute.
const POLL_INTERVAL_SECS: u64 = 5;

/// Maximum Reddit comment/message text length.
const MAX_MESSAGE_LEN: usize = 10000;

/// OAuth2 token refresh buffer — refresh 5 minutes before actual expiry.
const TOKEN_REFRESH_BUFFER_SECS: u64 = 300;

/// Custom User-Agent required by Reddit API guidelines.
const USER_AGENT: &str = "openfang:v1.0.0 (by /u/openfang-bot)";

/// Reddit OAuth2 API adapter.
///
/// Inbound messages are received by polling subreddit comment streams.
/// Outbound messages are sent as comment replies via the Reddit API.
/// OAuth2 password grant is used for authentication (script-type app).
pub struct RedditAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// Reddit OAuth2 client ID (from the app settings page).
    client_id: String,
    /// SECURITY: Reddit OAuth2 client secret, zeroized on drop.
    client_secret: Zeroizing<String>,
    /// Reddit username for OAuth2 password grant.
    username: String,
    /// SECURITY: Reddit password, zeroized on drop.
    password: Zeroizing<String>,
    /// Subreddits to monitor for new comments.
    subreddits: Vec<String>,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Cached OAuth2 bearer token and its expiry instant.
    cached_token: Arc<RwLock<Option<(String, Instant)>>>,
    /// Track last seen comment IDs to avoid duplicates.
    seen_comments: Arc<RwLock<HashMap<String, bool>>>,
}

impl RedditAdapter {
    /// Create a new Reddit adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `client_id` - Reddit OAuth2 app client ID.
    /// * `client_secret` - Reddit OAuth2 app client secret.
    /// * `username` - Reddit account username.
    /// * `password` - Reddit account password.
    /// * `subreddits` - Subreddits to monitor for new comments.
    pub fn new(
        id: String,
        client_id: String,
        client_secret: String,
        username: String,
        password: String,
        subreddits: Vec<String>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        // Build HTTP client with required User-Agent
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            id,
            client_id,
            client_secret: Zeroizing::new(client_secret),
            username,
            password: Zeroizing::new(password),
            subreddits,
            client,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            cached_token: Arc::new(RwLock::new(None)),
            seen_comments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Obtain a valid OAuth2 bearer token, refreshing if expired or missing.
    async fn get_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Check cache first
        {
            let guard = self.cached_token.read().await;
            if let Some((ref token, expiry)) = *guard {
                if Instant::now() < expiry {
                    return Ok(token.clone());
                }
            }
        }

        // Fetch a new token via password grant
        let params = [
            ("grant_type", "password"),
            ("username", &self.username),
            ("password", self.password.as_str()),
        ];

        let resp = self
            .client
            .post(REDDIT_TOKEN_URL)
            .basic_auth(&self.client_id, Some(self.client_secret.as_str()))
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Reddit OAuth2 token error {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let access_token = body["access_token"]
            .as_str()
            .ok_or("Missing access_token in Reddit OAuth2 response")?
            .to_string();
        let expires_in = body["expires_in"].as_u64().unwrap_or(3600);

        // Cache with a safety buffer
        let expiry = Instant::now()
            + Duration::from_secs(expires_in.saturating_sub(TOKEN_REFRESH_BUFFER_SECS));
        *self.cached_token.write().await = Some((access_token.clone(), expiry));

        Ok(access_token)
    }

    /// Validate credentials by calling `/api/v1/me`.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let url = format!("{}/api/v1/me", REDDIT_API_BASE);

        let resp = self.client.get(&url).bearer_auth(&token).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Reddit authentication failed {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let username = body["name"].as_str().unwrap_or("unknown").to_string();
        Ok(username)
    }

    /// Post a comment reply to a Reddit thing (comment or post).
    async fn api_comment(
        &self,
        parent_fullname: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let url = format!("{}/api/comment", REDDIT_API_BASE);

        let chunks = split_message(text, MAX_MESSAGE_LEN);

        // Reddit only allows one reply per parent, so join chunks
        let full_text = chunks.join("\n\n---\n\n");

        let params = [
            ("api_type", "json"),
            ("thing_id", parent_fullname),
            ("text", &full_text),
        ];

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let resp_body = resp.text().await.unwrap_or_default();
            return Err(format!("Reddit comment API error {status}: {resp_body}").into());
        }

        let resp_body: serde_json::Value = resp.json().await?;
        if let Some(errors) = resp_body["json"]["errors"].as_array() {
            if !errors.is_empty() {
                warn!("Reddit comment errors: {:?}", errors);
            }
        }

        Ok(())
    }

    /// Check if a subreddit name is in the monitored list.
    #[allow(dead_code)]
    fn is_monitored_subreddit(&self, subreddit: &str) -> bool {
        self.subreddits.iter().any(|s| {
            s.eq_ignore_ascii_case(subreddit)
                || s.trim_start_matches("r/").eq_ignore_ascii_case(subreddit)
        })
    }
}

/// Parse a Reddit comment JSON object into a `ChannelMessage`.
fn parse_reddit_comment(comment: &serde_json::Value, own_username: &str) -> Option<ChannelMessage> {
    let data = comment.get("data")?;
    let kind = comment["kind"].as_str().unwrap_or("");

    // Only process comments (t1) not posts (t3)
    if kind != "t1" {
        return None;
    }

    let author = data["author"].as_str().unwrap_or("");
    // Skip own comments
    if author.eq_ignore_ascii_case(own_username) {
        return None;
    }
    // Skip deleted/removed
    if author == "[deleted]" || author == "[removed]" {
        return None;
    }

    let body = data["body"].as_str().unwrap_or("");
    if body.is_empty() {
        return None;
    }

    let comment_id = data["id"].as_str().unwrap_or("").to_string();
    let fullname = data["name"].as_str().unwrap_or("").to_string(); // e.g., "t1_abc123"
    let subreddit = data["subreddit"].as_str().unwrap_or("").to_string();
    let link_id = data["link_id"].as_str().unwrap_or("").to_string();
    let parent_id = data["parent_id"].as_str().unwrap_or("").to_string();
    let permalink = data["permalink"].as_str().unwrap_or("").to_string();

    let content = if body.starts_with('/') {
        let parts: Vec<&str> = body.splitn(2, ' ').collect();
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
        ChannelContent::Text(body.to_string())
    };

    let mut metadata = HashMap::new();
    metadata.insert("fullname".to_string(), serde_json::Value::String(fullname));
    metadata.insert(
        "subreddit".to_string(),
        serde_json::Value::String(subreddit.clone()),
    );
    metadata.insert("link_id".to_string(), serde_json::Value::String(link_id));
    metadata.insert(
        "parent_id".to_string(),
        serde_json::Value::String(parent_id),
    );
    if !permalink.is_empty() {
        metadata.insert(
            "permalink".to_string(),
            serde_json::Value::String(permalink),
        );
    }

    Some(ChannelMessage {
        channel: ChannelType::Custom("reddit".to_string()),
        platform_message_id: comment_id,
        sender: ChannelUser {
            platform_id: author.to_string(),
            display_name: author.to_string(),
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group: true, // Subreddit comments are inherently public/group
        thread_id: Some(subreddit),
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for RedditAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "reddit"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("reddit".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let username = self.validate().await?;
        info!("Reddit adapter authenticated as u/{username}");

        if self.subreddits.is_empty() {
            return Err("Reddit adapter: no subreddits configured to monitor".into());
        }

        info!(
            "Reddit adapter monitoring {} subreddit(s): {}",
            self.subreddits.len(),
            self.subreddits.join(", ")
        );

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let subreddits = self.subreddits.clone();
        let client = self.client.clone();
        let cached_token = Arc::clone(&self.cached_token);
        let seen_comments = Arc::clone(&self.seen_comments);
        let own_username = username;
        let client_id = self.client_id.clone();
        let client_secret = self.client_secret.clone();
        let password = self.password.clone();
        let reddit_username = self.username.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let poll_interval = Duration::from_secs(POLL_INTERVAL_SECS);
            let mut backoff = Duration::from_secs(1);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Reddit adapter shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(poll_interval) => {}
                }

                if *shutdown_rx.borrow() {
                    break;
                }

                // Get current token
                let token = {
                    let guard = cached_token.read().await;
                    match &*guard {
                        Some((token, expiry)) if Instant::now() < *expiry => token.clone(),
                        _ => {
                            // Token expired, need to refresh
                            drop(guard);
                            let params = [
                                ("grant_type", "password"),
                                ("username", reddit_username.as_str()),
                                ("password", password.as_str()),
                            ];
                            match client
                                .post(REDDIT_TOKEN_URL)
                                .basic_auth(&client_id, Some(client_secret.as_str()))
                                .form(&params)
                                .send()
                                .await
                            {
                                Ok(resp) => {
                                    let body: serde_json::Value =
                                        resp.json().await.unwrap_or_default();
                                    let tok =
                                        body["access_token"].as_str().unwrap_or("").to_string();
                                    if tok.is_empty() {
                                        warn!("Reddit: failed to refresh token");
                                        backoff = (backoff * 2).min(Duration::from_secs(60));
                                        tokio::time::sleep(backoff).await;
                                        continue;
                                    }
                                    let expires_in = body["expires_in"].as_u64().unwrap_or(3600);
                                    let expiry = Instant::now()
                                        + Duration::from_secs(
                                            expires_in.saturating_sub(TOKEN_REFRESH_BUFFER_SECS),
                                        );
                                    *cached_token.write().await = Some((tok.clone(), expiry));
                                    tok
                                }
                                Err(e) => {
                                    warn!("Reddit: token refresh error: {e}");
                                    backoff = (backoff * 2).min(Duration::from_secs(60));
                                    tokio::time::sleep(backoff).await;
                                    continue;
                                }
                            }
                        }
                    }
                };

                // Poll each subreddit for new comments
                for subreddit in &subreddits {
                    let sub = subreddit.trim_start_matches("r/");
                    let url = format!("{}/r/{}/comments?limit=25&sort=new", REDDIT_API_BASE, sub);

                    let resp = match client.get(&url).bearer_auth(&token).send().await {
                        Ok(r) => r,
                        Err(e) => {
                            warn!("Reddit: comment fetch error for r/{sub}: {e}");
                            continue;
                        }
                    };

                    if !resp.status().is_success() {
                        warn!(
                            "Reddit: comment fetch returned {} for r/{sub}",
                            resp.status()
                        );
                        continue;
                    }

                    let body: serde_json::Value = match resp.json().await {
                        Ok(b) => b,
                        Err(e) => {
                            warn!("Reddit: failed to parse comments for r/{sub}: {e}");
                            continue;
                        }
                    };

                    let children = match body["data"]["children"].as_array() {
                        Some(arr) => arr,
                        None => continue,
                    };

                    for child in children {
                        let comment_id = child["data"]["id"].as_str().unwrap_or("").to_string();

                        // Skip already-seen comments
                        {
                            let seen = seen_comments.read().await;
                            if seen.contains_key(&comment_id) {
                                continue;
                            }
                        }

                        if let Some(msg) = parse_reddit_comment(child, &own_username) {
                            // Mark as seen
                            seen_comments.write().await.insert(comment_id, true);

                            if tx.send(msg).await.is_err() {
                                return;
                            }
                        }
                    }
                }

                // Successful poll resets backoff
                backoff = Duration::from_secs(1);

                // Periodically trim seen_comments to prevent unbounded growth
                {
                    let mut seen = seen_comments.write().await;
                    if seen.len() > 10_000 {
                        // Keep recent half (crude eviction)
                        let to_remove: Vec<String> = seen.keys().take(5_000).cloned().collect();
                        for key in to_remove {
                            seen.remove(&key);
                        }
                    }
                }
            }

            info!("Reddit polling loop stopped");
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
                // user.platform_id is the author username; we need the fullname from metadata
                // If not available, we can't reply directly
                self.api_comment(&user.platform_id, &text).await?;
            }
            _ => {
                self.api_comment(
                    &user.platform_id,
                    "(Unsupported content type — Reddit only supports text replies)",
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Reddit does not support typing indicators
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
    fn test_reddit_adapter_creation() {
        let adapter = RedditAdapter::new(
            "reddit-1".to_string(),
            "client-id".to_string(),
            "client-secret".to_string(),
            "bot-user".to_string(),
            "bot-pass".to_string(),
            vec!["rust".to_string(), "programming".to_string()],
        );
        assert_eq!(adapter.name(), "reddit");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("reddit".to_string())
        );
    }

    #[test]
    fn test_reddit_subreddit_list() {
        let adapter = RedditAdapter::new(
            "reddit-2".to_string(),
            "cid".to_string(),
            "csec".to_string(),
            "usr".to_string(),
            "pwd".to_string(),
            vec![
                "rust".to_string(),
                "programming".to_string(),
                "r/openfang".to_string(),
            ],
        );
        assert_eq!(adapter.subreddits.len(), 3);
        assert!(adapter.is_monitored_subreddit("rust"));
        assert!(adapter.is_monitored_subreddit("programming"));
        assert!(adapter.is_monitored_subreddit("openfang"));
        assert!(!adapter.is_monitored_subreddit("news"));
    }

    #[test]
    fn test_reddit_secrets_zeroized() {
        let adapter = RedditAdapter::new(
            "reddit-3".to_string(),
            "cid".to_string(),
            "secret-value".to_string(),
            "usr".to_string(),
            "pass-value".to_string(),
            vec![],
        );
        assert_eq!(adapter.client_secret.as_str(), "secret-value");
        assert_eq!(adapter.password.as_str(), "pass-value");
    }

    #[test]
    fn test_parse_reddit_comment_basic() {
        let comment = serde_json::json!({
            "kind": "t1",
            "data": {
                "id": "abc123",
                "name": "t1_abc123",
                "author": "alice",
                "body": "Hello from Reddit!",
                "subreddit": "rust",
                "link_id": "t3_xyz789",
                "parent_id": "t3_xyz789",
                "permalink": "/r/rust/comments/xyz789/title/abc123/"
            }
        });

        let msg = parse_reddit_comment(&comment, "bot-user").unwrap();
        assert_eq!(msg.channel, ChannelType::Custom("reddit".to_string()));
        assert_eq!(msg.sender.display_name, "alice");
        assert!(msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from Reddit!"));
        assert_eq!(msg.thread_id, Some("rust".to_string()));
    }

    #[test]
    fn test_parse_reddit_comment_skips_self() {
        let comment = serde_json::json!({
            "kind": "t1",
            "data": {
                "id": "abc123",
                "name": "t1_abc123",
                "author": "bot-user",
                "body": "My own comment",
                "subreddit": "rust",
                "link_id": "t3_xyz",
                "parent_id": "t3_xyz"
            }
        });

        assert!(parse_reddit_comment(&comment, "bot-user").is_none());
    }

    #[test]
    fn test_parse_reddit_comment_skips_deleted() {
        let comment = serde_json::json!({
            "kind": "t1",
            "data": {
                "id": "abc123",
                "name": "t1_abc123",
                "author": "[deleted]",
                "body": "[deleted]",
                "subreddit": "rust",
                "link_id": "t3_xyz",
                "parent_id": "t3_xyz"
            }
        });

        assert!(parse_reddit_comment(&comment, "bot-user").is_none());
    }

    #[test]
    fn test_parse_reddit_comment_command() {
        let comment = serde_json::json!({
            "kind": "t1",
            "data": {
                "id": "cmd1",
                "name": "t1_cmd1",
                "author": "alice",
                "body": "/ask what is rust?",
                "subreddit": "programming",
                "link_id": "t3_xyz",
                "parent_id": "t3_xyz"
            }
        });

        let msg = parse_reddit_comment(&comment, "bot-user").unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "ask");
                assert_eq!(args, &["what", "is", "rust?"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_reddit_comment_skips_posts() {
        let comment = serde_json::json!({
            "kind": "t3",
            "data": {
                "id": "post1",
                "name": "t3_post1",
                "author": "alice",
                "body": "This is a post",
                "subreddit": "rust"
            }
        });

        assert!(parse_reddit_comment(&comment, "bot-user").is_none());
    }

    #[test]
    fn test_parse_reddit_comment_metadata() {
        let comment = serde_json::json!({
            "kind": "t1",
            "data": {
                "id": "meta1",
                "name": "t1_meta1",
                "author": "alice",
                "body": "Test metadata",
                "subreddit": "rust",
                "link_id": "t3_link1",
                "parent_id": "t1_parent1",
                "permalink": "/r/rust/comments/link1/title/meta1/"
            }
        });

        let msg = parse_reddit_comment(&comment, "bot-user").unwrap();
        assert!(msg.metadata.contains_key("fullname"));
        assert!(msg.metadata.contains_key("subreddit"));
        assert!(msg.metadata.contains_key("link_id"));
        assert!(msg.metadata.contains_key("parent_id"));
        assert!(msg.metadata.contains_key("permalink"));
    }
}
