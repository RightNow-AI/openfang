//! DingTalk Stream channel adapter.
//!
//! Uses DingTalk Stream Mode (WebSocket long-connection) instead of the
//! legacy webhook approach. The webhook adapter in `dingtalk.rs` is preserved
//! for backwards compatibility.
//!
//! Protocol:
//! 1. POST /v1.0/oauth2/accessToken        → get access token
//! 2. POST /v1.0/gateway/connections/open   → get WebSocket URL
//! 3. Connect via WebSocket, handle ping/pong and EVENT messages
//! 4. Outbound: POST /v1.0/robot/oToMessages/batchSend

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

const API_BASE: &str = "https://api.dingtalk.com";
const MAX_MESSAGE_LEN: usize = 20000;

// ─── Adapter ─────────────────────────────────────────────────────────────────

pub struct DingTalkStreamAdapter {
    app_key: String,
    app_secret: String,
    robot_code: String,
    client: reqwest::Client,
    token_cache: Arc<Mutex<TokenCache>>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl DingTalkStreamAdapter {
    pub fn new(app_key: String, app_secret: String, robot_code: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            app_key,
            app_secret,
            robot_code,
            client: reqwest::Client::new(),
            token_cache: Arc::new(Mutex::new(TokenCache::default())),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    async fn get_token(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        {
            let c = self.token_cache.lock().unwrap();
            if !c.token.is_empty() && c.expire_at > now + 300 {
                return Ok(c.token.clone());
            }
        }

        let resp: serde_json::Value = self
            .client
            .post(format!("{API_BASE}/v1.0/oauth2/accessToken"))
            .json(&serde_json::json!({
                "appKey": self.app_key,
                "appSecret": self.app_secret,
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let token = resp["accessToken"]
            .as_str()
            .ok_or("missing accessToken")?
            .to_string();
        let expire_in = resp["expireIn"].as_u64().unwrap_or(7200);

        {
            let mut c = self.token_cache.lock().unwrap();
            c.token = token.clone();
            c.expire_at = now + expire_in;
        }
        Ok(token)
    }

    async fn send_to_ids(
        &self,
        user_ids: &[&str],
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let token = self
            .get_token()
            .await
            .map_err(|e| -> Box<dyn std::error::Error> { e })?;

        let (msg_key, _msg_param) = match &content {
            ChannelContent::Text(t) => (
                "sampleText",
                serde_json::json!({ "content": t }).to_string(),
            ),
            _ => (
                "sampleText",
                serde_json::json!({ "content": "(unsupported content type)" }).to_string(),
            ),
        };

        let text = match &content {
            ChannelContent::Text(t) => t.as_str(),
            _ => "(unsupported)",
        };
        let chunks = split_message(text, MAX_MESSAGE_LEN);

        for chunk in &chunks {
            let param = serde_json::json!({ "content": chunk }).to_string();
            let body = serde_json::json!({
                "robotCode": self.robot_code,
                "userIds": user_ids,
                "msgKey": msg_key,
                "msgParam": param,
            });

            let resp = self
                .client
                .post(format!("{API_BASE}/v1.0/robot/oToMessages/batchSend"))
                .header("x-acs-dingtalk-access-token", &token)
                .json(&body)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status();
                let err_body = resp.text().await.unwrap_or_default();
                return Err(format!("DingTalk batchSend error {status}: {err_body}").into());
            }

            if chunks.len() > 1 {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ChannelAdapter for DingTalkStreamAdapter {
    fn name(&self) -> &str {
        "dingtalk_stream"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("dingtalk_stream".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let app_key = self.app_key.clone();
        let app_secret = self.app_secret.clone();
        let client = self.client.clone();
        let token_cache = Arc::clone(&self.token_cache);
        let mut shutdown_rx = self.shutdown_rx.clone();

        info!("DingTalk Stream adapter starting WebSocket connection");

        tokio::spawn(async move {
            let mut attempt: u32 = 0;

            loop {
                if *shutdown_rx.borrow() {
                    info!("DingTalk Stream: shutdown requested");
                    break;
                }

                // 1. Get access token
                let token =
                    match get_access_token(&client, &app_key, &app_secret, &token_cache).await {
                        Ok(t) => t,
                        Err(e) => {
                            warn!("DingTalk Stream: token fetch failed: {e}");
                            attempt += 1;
                            tokio::time::sleep(backoff(attempt)).await;
                            continue;
                        }
                    };

                // 2. Get WebSocket endpoint
                let ws_url = match get_ws_endpoint(&client, &app_key, &app_secret, &token).await {
                    Ok(u) => u,
                    Err(e) => {
                        warn!("DingTalk Stream: endpoint fetch failed: {e}");
                        attempt += 1;
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }
                };

                info!(
                    "DingTalk Stream: connecting to {}...",
                    &ws_url[..ws_url.len().min(60)]
                );

                // 3. Connect
                let ws_stream = match connect_async(&ws_url).await {
                    Ok((ws, _)) => ws,
                    Err(e) => {
                        warn!("DingTalk Stream: WS connect failed: {e}");
                        attempt += 1;
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }
                };

                info!("DingTalk Stream: connected");
                attempt = 0;
                let (mut sink, mut source) = ws_stream.split();

                // 4. Message loop
                loop {
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                info!("DingTalk Stream: graceful shutdown");
                                return;
                            }
                        }
                        msg = source.next() => {
                            match msg {
                                None => { warn!("DingTalk Stream: connection closed"); break; }
                                Some(Err(e)) => { warn!("DingTalk Stream: WS error: {e}"); break; }
                                Some(Ok(Message::Text(text))) => {
                                    handle_frame(&text, &mut sink, &tx).await;
                                }
                                Some(Ok(Message::Ping(d))) => { let _ = sink.send(Message::Pong(d)).await; }
                                Some(Ok(Message::Close(_))) => { info!("DingTalk Stream: close frame"); break; }
                                _ => {}
                            }
                        }
                    }
                }

                // Reconnect
                attempt += 1;
                let delay = backoff(attempt);
                info!("DingTalk Stream: reconnecting in {delay:?}");
                tokio::time::sleep(delay).await;
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let uid = &user.platform_id;
        if uid.is_empty() {
            return Err("DingTalk Stream: no platform_id to reply to".into());
        }
        self.send_to_ids(&[uid.as_str()], content).await
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

// ─── Token helpers ───────────────────────────────────────────────────────────

#[derive(Default)]
struct TokenCache {
    token: String,
    expire_at: u64,
}

async fn get_access_token(
    http: &reqwest::Client,
    app_key: &str,
    app_secret: &str,
    cache: &Arc<Mutex<TokenCache>>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    {
        let c = cache.lock().unwrap();
        if !c.token.is_empty() && c.expire_at > now + 300 {
            return Ok(c.token.clone());
        }
    }

    let resp: serde_json::Value = http
        .post(format!("{API_BASE}/v1.0/oauth2/accessToken"))
        .json(&serde_json::json!({ "appKey": app_key, "appSecret": app_secret }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let token = resp["accessToken"]
        .as_str()
        .ok_or("missing accessToken")?
        .to_string();
    let expire_in = resp["expireIn"].as_u64().unwrap_or(7200);
    {
        let mut c = cache.lock().unwrap();
        c.token = token.clone();
        c.expire_at = now + expire_in;
    }
    Ok(token)
}

// ─── Gateway / WebSocket helpers ─────────────────────────────────────────────

#[derive(Serialize)]
struct OpenConnectionRequest<'a> {
    #[serde(rename = "clientId")]
    client_id: &'a str,
    #[serde(rename = "clientSecret")]
    client_secret: &'a str,
    subscriptions: Vec<SubItem>,
    ua: &'a str,
    #[serde(rename = "localIp")]
    local_ip: &'a str,
}

#[derive(Serialize)]
struct SubItem {
    #[serde(rename = "type")]
    sub_type: String,
    topic: String,
}

#[derive(Deserialize)]
struct OpenConnectionResponse {
    endpoint: String,
    ticket: String,
}

async fn get_ws_endpoint(
    http: &reqwest::Client,
    app_key: &str,
    app_secret: &str,
    token: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let body = OpenConnectionRequest {
        client_id: app_key,
        client_secret: app_secret,
        subscriptions: vec![SubItem {
            sub_type: "CALLBACK".to_string(),
            topic: "/v1.0/im/bot/messages/get".to_string(),
        }],
        ua: "openfang/0.3",
        local_ip: "",
    };
    let resp: OpenConnectionResponse = http
        .post(format!("{API_BASE}/v1.0/gateway/connections/open"))
        .header("x-acs-dingtalk-access-token", token)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let sep = if resp.endpoint.contains('?') {
        "&"
    } else {
        "?"
    };
    Ok(format!("{}{}ticket={}", resp.endpoint, sep, resp.ticket))
}

// ─── Frame handling ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ProtoFrame {
    #[serde(rename = "type")]
    msg_type: String,
    headers: ProtoHeaders,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Deserialize)]
struct ProtoHeaders {
    #[serde(rename = "messageId", default)]
    message_id: String,
    #[serde(default)]
    topic: String,
}

#[derive(Serialize)]
struct AckReply {
    code: u32,
    headers: AckHeaders,
    message: String,
    data: String,
}

#[derive(Serialize)]
struct AckHeaders {
    #[serde(rename = "contentType")]
    content_type: String,
    #[serde(rename = "messageId")]
    message_id: String,
    topic: String,
}

fn make_ack(message_id: &str, topic: &str) -> String {
    serde_json::to_string(&AckReply {
        code: 200,
        headers: AckHeaders {
            content_type: "application/json".to_string(),
            message_id: message_id.to_string(),
            topic: topic.to_string(),
        },
        message: "OK".to_string(),
        data: String::new(),
    })
    .unwrap_or_default()
}

#[derive(Deserialize)]
struct CallbackPayload {
    #[serde(rename = "msgtype", default)]
    msg_type: String,
    #[serde(default)]
    text: Option<TextContent>,
    #[serde(rename = "senderStaffId", default)]
    sender_staff_id: String,
    #[serde(rename = "senderId", default)]
    sender_id: String,
    #[serde(rename = "senderNick", default)]
    sender_nick: String,
    #[serde(rename = "conversationId", default)]
    conversation_id: String,
    #[serde(rename = "conversationType", default)]
    conversation_type: String,
    #[serde(rename = "messageId", default)]
    message_id: String,
}

#[derive(Deserialize)]
struct TextContent {
    content: String,
}

async fn handle_frame<S>(text: &str, sink: &mut S, tx: &mpsc::Sender<ChannelMessage>)
where
    S: SinkExt<Message> + Unpin,
    <S as futures::Sink<Message>>::Error: std::fmt::Display,
{
    let frame: ProtoFrame = match serde_json::from_str(text) {
        Ok(f) => f,
        Err(e) => {
            warn!("DingTalk Stream: bad frame: {e}");
            return;
        }
    };

    let mid = &frame.headers.message_id;
    let topic = &frame.headers.topic;

    match frame.msg_type.as_str() {
        "SYSTEM" if topic == "ping" => {
            let _ = sink.send(Message::Text(make_ack(mid, "pong"))).await;
        }
        "CALLBACK" | "EVENT" => {
            let data_str = frame.data.to_string();
            // Try direct parse, then try unwrapping double-encoded string
            let cb: Option<CallbackPayload> = serde_json::from_str(&data_str).ok().or_else(|| {
                serde_json::from_str::<String>(&data_str)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            });

            if let Some(cb) = cb {
                if cb.msg_type == "text" {
                    if let Some(ref tc) = cb.text {
                        let trimmed = tc.content.trim().to_string();
                        if !trimmed.is_empty() {
                            let content = if trimmed.starts_with('/') {
                                let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
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
                                ChannelContent::Text(trimmed)
                            };

                            let mut meta = HashMap::new();
                            meta.insert(
                                "conversation_id".to_string(),
                                serde_json::Value::String(cb.conversation_id),
                            );

                            let uid = if cb.sender_staff_id.is_empty() {
                                cb.sender_id
                            } else {
                                cb.sender_staff_id
                            };

                            let msg = ChannelMessage {
                                channel: ChannelType::Custom("dingtalk_stream".to_string()),
                                platform_message_id: cb.message_id,
                                sender: ChannelUser {
                                    platform_id: uid,
                                    display_name: cb.sender_nick,
                                    openfang_user: None,
                                },
                                content,
                                target_agent: None,
                                timestamp: Utc::now(),
                                is_group: cb.conversation_type == "2",
                                thread_id: None,
                                metadata: meta,
                            };

                            if tx.send(msg).await.is_err() {
                                error!("DingTalk Stream: channel receiver dropped");
                            }
                        }
                    }
                }
            }

            let _ = sink.send(Message::Text(make_ack(mid, topic))).await;
        }
        _ => {
            let _ = sink.send(Message::Text(make_ack(mid, topic))).await;
        }
    }
}

fn backoff(attempt: u32) -> Duration {
    let ms = (1000u64 * 2u64.saturating_pow(attempt.min(6))).min(60_000);
    Duration::from_millis(ms)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_creation() {
        let a = DingTalkStreamAdapter::new("k".into(), "s".into(), "r".into());
        assert_eq!(a.name(), "dingtalk_stream");
        assert_eq!(
            a.channel_type(),
            ChannelType::Custom("dingtalk_stream".to_string())
        );
    }

    #[test]
    fn backoff_doubles() {
        assert_eq!(backoff(0), Duration::from_millis(1000));
        assert_eq!(backoff(1), Duration::from_millis(2000));
        assert_eq!(backoff(2), Duration::from_millis(4000));
    }

    #[test]
    fn backoff_capped() {
        assert_eq!(backoff(10), Duration::from_millis(60_000));
        assert_eq!(backoff(20), Duration::from_millis(60_000));
    }

    #[test]
    fn make_ack_valid_json() {
        let ack = make_ack("msg1", "topic1");
        let v: serde_json::Value = serde_json::from_str(&ack).unwrap();
        assert_eq!(v["code"], 200);
        assert_eq!(v["headers"]["messageId"], "msg1");
    }
}
