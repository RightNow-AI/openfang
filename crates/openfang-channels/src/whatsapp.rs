//! WhatsApp Cloud API channel adapter.
//!
//! Uses the official WhatsApp Business Cloud API to send and receive messages.
//! Requires a webhook endpoint for incoming messages and the Cloud API for outgoing.

use crate::types::{ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tracing::{error, info};
use zeroize::Zeroizing;

const MAX_MESSAGE_LEN: usize = 4096;
const MAX_SEEN_MESSAGE_IDS: usize = 10_000;

/// WhatsApp Cloud API adapter.
///
/// Supports two modes:
/// - **Cloud API mode**: Uses the official WhatsApp Business Cloud API (requires Meta dev account).
/// - **Web/QR mode**: Routes outgoing messages through a local Baileys-based gateway process.
///
/// Mode is selected automatically: if `gateway_url` is set (from `WHATSAPP_WEB_GATEWAY_URL`),
/// the adapter uses Web mode. Otherwise it falls back to Cloud API mode.
pub struct WhatsAppAdapter {
    /// WhatsApp Business phone number ID (Cloud API mode).
    phone_number_id: String,
    /// SECURITY: Access token is zeroized on drop.
    access_token: Zeroizing<String>,
    /// SECURITY: Verify token is zeroized on drop.
    verify_token: Zeroizing<String>,
    /// SECURITY: Meta app secret used to validate webhook signatures.
    app_secret: Zeroizing<String>,
    /// Port to listen for webhook callbacks (Cloud API mode).
    webhook_port: u16,
    /// HTTP client.
    client: reqwest::Client,
    /// Allowed phone numbers (empty = allow all).
    allowed_users: Vec<String>,
    /// Optional WhatsApp Web gateway URL for QR/Web mode (e.g. "http://127.0.0.1:3009").
    gateway_url: Option<String>,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl WhatsAppAdapter {
    /// Create a new WhatsApp Cloud API adapter.
    pub fn new(
        phone_number_id: String,
        access_token: String,
        verify_token: String,
        app_secret: String,
        webhook_port: u16,
        allowed_users: Vec<String>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            phone_number_id,
            access_token: Zeroizing::new(access_token),
            verify_token: Zeroizing::new(verify_token),
            app_secret: Zeroizing::new(app_secret),
            webhook_port,
            client: reqwest::Client::new(),
            allowed_users,
            gateway_url: None,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Create a new WhatsApp adapter with gateway URL for Web/QR mode.
    ///
    /// When `gateway_url` is `Some`, outgoing messages are sent via `POST {gateway_url}/message/send`
    /// instead of the Cloud API. Incoming messages are handled by the gateway itself.
    pub fn with_gateway(mut self, gateway_url: Option<String>) -> Self {
        self.gateway_url = gateway_url.filter(|u| !u.is_empty());
        self
    }

    /// Send a text message via the WhatsApp Cloud API.
    async fn api_send_message(
        &self,
        to: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://graph.facebook.com/v21.0/{}/messages",
            self.phone_number_id
        );

        // Split long messages
        let chunks = crate::types::split_message(text, MAX_MESSAGE_LEN);
        for chunk in chunks {
            let body = serde_json::json!({
                "messaging_product": "whatsapp",
                "to": to,
                "type": "text",
                "text": { "body": chunk }
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(&*self.access_token)
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                error!("WhatsApp API error {status}: {body}");
                return Err(format!("WhatsApp API error {status}: {body}").into());
            }
        }

        Ok(())
    }

    /// Mark a message as read.
    #[allow(dead_code)]
    async fn api_mark_read(&self, message_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://graph.facebook.com/v21.0/{}/messages",
            self.phone_number_id
        );

        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "status": "read",
            "message_id": message_id
        });

        let _ = self
            .client
            .post(&url)
            .bearer_auth(&*self.access_token)
            .json(&body)
            .send()
            .await;

        Ok(())
    }

    /// Send a text message via the WhatsApp Web gateway.
    async fn gateway_send_message(
        &self,
        gateway_url: &str,
        to: &str,
        text: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/message/send", gateway_url.trim_end_matches('/'));
        let body = serde_json::json!({ "to": to, "text": text });

        let resp = self.client.post(&url).json(&body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            error!("WhatsApp gateway error {status}: {body}");
            return Err(format!("WhatsApp gateway error {status}: {body}").into());
        }

        Ok(())
    }

    /// Check if a phone number is allowed.
    #[allow(dead_code)]
    fn is_allowed(&self, phone: &str) -> bool {
        is_allowed_phone(&self.allowed_users, phone)
    }

    /// Returns true if this adapter is configured for Web/QR gateway mode.
    #[allow(dead_code)]
    pub fn is_gateway_mode(&self) -> bool {
        self.gateway_url.is_some()
    }
}

fn verify_meta_signature(app_secret: &str, body: &[u8], signature: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let Some(signature) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(signature) = hex::decode(signature) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(app_secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&signature).is_ok()
}

fn verify_webhook_challenge(
    query: &HashMap<String, String>,
    verify_token: &str,
) -> Option<String> {
    let mode = query.get("hub.mode").map(String::as_str).unwrap_or("");
    let token = query
        .get("hub.verify_token")
        .map(String::as_str)
        .unwrap_or("");
    if !verify_token.is_empty() && mode == "subscribe" && token == verify_token {
        query.get("hub.challenge").cloned()
    } else {
        None
    }
}

fn should_accept_message_id(seen: &mut HashSet<String>, message_id: &str) -> bool {
    if message_id.is_empty() || seen.contains(message_id) {
        return message_id.is_empty();
    }
    if seen.len() >= MAX_SEEN_MESSAGE_IDS {
        seen.clear();
    }
    seen.insert(message_id.to_string());
    true
}

fn build_webhook_router(
    verify_token: Arc<Zeroizing<String>>,
    app_secret: Arc<Zeroizing<String>>,
    allowed_users: Arc<Vec<String>>,
    seen_message_ids: Arc<Mutex<HashSet<String>>>,
    tx: Arc<mpsc::Sender<ChannelMessage>>,
) -> axum::Router {
    axum::Router::new()
        .route(
            "/webhook",
            axum::routing::get({
                let verify_token = Arc::clone(&verify_token);
                move |query: axum::extract::Query<HashMap<String, String>>| {
                    let verify_token = Arc::clone(&verify_token);
                    async move {
                        match verify_webhook_challenge(&query, verify_token.as_str()) {
                            Some(challenge) => (axum::http::StatusCode::OK, challenge),
                            None => (axum::http::StatusCode::FORBIDDEN, String::new()),
                        }
                    }
                }
            })
            .post({
                let app_secret = Arc::clone(&app_secret);
                let allowed_users = Arc::clone(&allowed_users);
                let seen_message_ids = Arc::clone(&seen_message_ids);
                move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                    let app_secret = Arc::clone(&app_secret);
                    let allowed_users = Arc::clone(&allowed_users);
                    let seen_message_ids = Arc::clone(&seen_message_ids);
                    let tx = Arc::clone(&tx);
                    async move {
                        let signature = headers
                            .get("x-hub-signature-256")
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or_default();
                        if !verify_meta_signature(app_secret.as_str(), &body, signature) {
                            return axum::http::StatusCode::UNAUTHORIZED;
                        }
                        let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) else {
                            return axum::http::StatusCode::BAD_REQUEST;
                        };
                        for message in parse_whatsapp_webhook(&payload, &allowed_users) {
                            let message_id = message.platform_message_id.clone();
                            let accepted = {
                                let mut seen = seen_message_ids.lock().await;
                                should_accept_message_id(&mut seen, &message_id)
                            };
                            if !accepted {
                                continue;
                            }
                            if tx.send(message).await.is_err() {
                                if !message_id.is_empty() {
                                    seen_message_ids.lock().await.remove(&message_id);
                                }
                                return axum::http::StatusCode::SERVICE_UNAVAILABLE;
                            }
                        }
                        axum::http::StatusCode::OK
                    }
                }
            }),
        )
        .layer(axum::extract::DefaultBodyLimit::max(1_048_576))
}

fn is_allowed_phone(allowed_users: &[String], phone: &str) -> bool {
    allowed_users.is_empty()
        || allowed_users.iter().any(|allowed| {
            allowed.trim().trim_start_matches('+') == phone.trim().trim_start_matches('+')
        })
}

fn parse_whatsapp_webhook(
    payload: &serde_json::Value,
    allowed_users: &[String],
) -> Vec<ChannelMessage> {
    if payload["object"].as_str() != Some("whatsapp_business_account") {
        return Vec::new();
    }

    let mut messages = Vec::new();
    let Some(entries) = payload["entry"].as_array() else {
        return messages;
    };

    for entry in entries {
        let Some(changes) = entry["changes"].as_array() else {
            continue;
        };
        for change in changes {
            if change["field"].as_str() != Some("messages") {
                continue;
            }
            let value = &change["value"];
            let Some(incoming) = value["messages"].as_array() else {
                continue;
            };
            for message in incoming {
                if message["type"].as_str() != Some("text") {
                    continue;
                }
                let Some(phone) = message["from"].as_str().filter(|phone| !phone.is_empty())
                else {
                    continue;
                };
                if !is_allowed_phone(allowed_users, phone) {
                    continue;
                }
                let Some(text) = message["text"]["body"].as_str().filter(|text| !text.is_empty())
                else {
                    continue;
                };

                let display_name = value["contacts"]
                    .as_array()
                    .and_then(|contacts| {
                        contacts.iter().find(|contact| {
                            contact["wa_id"].as_str() == Some(phone)
                        })
                    })
                    .and_then(|contact| contact["profile"]["name"].as_str())
                    .unwrap_or(phone)
                    .to_string();
                let content = if text.starts_with('/') {
                    let parts: Vec<&str> = text.splitn(2, ' ').collect();
                    ChannelContent::Command {
                        name: parts[0].trim_start_matches('/').to_string(),
                        args: parts
                            .get(1)
                            .map(|args| args.split_whitespace().map(String::from).collect())
                            .unwrap_or_default(),
                    }
                } else {
                    ChannelContent::Text(text.to_string())
                };
                let timestamp_seconds = message["timestamp"]
                    .as_str()
                    .and_then(|timestamp| timestamp.parse::<i64>().ok());
                let timestamp = timestamp_seconds
                    .and_then(|seconds| chrono::DateTime::<Utc>::from_timestamp(seconds, 0))
                    .unwrap_or_else(Utc::now);
                let mut metadata = HashMap::new();
                metadata.insert(
                    "phone_number_id".to_string(),
                    value["metadata"]["phone_number_id"].clone(),
                );
                metadata.insert(
                    "timestamp".to_string(),
                    serde_json::Value::Number(timestamp.timestamp().into()),
                );

                messages.push(ChannelMessage {
                    channel: ChannelType::WhatsApp,
                    platform_message_id: message["id"].as_str().unwrap_or_default().to_string(),
                    sender: ChannelUser {
                        platform_id: phone.to_string(),
                        display_name,
                        openfang_user: None,
                    },
                    content,
                    target_agent: None,
                    timestamp,
                    is_group: false,
                    thread_id: None,
                    metadata,
                });
            }
        }
    }

    messages
}

#[async_trait]
impl ChannelAdapter for WhatsAppAdapter {
    fn name(&self) -> &str {
        "whatsapp"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::WhatsApp
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let mut shutdown_rx = self.shutdown_rx.clone();

        if self.gateway_url.is_some() {
            tokio::spawn(async move {
                let _tx = tx;
                let _ = shutdown_rx.changed().await;
            });
            return Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)));
        }

        if self.app_secret.is_empty() {
            return Err("WhatsApp Cloud API requires an app secret for webhook signature validation".into());
        }
        if self.access_token.is_empty() {
            return Err("WhatsApp Cloud API requires an access token".into());
        }
        if self.phone_number_id.is_empty() {
            return Err("WhatsApp Cloud API requires a phone number ID".into());
        }
        if self.verify_token.is_empty() {
            return Err("WhatsApp Cloud API requires a webhook verify token".into());
        }

        let port = self.webhook_port;
        let verify_token = self.verify_token.clone();
        let app_secret = self.app_secret.clone();
        let allowed_users = self.allowed_users.clone();
        let seen_message_ids = Arc::new(Mutex::new(HashSet::<String>::new()));
        let (startup_tx, startup_rx) = oneshot::channel();

        info!("Starting WhatsApp webhook listener on port {port}");

        tokio::spawn(async move {
            let app_secret = std::sync::Arc::new(app_secret);
            let verify_token = std::sync::Arc::new(verify_token);
            let allowed_users = std::sync::Arc::new(allowed_users);
            let tx = std::sync::Arc::new(tx);
            let seen_message_ids = std::sync::Arc::clone(&seen_message_ids);
            let startup_tx = Some(startup_tx);
            let app = build_webhook_router(verify_token, app_secret, allowed_users, seen_message_ids, tx);

            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(error) => {
                    if let Some(startup_tx) = startup_tx {
                        let _ = startup_tx.send(Err(error.to_string()));
                    }
                    tracing::warn!("WhatsApp webhook bind failed: {error}");
                    return;
                }
            };
            if let Some(startup_tx) = startup_tx {
                let _ = startup_tx.send(Ok(()));
            }
            info!("WhatsApp webhook listening on {addr}/webhook");
            tokio::select! {
                result = axum::serve(listener, app) => {
                    if let Err(error) = result {
                        tracing::warn!("WhatsApp webhook server error: {error}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("WhatsApp adapter stopped");
                }
            }
        });

        match startup_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => return Err(format!("WhatsApp webhook bind failed: {error}").into()),
            Err(_) => return Err("WhatsApp webhook startup task exited unexpectedly".into()),
        }

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Web/QR gateway mode: route all messages through the gateway
        if let Some(ref gw) = self.gateway_url {
            let text = match &content {
                ChannelContent::Text(t) => t.clone(),
                ChannelContent::Image { caption, .. } => caption
                    .clone()
                    .unwrap_or_else(|| "(Image — not supported in Web mode)".to_string()),
                ChannelContent::File { filename, .. } => {
                    format!("(File: {filename} — not supported in Web mode)")
                }
                _ => "(Unsupported content type in Web mode)".to_string(),
            };
            // Split long messages the same way as Cloud API mode
            let chunks = crate::types::split_message(&text, MAX_MESSAGE_LEN);
            for chunk in chunks {
                self.gateway_send_message(gw, &user.platform_id, chunk)
                    .await?;
            }
            return Ok(());
        }

        // Cloud API mode (default)
        match content {
            ChannelContent::Text(text) => {
                self.api_send_message(&user.platform_id, &text).await?;
            }
            ChannelContent::Image { url, caption } => {
                let body = serde_json::json!({
                    "messaging_product": "whatsapp",
                    "to": user.platform_id,
                    "type": "image",
                    "image": {
                        "link": url,
                        "caption": caption.unwrap_or_default()
                    }
                });
                let api_url = format!(
                    "https://graph.facebook.com/v21.0/{}/messages",
                    self.phone_number_id
                );
                let resp = self
                    .client
                    .post(&api_url)
                    .bearer_auth(&*self.access_token)
                    .json(&body)
                    .send()
                    .await?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("WhatsApp API error {status}: {body}").into());
                }
            }
            ChannelContent::File { url, filename, .. } => {
                let body = serde_json::json!({
                    "messaging_product": "whatsapp",
                    "to": user.platform_id,
                    "type": "document",
                    "document": {
                        "link": url,
                        "filename": filename
                    }
                });
                let api_url = format!(
                    "https://graph.facebook.com/v21.0/{}/messages",
                    self.phone_number_id
                );
                let resp = self
                    .client
                    .post(&api_url)
                    .bearer_auth(&*self.access_token)
                    .json(&body)
                    .send()
                    .await?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("WhatsApp API error {status}: {body}").into());
                }
            }
            ChannelContent::Location { lat, lon } => {
                let body = serde_json::json!({
                    "messaging_product": "whatsapp",
                    "to": user.platform_id,
                    "type": "location",
                    "location": {
                        "latitude": lat,
                        "longitude": lon
                    }
                });
                let api_url = format!(
                    "https://graph.facebook.com/v21.0/{}/messages",
                    self.phone_number_id
                );
                let resp = self
                    .client
                    .post(&api_url)
                    .bearer_auth(&*self.access_token)
                    .json(&body)
                    .send()
                    .await?;
                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(format!("WhatsApp API error {status}: {body}").into());
                }
            }
            _ => {
                self.api_send_message(&user.platform_id, "(Unsupported content type)")
                    .await?;
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
    use axum::body::to_bytes;
    use axum::http::Request;
    use hmac::Mac;
    use tower::ServiceExt;

    #[test]
    fn test_whatsapp_adapter_creation() {
        let adapter = WhatsAppAdapter::new(
            "12345".to_string(),
            "access_token".to_string(),
            "verify_token".to_string(),
            "app_secret".to_string(),
            8443,
            vec![],
        );
        assert_eq!(adapter.name(), "whatsapp");
        assert_eq!(adapter.channel_type(), ChannelType::WhatsApp);
    }

    #[test]
    fn test_allowed_users_check() {
        let adapter = WhatsAppAdapter::new(
            "12345".to_string(),
            "token".to_string(),
            "verify".to_string(),
            "app_secret".to_string(),
            8443,
            vec!["+1234567890".to_string()],
        );
        assert!(adapter.is_allowed("+1234567890"));
        assert!(!adapter.is_allowed("+9999999999"));

        let open = WhatsAppAdapter::new(
            "12345".to_string(),
            "token".to_string(),
            "verify".to_string(),
            "app_secret".to_string(),
            8443,
            vec![],
        );
        assert!(open.is_allowed("+anything"));
    }

    #[test]
    fn test_meta_webhook_signature_validation() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let body = br#"{"object":"whatsapp_business_account"}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(b"app-secret").unwrap();
        mac.update(body);
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        assert!(verify_meta_signature("app-secret", body, &signature));
        assert!(!verify_meta_signature("wrong-secret", body, &signature));
        assert!(!verify_meta_signature("app-secret", body, "bad-signature"));
    }

    #[test]
    fn test_webhook_challenge_validation() {
        let query = HashMap::from([
            ("hub.mode".to_string(), "subscribe".to_string()),
            ("hub.verify_token".to_string(), "verify-me".to_string()),
            ("hub.challenge".to_string(), "challenge-value".to_string()),
        ]);

        assert_eq!(
            verify_webhook_challenge(&query, "verify-me"),
            Some("challenge-value".to_string())
        );
        assert_eq!(verify_webhook_challenge(&query, "wrong-token"), None);
        assert_eq!(verify_webhook_challenge(&query, ""), None);
    }

    #[test]
    fn test_duplicate_message_ids_are_rejected() {
        let mut seen = HashSet::new();

        assert!(should_accept_message_id(&mut seen, "wamid.test"));
        assert!(!should_accept_message_id(&mut seen, "wamid.test"));
        assert!(should_accept_message_id(&mut seen, ""));
    }

    #[tokio::test]
    async fn test_webhook_router_end_to_end() {
        let (tx, mut rx) = mpsc::channel(4);
        let app = build_webhook_router(
            Arc::new(Zeroizing::new("verify-me".to_string())),
            Arc::new(Zeroizing::new("app-secret".to_string())),
            Arc::new(vec![]),
            Arc::new(Mutex::new(HashSet::new())),
            Arc::new(tx),
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/webhook?hub.mode=subscribe&hub.verify_token=verify-me&hub.challenge=abc")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), usize::MAX).await.unwrap().as_ref(),
            b"abc"
        );

        let body = br#"{"object":"whatsapp_business_account","entry":[{"changes":[{"field":"messages","value":{"messages":[{"from":"15551234567","id":"wamid.test","timestamp":"1700000000","type":"text","text":{"body":"hello"}}]}}]}]}"#;
        let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(b"app-secret").unwrap();
        mac.update(body);
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/webhook")
                        .header("x-hub-signature-256", &signature)
                        .body(axum::body::Body::from(body.to_vec()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
        }
        assert_eq!(rx.try_recv().unwrap().platform_message_id, "wamid.test");
        assert!(rx.try_recv().is_err());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .body(axum::body::Body::from(vec![b'x'; 1_048_577]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn test_parse_whatsapp_inbound_message_and_allowlist() {
        let payload = serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "changes": [{
                    "field": "messages",
                    "value": {
                        "metadata": { "phone_number_id": "12345" },
                        "contacts": [{
                            "wa_id": "15551234567",
                            "profile": { "name": "Alex" }
                        }],
                        "messages": [{
                            "from": "15551234567",
                            "id": "wamid.test",
                            "timestamp": "1700000000",
                            "type": "text",
                            "text": { "body": "hello" }
                        }]
                    }
                }]
            }]
        });

        let messages = parse_whatsapp_webhook(&payload, &["+15551234567".to_string()]);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender.display_name, "Alex");
        assert_eq!(messages[0].platform_message_id, "wamid.test");
        assert!(matches!(&messages[0].content, ChannelContent::Text(text) if text == "hello"));
        assert!(parse_whatsapp_webhook(&payload, &["15550000000".to_string()]).is_empty());
    }
}
