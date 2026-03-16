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
use axum::response::IntoResponse;
use chrono::Utc;
use futures::Stream;
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{info, warn};
use zeroize::Zeroizing;

/// WeCom token endpoint.
const WECOM_TOKEN_URL: &str = "https://qyapi.weixin.qq.com/cgi-bin/gettoken";

/// WeCom send message endpoint.
const WECOM_SEND_URL: &str = "https://qyapi.weixin.qq.com/cgi-bin/message/send";

/// Maximum WeCom message text length (characters).
const MAX_MESSAGE_LEN: usize = 2048;

/// Token refresh buffer — refresh 5 minutes before actual expiry.
const TOKEN_REFRESH_BUFFER_SECS: u64 = 300;

fn decrypt_aes_cbc(key: &[u8], encrypted_base64: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    use cbc::cipher::{BlockDecryptMut, KeyIvInit};

    // Decode base64
    let mut encrypted = base64::engine::general_purpose::STANDARD
        .decode(encrypted_base64)
        .map_err(|e| format!("base64 decode error: {}", e))?;

    // IV is first 16 bytes of key
    type Aes256CbcDecrypt = cbc::Decryptor<aes::Aes256>;
    let iv = &key[..16];
    let cipher = Aes256CbcDecrypt::new(key.into(), iv.into());

    let decrypted = cipher
        .decrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut encrypted)
        .map_err(|e| format!("decrypt error: {}", e))?;

    let decrypted = decrypted.to_vec();
    let pad = decrypted
        .last()
        .copied()
        .ok_or_else(|| "decrypted payload is empty".to_string())? as usize;

    if pad == 0 || pad > 32 || decrypted.len() < pad {
        return Err(format!("invalid WeCom PKCS7 padding length: {pad}"));
    }
    if !decrypted[decrypted.len() - pad..]
        .iter()
        .all(|byte| *byte as usize == pad)
    {
        return Err("invalid WeCom PKCS7 padding bytes".to_string());
    }

    Ok(decrypted[..decrypted.len() - pad].to_vec())
}

fn is_valid_wecom_signature(
    token: &str,
    timestamp: &str,
    nonce: &str,
    encrypted_payload: &str,
    msg_signature: &str,
) -> bool {
    let mut parts = [token, timestamp, nonce, encrypted_payload];
    parts.sort_unstable();

    let mut hasher = Sha1::new();
    hasher.update(parts.concat().as_bytes());
    hex::encode(hasher.finalize()) == msg_signature
}

fn decode_wecom_payload(encoding_aes_key: &str, encrypted_payload: &str) -> Result<String, String> {
    use base64::{
        alphabet,
        engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
        Engine,
    };

    let aes_key_engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::RequireNone)
            .with_decode_allow_trailing_bits(true),
    );

    let aes_key = aes_key_engine
        .decode(encoding_aes_key)
        .map_err(|e| format!("aes key decode error: {e}"))?;
    let decrypted = decrypt_aes_cbc(&aes_key, encrypted_payload)?;

    if decrypted.len() < 20 {
        return Err("decrypted payload too short".to_string());
    }

    let msg_len =
        u32::from_be_bytes([decrypted[16], decrypted[17], decrypted[18], decrypted[19]]) as usize;
    if decrypted.len() < 20 + msg_len {
        return Err("decrypted payload shorter than declared echostr".to_string());
    }

    String::from_utf8(decrypted[20..20 + msg_len].to_vec())
        .map_err(|e| format!("echostr is not valid utf-8: {e}"))
}

fn parse_wecom_xml_fields(xml: &str) -> Result<HashMap<String, String>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("invalid xml: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "xml" {
        return Err("root element is not <xml>".to_string());
    }

    let mut fields = HashMap::new();
    for child in root.children().filter(|node| node.is_element()) {
        let value = child
            .children()
            .filter_map(|node| node.text())
            .collect::<String>()
            .trim()
            .to_string();
        fields.insert(child.tag_name().name().to_string(), value);
    }

    Ok(fields)
}

fn decode_wecom_post_body(
    body: &str,
    params: &HashMap<String, String>,
    token: Option<&str>,
    encoding_aes_key: Option<&str>,
) -> Result<HashMap<String, String>, String> {
    let parsed = parse_wecom_xml_fields(body)?;

    let Some(encrypted_payload) = parsed.get("Encrypt") else {
        return Ok(parsed);
    };

    let token = token.ok_or_else(|| "missing WeCom callback token".to_string())?;
    let timestamp = params
        .get("timestamp")
        .ok_or_else(|| "missing timestamp".to_string())?;
    let nonce = params
        .get("nonce")
        .ok_or_else(|| "missing nonce".to_string())?;
    let msg_signature = params
        .get("msg_signature")
        .ok_or_else(|| "missing msg_signature".to_string())?;

    if !is_valid_wecom_signature(token, timestamp, nonce, encrypted_payload, msg_signature) {
        return Err("invalid WeCom callback signature".to_string());
    }

    let aes_key = encoding_aes_key
        .filter(|key| !key.is_empty())
        .ok_or_else(|| "missing WeCom encoding_aes_key".to_string())?;
    let decrypted_xml = decode_wecom_payload(aes_key, encrypted_payload)?;
    parse_wecom_xml_fields(&decrypted_xml)
}

fn wecom_success_response() -> axum::response::Response {
    (
        axum::http::StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        "success",
    )
        .into_response()
}

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
    pub fn new(corp_id: String, agent_id: String, secret: String, webhook_port: u16) -> Self {
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
            WECOM_TOKEN_URL,
            self.corp_id,
            self.secret.as_str()
        );

        let response = self.client.get(&url).send().await?;
        let json: serde_json::Value = response.json().await?;

        if let Some(errcode) = json.get("errcode").and_then(|v| v.as_i64()) {
            if errcode != 0 {
                return Err(format!(
                    "WeCom API error: {} - {}",
                    errcode,
                    json.get("errmsg").and_then(|v| v.as_str()).unwrap_or("")
                )
                .into());
            }
        }

        let token = json["access_token"]
            .as_str()
            .ok_or("Missing access_token in response")?
            .to_string();

        let expires_in = json["expires_in"].as_i64().unwrap_or(7200) as u64;

        let expiry = Instant::now() + Duration::from_secs(expires_in);
        *cached = Some((token.clone(), expiry));

        info!("WeCom access token refreshed, expires in {}s", expires_in);
        Ok(token)
    }

    /// Send a text message to a user.
    async fn send_text(
        &self,
        user_id: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
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

        let response = self.client.post(&url).json(&payload).send().await?;

        let json: serde_json::Value = response.json().await?;

        if let Some(errcode) = json.get("errcode").and_then(|v| v.as_i64()) {
            if errcode != 0 {
                return Err(format!(
                    "WeCom send error: {} - {}",
                    errcode,
                    json.get("errmsg").and_then(|v| v.as_str()).unwrap_or("")
                )
                .into());
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
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials
        let _ = self.validate().await?;
        info!("WeCom adapter initialized");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let port = self.webhook_port;
        let token = self.token.clone();
        let encoding_aes_key = self.encoding_aes_key.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let token = Arc::new(token);
            let encoding_aes_key = Arc::new(encoding_aes_key);
            let tx = Arc::new(tx);

            let app = axum::Router::new().route(
                "/wecom/webhook",
                axum::routing::get({
                    let encoding_aes_key = Arc::clone(&encoding_aes_key);
                    let token = Arc::clone(&token);
                    move |axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>| {
                        let encoding_aes_key = Arc::clone(&encoding_aes_key);
                        let token = Arc::clone(&token);
                        async move {
                            // Handle callback verification (URL validation GET request)
                            // WeChat Work sends GET with msg_signature, timestamp, nonce, echostr
                            if let (Some(echostr_encoded), Some(msg_sig), Some(timestamp), Some(nonce)) = (
                                params.get("echostr"),
                                params.get("msg_signature"),
                                params.get("timestamp"),
                                params.get("nonce"),
                            ) {
                                let Some(token_str) = token.as_deref() else {
                                    return (
                                        axum::http::StatusCode::BAD_REQUEST,
                                        "missing WeCom callback token",
                                    )
                                        .into_response();
                                };

                                if !is_valid_wecom_signature(
                                    token_str,
                                    timestamp,
                                    nonce,
                                    echostr_encoded,
                                    msg_sig,
                                ) {
                                    return (
                                        axum::http::StatusCode::FORBIDDEN,
                                        "invalid WeCom callback signature",
                                    )
                                        .into_response();
                                }

                                let body = match encoding_aes_key.as_deref() {
                                    Some(aes_key) if !aes_key.is_empty() => {
                                        match decode_wecom_payload(aes_key, echostr_encoded) {
                                            Ok(echostr_plain) => echostr_plain,
                                            Err(err) => {
                                                warn!(error = %err, "Failed to decrypt WeCom echostr");
                                                return (
                                                    axum::http::StatusCode::BAD_REQUEST,
                                                    "invalid WeCom echostr",
                                                )
                                                    .into_response();
                                            }
                                        }
                                    }
                                    _ => echostr_encoded.clone(),
                                };

                                return (
                                    axum::http::StatusCode::OK,
                                    [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                                    body,
                                )
                                    .into_response();
                            }
                            (
                                axum::http::StatusCode::BAD_REQUEST,
                                "missing WeCom verification parameters",
                            )
                                .into_response()
                        }
                    }
                }).post({
                    let token = Arc::clone(&token);
                    let encoding_aes_key = Arc::clone(&encoding_aes_key);
                    let tx = Arc::clone(&tx);
                    move |axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>, body: String| {
                        let token = Arc::clone(&token);
                        let encoding_aes_key = Arc::clone(&encoding_aes_key);
                        let tx = Arc::clone(&tx);
                        async move {
                            let fields = match decode_wecom_post_body(
                                &body,
                                &params,
                                token.as_deref(),
                                encoding_aes_key.as_deref(),
                            ) {
                                Ok(fields) => fields,
                                Err(err) => {
                                    warn!(error = %err, "Failed to parse WeCom callback body");
                                    return (
                                        axum::http::StatusCode::BAD_REQUEST,
                                        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                                        "invalid WeCom callback body",
                                    )
                                        .into_response();
                                }
                            };

                            let msg_type = fields.get("MsgType").map(String::as_str).unwrap_or("");
                            let user_id = fields
                                .get("FromUserName")
                                .cloned()
                                .unwrap_or_default();
                            let event = fields.get("Event").map(String::as_str).unwrap_or("");

                            info!(
                                msg_type = msg_type,
                                event = event,
                                from_user = %user_id,
                                "Received WeCom callback"
                            );

                            if msg_type == "event" {
                                if (event == "subscribe" || event == "enter_agent")
                                    && !user_id.is_empty()
                                {
                                    let msg = ChannelMessage {
                                        channel: ChannelType::Custom("wecom".to_string()),
                                        platform_message_id: String::new(),
                                        sender: ChannelUser {
                                            platform_id: user_id.clone(),
                                            display_name: user_id.clone(),
                                            openfang_user: None,
                                        },
                                        content: ChannelContent::Text(String::new()),
                                        target_agent: None,
                                        timestamp: Utc::now(),
                                        is_group: false,
                                        thread_id: None,
                                        metadata: HashMap::new(),
                                    };
                                    let _ = tx.send(msg).await;
                                }

                                return wecom_success_response();
                            }

                            if msg_type == "text" {
                                let content = fields.get("Content").cloned().unwrap_or_default();
                                let msg_id = fields.get("MsgId").cloned().unwrap_or_default();

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

                            wecom_success_response()
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
                    self.send_text(user_id, chunk).await?;
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

    #[test]
    fn test_adapter_channel_type() {
        let adapter = WeComAdapter::new(
            "corp_id".to_string(),
            "agent_id".to_string(),
            "secret".to_string(),
            8080,
        );
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("wecom".to_string())
        );
    }

    #[test]
    fn test_adapter_with_verification() {
        let adapter = WeComAdapter::with_verification(
            "corp_id".to_string(),
            "agent_id".to_string(),
            "secret".to_string(),
            8080,
            Some("encoding_aes_key".to_string()),
            Some("token".to_string()),
        );
        assert_eq!(adapter.name(), "wecom");
    }

    #[test]
    fn test_max_message_length() {
        // MAX_MESSAGE_LEN should be 2048 for WeCom
        assert_eq!(MAX_MESSAGE_LEN, 2048);
    }

    #[test]
    fn test_token_refresh_buffer() {
        // Token refresh buffer should be 5 minutes
        assert_eq!(TOKEN_REFRESH_BUFFER_SECS, 300);
    }

    #[test]
    fn test_wecom_signature_validation() {
        assert!(is_valid_wecom_signature(
            "token",
            "1710000000",
            "nonce",
            "echostr",
            "bf56bf867459f80e3ceb854596f39f02a5ac5e13",
        ));
        assert!(!is_valid_wecom_signature(
            "token",
            "1710000000",
            "nonce",
            "echostr",
            "bad-signature",
        ));
    }

    #[test]
    fn test_decode_wecom_payload() {
        let plain = decode_wecom_payload(
            "ShlNaJ0PrdXQAuCDVqMki7c2JLNnY6mebvQodTv9qoV",
            "/gKbXNFpvlyYNTCneTag1rGm1P4Q5fExE3OPzdYlEyUVDgi55PHVIbo+mHMXWatdW8H8RTQJCly0HBNrWry2Uw==",
        )
        .expect("echostr should decrypt");

        assert_eq!(plain, "openfang-wecom-check");
    }

    #[test]
    fn test_parse_wecom_xml_fields() {
        let fields = parse_wecom_xml_fields(
            r#"<xml>
<ToUserName><![CDATA[wwcorp]]></ToUserName>
<FromUserName><![CDATA[user123]]></FromUserName>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[hello]]></Content>
<MsgId>123456</MsgId>
</xml>"#,
        )
        .expect("xml should parse");

        assert_eq!(
            fields.get("FromUserName").map(String::as_str),
            Some("user123")
        );
        assert_eq!(fields.get("MsgType").map(String::as_str), Some("text"));
        assert_eq!(fields.get("Content").map(String::as_str), Some("hello"));
        assert_eq!(fields.get("MsgId").map(String::as_str), Some("123456"));
    }
}
