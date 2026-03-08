//! Microsoft Teams channel adapter for the OpenFang channel bridge.
//!
//! Uses Bot Framework v3 REST API for sending messages and a lightweight axum
//! HTTP webhook server for receiving inbound activities. OAuth2 client credentials
//! flow is used to obtain and cache access tokens for outbound API calls.

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

/// OAuth2 token endpoint for Bot Framework.
const OAUTH_TOKEN_URL: &str =
    "https://login.microsoftonline.com/botframework.com/oauth2/v2.0/token";

/// Maximum Teams message length (characters).
const MAX_MESSAGE_LEN: usize = 4096;

/// OAuth2 token refresh buffer — refresh 5 minutes before actual expiry.
const TOKEN_REFRESH_BUFFER_SECS: u64 = 300;

/// Microsoft Teams Bot Framework v3 adapter.
///
/// Inbound messages arrive via an axum HTTP webhook on `POST /api/messages`.
/// Outbound messages are sent via the Bot Framework v3 REST API using a
/// cached OAuth2 bearer token (client credentials flow).
pub struct TeamsAdapter {
    /// Bot Framework App ID (also called "Microsoft App ID").
    app_id: String,
    /// SECURITY: App password is zeroized on drop to prevent memory disclosure.
    app_password: Zeroizing<String>,
    /// Port on which the inbound webhook HTTP server listens.
    webhook_port: u16,
    /// Restrict inbound activities to specific Azure AD tenant IDs (empty = allow all).
    allowed_tenants: Vec<String>,
    /// HTTP client for outbound API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Cached OAuth2 bearer token and its expiry instant.
    cached_token: Arc<RwLock<Option<(String, Instant)>>>,
}

impl TeamsAdapter {
    /// Create a new Teams adapter.
    ///
    /// * `app_id` — Bot Framework application ID.
    /// * `app_password` — Bot Framework application password (client secret).
    /// * `webhook_port` — Local port for the inbound webhook HTTP server.
    /// * `allowed_tenants` — Azure AD tenant IDs to accept (empty = accept all).
    pub fn new(
        app_id: String,
        app_password: String,
        webhook_port: u16,
        allowed_tenants: Vec<String>,
        client: reqwest::Client,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_id,
            app_password: Zeroizing::new(app_password),
            webhook_port,
            allowed_tenants,
            client,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            cached_token: Arc::new(RwLock::new(None)),
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

        // Fetch a new token via client credentials flow
        let params = [
            ("grant_type", "client_credentials"),
            ("client_id", &self.app_id),
            ("client_secret", self.app_password.as_str()),
            ("scope", "https://api.botframework.com/.default"),
        ];

        let resp = self
            .client
            .post(OAUTH_TOKEN_URL)
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Teams OAuth2 token error {status}: {body}").into());
        }

        let body: serde_json::Value = resp.json().await?;
        let access_token = body["access_token"]
            .as_str()
            .ok_or("Missing access_token in OAuth2 response")?
            .to_string();
        let expires_in = body["expires_in"].as_u64().unwrap_or(3600);

        // Cache with a safety buffer
        let expiry = Instant::now()
            + Duration::from_secs(expires_in.saturating_sub(TOKEN_REFRESH_BUFFER_SECS));
        *self.cached_token.write().await = Some((access_token.clone(), expiry));

        Ok(access_token)
    }

    /// Send a text reply to a Teams conversation via Bot Framework v3.
    ///
    /// * `service_url` — The per-conversation service URL provided in inbound activities.
    /// * `conversation_id` — The Teams conversation ID.
    /// * `text` — The message text to send.
    async fn api_send_message(
        &self,
        service_url: &str,
        conversation_id: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let url = format!(
            "{}/v3/conversations/{}/activities",
            service_url.trim_end_matches('/'),
            conversation_id
        );

        let chunks = split_message(text, MAX_MESSAGE_LEN);
        for chunk in chunks {
            let body = serde_json::json!({
                "type": "message",
                "text": chunk,
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
                warn!("Teams API error {status}: {resp_body}");
            }
        }

        Ok(())
    }

    /// Check whether a tenant ID is allowed (empty list = allow all).
    #[allow(dead_code)]
    fn is_allowed_tenant(&self, tenant_id: &str) -> bool {
        self.allowed_tenants.is_empty() || self.allowed_tenants.iter().any(|t| t == tenant_id)
    }
}

/// Parse an inbound Bot Framework activity JSON into a `ChannelMessage`.
///
/// Returns `None` for activities that should be ignored (non-message types,
/// activities from the bot itself, activities from disallowed tenants, etc.).
fn parse_teams_activity(
    activity: &serde_json::Value,
    app_id: &str,
    allowed_tenants: &[String],
) -> Option<ChannelMessage> {
    let activity_type = activity["type"].as_str().unwrap_or("");
    if activity_type != "message" {
        return None;
    }

    // Extract sender info
    let from = activity.get("from")?;
    let from_id = from["id"].as_str().unwrap_or("");
    let from_name = from["name"].as_str().unwrap_or("Unknown");

    // Skip messages from the bot itself
    if from_id == app_id {
        return None;
    }

    // Tenant filtering
    if !allowed_tenants.is_empty() {
        let tenant_id = activity["channelData"]["tenant"]["id"]
            .as_str()
            .unwrap_or("");
        if !allowed_tenants.iter().any(|t| t == tenant_id) {
            return None;
        }
    }

    let text = activity["text"].as_str().unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let conversation_id = activity["conversation"]["id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let activity_id = activity["id"].as_str().unwrap_or("").to_string();
    let service_url = activity["serviceUrl"].as_str().unwrap_or("").to_string();

    // Determine if this is a group conversation
    let is_group = activity["conversation"]["isGroup"]
        .as_bool()
        .unwrap_or(false);

    // Parse commands (messages starting with /)
    let content = if text.starts_with('/') {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let cmd_name = &parts[0][1..];
        let args = if parts.len() > 1 {
            parts[1].split_whitespace().map(String::from).collect()
        } else {
            vec![]
        };
        ChannelContent::Command {
            name: cmd_name.to_string(),
            args,
        }
    } else {
        ChannelContent::Text(text.to_string())
    };

    let mut metadata = HashMap::new();
    // Store serviceUrl in metadata so outbound replies can use it
    if !service_url.is_empty() {
        metadata.insert(
            "serviceUrl".to_string(),
            serde_json::Value::String(service_url),
        );
    }

    Some(ChannelMessage {
        channel: ChannelType::Teams,
        platform_message_id: activity_id,
        sender: ChannelUser {
            platform_id: conversation_id,
            display_name: from_name.to_string(),
            openfang_user: None,
        },
        content,
        target_agent: None,
        timestamp: Utc::now(),
        is_group,
        thread_id: None,
        metadata,
    })
}

#[async_trait]
impl ChannelAdapter for TeamsAdapter {
    fn name(&self) -> &str {
        "teams"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Teams
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials by obtaining an initial token
        let _ = self.get_token().await?;
        info!("Teams adapter authenticated (app_id: {})", self.app_id);

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let app_id = self.app_id.clone();
        let allowed_tenants = self.allowed_tenants.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            // Build the axum webhook router
            let app_id_shared = Arc::new(app_id);
            let tenants_shared = Arc::new(allowed_tenants);
            let tx_shared = Arc::new(tx);

            let app = axum::Router::new().route(
                "/api/messages",
                axum::routing::post({
                    let app_id = Arc::clone(&app_id_shared);
                    let tenants = Arc::clone(&tenants_shared);
                    let tx = Arc::clone(&tx_shared);
                    move |body: axum::extract::Json<serde_json::Value>| {
                        let app_id = Arc::clone(&app_id);
                        let tenants = Arc::clone(&tenants);
                        let tx = Arc::clone(&tx);
                        async move {
                            if let Some(msg) = parse_teams_activity(&body, &app_id, &tenants) {
                                let _ = tx.send(msg).await;
                            }
                            axum::http::StatusCode::OK
                        }
                    }
                }),
            );

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            info!("Teams webhook server listening on {addr}");

            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    warn!("Teams webhook bind failed: {e}");
                    return;
                }
            };

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("Teams webhook server error: {e}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("Teams adapter shutting down");
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
        // We need the serviceUrl from metadata; fall back to the default Bot Framework URL
        let default_service_url = "https://smba.trafficmanager.net/teams/".to_string();
        let conversation_id = &user.platform_id;

        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(&default_service_url, conversation_id, &text)
                    .await?;
            }
            _ => {
                self.api_send_message(
                    &default_service_url,
                    conversation_id,
                    "(Unsupported content type)",
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn send_typing(&self, user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token().await?;
        let default_service_url = "https://smba.trafficmanager.net/teams/";
        let url = format!(
            "{}/v3/conversations/{}/activities",
            default_service_url.trim_end_matches('/'),
            user.platform_id
        );

        let body = serde_json::json!({
            "type": "typing",
        });

        let _ = self
            .client
            .post(&url)
            .bearer_auth(&token)
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
    fn test_teams_adapter_creation() {
        let adapter = TeamsAdapter::new(
            "app-id-123".to_string(),
            "app-password".to_string(),
            3978,
            vec![],
            reqwest::Client::new(),
        );
        assert_eq!(adapter.name(), "teams");
        assert_eq!(adapter.channel_type(), ChannelType::Teams);
    }

    #[test]
    fn test_teams_allowed_tenants() {
        let adapter = TeamsAdapter::new(
            "app-id".to_string(),
            "password".to_string(),
            3978,
            vec!["tenant-abc".to_string()],
            reqwest::Client::new(),
        );
        assert!(adapter.is_allowed_tenant("tenant-abc"));
        assert!(!adapter.is_allowed_tenant("tenant-xyz"));

        let open = TeamsAdapter::new("app-id".to_string(), "password".to_string(), 3978, vec![], reqwest::Client::new());
        assert!(open.is_allowed_tenant("any-tenant"));
    }

    #[test]
    fn test_parse_teams_activity_basic() {
        let activity = serde_json::json!({
            "type": "message",
            "id": "activity-1",
            "text": "Hello from Teams!",
            "from": {
                "id": "user-456",
                "name": "Alice"
            },
            "conversation": {
                "id": "conv-789",
                "isGroup": false
            },
            "serviceUrl": "https://smba.trafficmanager.net/teams/",
            "channelData": {
                "tenant": {
                    "id": "tenant-abc"
                }
            }
        });

        let msg = parse_teams_activity(&activity, "app-id-123", &[]).unwrap();
        assert_eq!(msg.channel, ChannelType::Teams);
        assert_eq!(msg.sender.display_name, "Alice");
        assert_eq!(msg.sender.platform_id, "conv-789");
        assert!(!msg.is_group);
        assert!(matches!(msg.content, ChannelContent::Text(ref t) if t == "Hello from Teams!"));
        assert!(msg.metadata.contains_key("serviceUrl"));
    }

    #[test]
    fn test_parse_teams_activity_skips_bot_self() {
        let activity = serde_json::json!({
            "type": "message",
            "id": "activity-1",
            "text": "Bot reply",
            "from": {
                "id": "app-id-123",
                "name": "OpenFang Bot"
            },
            "conversation": {
                "id": "conv-789"
            },
            "serviceUrl": "https://smba.trafficmanager.net/teams/"
        });

        let msg = parse_teams_activity(&activity, "app-id-123", &[]);
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_teams_activity_tenant_filter() {
        let activity = serde_json::json!({
            "type": "message",
            "id": "activity-1",
            "text": "Hello",
            "from": {
                "id": "user-1",
                "name": "Bob"
            },
            "conversation": {
                "id": "conv-1"
            },
            "serviceUrl": "https://smba.trafficmanager.net/teams/",
            "channelData": {
                "tenant": {
                    "id": "tenant-xyz"
                }
            }
        });

        // Not in allowed tenants
        let msg = parse_teams_activity(&activity, "app-id", &["tenant-abc".to_string()]);
        assert!(msg.is_none());

        // In allowed tenants
        let msg = parse_teams_activity(&activity, "app-id", &["tenant-xyz".to_string()]);
        assert!(msg.is_some());
    }

    #[test]
    fn test_parse_teams_activity_command() {
        let activity = serde_json::json!({
            "type": "message",
            "id": "activity-1",
            "text": "/agent hello-world",
            "from": {
                "id": "user-1",
                "name": "Alice"
            },
            "conversation": {
                "id": "conv-1"
            },
            "serviceUrl": "https://smba.trafficmanager.net/teams/"
        });

        let msg = parse_teams_activity(&activity, "app-id", &[]).unwrap();
        match &msg.content {
            ChannelContent::Command { name, args } => {
                assert_eq!(name, "agent");
                assert_eq!(args, &["hello-world"]);
            }
            other => panic!("Expected Command, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_teams_activity_non_message() {
        let activity = serde_json::json!({
            "type": "conversationUpdate",
            "id": "activity-1",
            "from": { "id": "user-1", "name": "Alice" },
            "conversation": { "id": "conv-1" },
            "serviceUrl": "https://smba.trafficmanager.net/teams/"
        });

        let msg = parse_teams_activity(&activity, "app-id", &[]);
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_teams_activity_empty_text() {
        let activity = serde_json::json!({
            "type": "message",
            "id": "activity-1",
            "text": "",
            "from": { "id": "user-1", "name": "Alice" },
            "conversation": { "id": "conv-1" },
            "serviceUrl": "https://smba.trafficmanager.net/teams/"
        });

        let msg = parse_teams_activity(&activity, "app-id", &[]);
        assert!(msg.is_none());
    }

    #[test]
    fn test_parse_teams_activity_group() {
        let activity = serde_json::json!({
            "type": "message",
            "id": "activity-1",
            "text": "Group hello",
            "from": { "id": "user-1", "name": "Alice" },
            "conversation": {
                "id": "conv-1",
                "isGroup": true
            },
            "serviceUrl": "https://smba.trafficmanager.net/teams/"
        });

        let msg = parse_teams_activity(&activity, "app-id", &[]).unwrap();
        assert!(msg.is_group);
    }
}
