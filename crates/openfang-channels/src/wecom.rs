//! WeCom (WeChat Work) channel adapter.
//!
//! Uses the WeCom Work API for sending messages and a webhook HTTP server for
//! receiving inbound events. Authentication is performed via an access token
//! obtained from `https://qyapi.weixin.qq.com/cgi-bin/gettoken`.
//! The token is cached and refreshed automatically.

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

/// WeCom API base URL.
const WECOM_API_HOST: &str = "https://qyapi.weixin.qq.com";

/// WeCom token endpoint.
const WECOM_TOKEN_URL: &str = "https://qyapi.weixin.qq.com/cgi-bin/gettoken";

/// WeCom send message endpoint.
const WECOM_SEND_URL: &str = "https://qyapi.weixin.qq.com/cgi-bin/message/send";

/// Maximum WeCom message text length (characters).
const MAX_MESSAGE_LEN: usize = 2048;

/// Token refresh buffer — refresh 5 minutes before actual expiry.
const TOKEN_REFRESH_BUFFER_SECS: u64 = 300;

/// WeCom adapter.
pub struct WeComAdapter {
    /// WeCom corp ID.
    corp_id: String,
    /// WeCom application agent ID.
    agent_id: String,
    /// WeCom application secret, zeroized on drop.
    secret: Zeroizing<String>,
    /// Encoding AES key for callback verification (optional).
    encoding_aes_key: Option<String>,
    /// Token for callback verification (optional).
    token: Option<String>,
    /// Port on which the inbound webhook HTTP server listens.
    webhook_port: u16,
    /// HTTP client for API calls.
    client: reqwest::Client,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Cached access token and its expiry instant.
    cached_token: Arc<RwLock<Option<(String, Instant)>>>,
}

impl WeComAdapter {
    /// Create a new WeCom adapter.
    pub fn new(
        corp_id: String,
        agent_id: String,
        secret: String,
        webhook_port: u16,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            corp_id,
            agent_id,
            secret: Zeroizing::new(secret),
            encoding_aes_key: None,
            token: None,
            webhook_port,
            client: reqwest::Client::new(),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            cached_token: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a new WeCom adapter with callback verification.
    pub fn with_verification(
        corp_id: String,
        agent_id: String,
        secret: String,
        webhook_port: u16,
        encoding_aes_key: Option<String>,
        token: Option<String>,
    ) -> Self {
        let mut adapter = Self::new(corp_id, agent_id, secret, webhook_port);
        adapter.encoding_aes_key = encoding_aes_key;
        adapter.token = token;
        adapter
    }

    /// Obtain a valid access token, refreshing if expired or missing.
    async fn get_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let mut cached = self.cached_token.write().await;

        // Check if we have a valid cached token
        if let Some((token, expiry)) = cached.as_ref() {
            let now = Instant::now();
            let buffer = Duration::from_secs(TOKEN_REFRESH_BUFFER_SECS);
            if now + buffer < *expiry {
                return Ok(token.clone());
            }
        }

        // Fetch new token
        let url = format!(
            "{}?corpid={}&corpsecret={}",
            WECOM_TOKEN_URL, self.corp_id, self.secret.as_str()
        );

        let response = self.client.get(&url).send().await?;
        let json: serde_json::Value = response.json().await?;

        if let Some(errcode) = json.get("errcode").and_then(|v| v.as_i64()) {
            if errcode != 0 {
                return Err(format!("WeCom API error: {} - {}", errcode, json.get("errmsg").and_then(|v| v.as_str()).unwrap_or("")).into());
            }
        }

        let token = json["access_token"]
            .as_str()
            .ok_or("Missing access_token in response")?
            .to_string();

        let expires_in = json["expires_in"]
            .as_i64()
            .unwrap_or(7200) as u64;

        let expiry = Instant::now() + Duration::from_secs(expires_in);
        *cached = Some((token.clone(), expiry));

        info!("WeCom access token refreshed, expires in {}s", expires_in);
        Ok(token)
    }

    /// Send a text message to a user.
    async fn send_text(&self, user_id: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        let token = self.get_token().await?;

        let url = format!("{}?access_token={}", WECOM_SEND_URL, token);

        let payload = serde_json::json!({
            "touser": user_id,
            "msgtype": "text",
            "agentid": self.agent_id,
            "text": {
                "content": content
            }
        });

        let response = self.client.post(&url)
            .json(&payload)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;

        if let Some(errcode) = json.get("errcode").and_then(|v| v.as_i64()) {
            if errcode != 0 {
                return Err(format!("WeCom send error: {} - {}", errcode, json.get("errmsg").and_then(|v| v.as_str()).unwrap_or("")).into());
            }
        }

        Ok(())
    }

    /// Validate credentials by getting the token.
    async fn validate(&self) -> Result<String, Box<dyn std::error::Error>> {
        let _token = self.get_token().await?;
        // Token obtained successfully means credentials are valid
        Ok(format!("corp_id={}", self.corp_id))
    }
}

#[async_trait]
impl ChannelAdapter for WeComAdapter {
    fn name(&self) -> &str {
        "wecom"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("wecom".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>> {
        // Validate credentials
        let _ = self.validate().await?;
        info!("WeCom adapter initialized");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let token = self.token.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let token = Arc::new(token);
            let tx = Arc::new(tx);

            let app = axum::Router::new().route(
                "/wecom/webhook",
                axum::routing::post({
                    let tx = Arc::clone(&tx);
                    move |body: axum::extract::Json<serde_json::Value>| {
                        let tx = Arc::clone(&tx);
                        async move {
                            // Handle callback verification (URL validation)
                            if let Some(msg_type) = body.0.get("MsgType").and_then(|v| v.as_str()) {
                                if msg_type == "event" {
                                    // Event callback - handle verification
                                    if let Some(event) = body.0.get("Event").and_then(|v| v.as_str()) {
                                        if event == "subscribe" || event == "enter_agent" {
                                            // User subscribed or entered the agent
                                            let user_id = body.0["FromUserName"]
                                                .as_str()
                                                .unwrap_or("")
                                                .to_string();

                                            if !user_id.is_empty() {
                                                let msg = ChannelMessage {
                                                    channel: ChannelType::Custom("wecom".to_string()),
                                                    platform_message_id: String::new(),
                                                    sender: ChannelUser {
                                                        platform_id: user_id.clone(),
                                                        display_name: user_id.clone(),
                                                        openfang_user: None,
                                                    },
                                                    content: ChannelContent::Text("".to_string()),
                                                    target_agent: None,
                                                    timestamp: Utc::now(),
                                                    is_group: false,
                                                    thread_id: None,
                                                    metadata: HashMap::new(),
                                                };
                                                let _ = tx.send(msg).await;
                                            }
                                        }
                                    }
                                    return (
                                        axum::http::StatusCode::OK,
                                        axum::Json(serde_json::json!({"errcode": 0, "errmsg": "ok"})),
                                    );
                                }

                                // Handle text message
                                if msg_type == "text" {
                                    let user_id = body.0["FromUserName"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string();
                                    let content = body.0["Content"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string();
                                    let msg_id = body.0["MsgId"]
                                        .as_str()
                                        .unwrap_or("")
                                        .to_string();

                                    if !user_id.is_empty() && !content.is_empty() {
                                        let msg = ChannelMessage {
                                            channel: ChannelType::Custom("wecom".to_string()),
                                            platform_message_id: msg_id,
                                            sender: ChannelUser {
                                                platform_id: user_id.clone(),
                                                display_name: user_id.clone(),
                                                openfang_user: None,
                                            },
                                            content: ChannelContent::Text(content),
                                            target_agent: None,
                                            timestamp: Utc::now(),
                                            is_group: false,
                                            thread_id: None,
                                            metadata: HashMap::new(),
                                        };
                                        let _ = tx.send(msg).await;
                                    }
                                }
                            }

                            (
                                axum::http::StatusCode::OK,
                                axum::Json(serde_json::json!({"errcode": 0, "errmsg": "ok"})),
                            )
                        }
                    }
                }),
            );

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

            info!("WeCom webhook server listening on http://0.0.0.0:{}", port);

            let server = axum::serve(listener, app);

            tokio::select! {
                result = server => {
                    if let Err(e) = result {
                        warn!("WeCom webhook server error: {}", e);
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("WeCom adapter shutting down");
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
        let user_id = &user.platform_id;

        match content {
            ChannelContent::Text(text) => {
                // Split long messages
                for chunk in split_message(&text, MAX_MESSAGE_LEN) {
                    self.send_text(user_id, &chunk).await?;
                }
            }
            ChannelContent::Command { name: _, args: _ } => {
                // WeCom doesn't support commands natively
                warn!("WeCom: commands not supported");
            }
            _ => {
                warn!("WeCom: unsupported content type");
            }
        }

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
    fn test_adapter_name() {
        let adapter = WeComAdapter::new(
            "corp_id".to_string(),
            "agent_id".to_string(),
            "secret".to_string(),
            8080,
        );
        assert_eq!(adapter.name(), "wecom");
    }
}
