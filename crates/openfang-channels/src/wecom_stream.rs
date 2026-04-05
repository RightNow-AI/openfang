//! WeCom (WeChat Work) channel adapter - WebSocket long connection mode.
//!
//! Uses WeCom WebSocket long-connection instead of the legacy webhook approach.
//! The webhook adapter in `wecom.rs` is preserved for backwards compatibility.
//!
//! Protocol:
//! 1. Connect to wss://openws.work.weixin.qq.com
//! 2. Send aibot_subscribe with bot_id + secret
//! 3. Handle ping (30s interval), aibot_msg_callback, aibot_event_callback
//! 4. Outbound: aibot_respond_msg (reply) or aibot_send_msg (active push)

use crate::types::{
    split_message, ChannelAdapter, ChannelContent, ChannelMessage, ChannelStatus, ChannelType,
    ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::{SinkExt, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, watch, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

/// WeCom WebSocket endpoint.
const WECOM_WS_URL: &str = "wss://openws.work.weixin.qq.com";

/// Maximum WeCom message text length (characters).
const MAX_MESSAGE_LEN: usize = 2048;

/// Heartbeat interval (30 seconds as recommended).
const PING_INTERVAL_SECS: u64 = 30;

/// Maximum cached message IDs for deduplication.
const DEDUP_CACHE_SIZE: usize = 1000;

// ─── Deduplication Cache ─────────────────────────────────────────────────────

/// Simple ring-buffer deduplication cache.
struct DedupCache {
    ids: Mutex<Vec<String>>,
    max_size: usize,
}

impl DedupCache {
    fn new(max_size: usize) -> Self {
        Self {
            ids: Mutex::new(Vec::with_capacity(max_size)),
            max_size,
        }
    }

    /// Returns `true` if the ID was already seen (duplicate).
    async fn check_and_insert(&self, id: &str) -> bool {
        let mut ids = self.ids.lock().await;
        if ids.iter().any(|s| s == id) {
            return true;
        }
        if ids.len() >= self.max_size {
            let drain_count = self.max_size / 2;
            ids.drain(..drain_count);
        }
        ids.push(id.to_string());
        false
    }
}

// ─── Protocol Types ───────────────────────────────────────────────────────────

/// Common frame structure for incoming/outgoing messages.
#[derive(Debug, Serialize, Deserialize)]
struct WsFrame {
    #[serde(default)]
    cmd: String,
    headers: WsHeaders,
    #[serde(default)]
    body: serde_json::Value,
    #[serde(default)]
    errcode: Option<i32>,
    #[serde(default)]
    errmsg: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WsHeaders {
    req_id: String,
}

/// Subscribe request body.
#[derive(Debug, Serialize)]
struct SubscribeBody<'a> {
    bot_id: &'a str,
    secret: &'a str,
}

/// Message callback body (aibot_msg_callback).
#[derive(Debug, Deserialize)]
struct MsgCallbackBody {
    #[serde(default)]
    msgid: String,
    #[serde(default)]
    chatid: String,
    #[serde(default)]
    chattype: String,
    #[serde(default)]
    from: Option<FromUser>,
    #[serde(default)]
    msgtype: String,
    #[serde(default)]
    text: Option<TextContent>,
}

#[derive(Debug, Deserialize)]
struct FromUser {
    #[serde(default)]
    userid: String,
}

#[derive(Debug, Deserialize)]
struct TextContent {
    #[serde(default)]
    content: String,
}

/// Event callback body (aibot_event_callback).
#[derive(Debug, Deserialize)]
struct EventCallbackBody {
    #[serde(default)]
    eventtype: String,
    #[serde(default)]
    chatid: String,
    #[serde(default)]
    from: Option<FromUser>,
}

/// Respond message request (aibot_respond_msg) - stream format.
#[derive(Debug, Serialize)]
struct RespondMsgBody<'a> {
    msgtype: &'a str,
    stream: StreamContentSer<'a>,
}

#[derive(Debug, Serialize)]
struct StreamContentSer<'a> {
    id: &'a str,
    finish: bool,
    content: &'a str,
}

/// Active send message request (aibot_send_msg) - stream format.
#[derive(Debug, Serialize)]
struct SendMsgBody<'a> {
    chatid: &'a str,
    #[serde(rename = "chat_type")]
    chat_type: u8, // 1=single, 2=group
    msgtype: &'a str,
    markdown: StreamContentSer<'a>,
}

/// Outbound message to send via WebSocket.
#[derive(Debug)]
enum OutboundMsg {
    Respond {
        req_id: String,
        stream_id: String,
        text: String,
    },
    Send {
        chatid: String,
        chat_type: u8,
        text: String,
    },
}

// ─── Adapter ─────────────────────────────────────────────────────────────────

pub struct WeComStreamAdapter {
    bot_id: String,
    secret: Zeroizing<String>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    connected_tx: Arc<watch::Sender<bool>>,
    connected_rx: watch::Receiver<bool>,
    /// Map userid -> PendingReply for replying.
    pending_replies: Arc<Mutex<HashMap<String, PendingReply>>>,
    /// Map platform_id (userid) -> (chatid, chat_type) for active send.
    chat_info: Arc<Mutex<HashMap<String, (String, u8)>>>,
    /// Message deduplication cache.
    msg_dedup: Arc<DedupCache>,
    /// Channel for sending outbound messages to WebSocket task.
    outbound_tx: mpsc::Sender<OutboundMsg>,
    outbound_rx: Arc<Mutex<Option<mpsc::Receiver<OutboundMsg>>>>,
}

#[derive(Clone)]
struct PendingReply {
    req_id: String,
    stream_id: String,
}

impl WeComStreamAdapter {
    pub fn new(bot_id: String, secret: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (connected_tx, connected_rx) = watch::channel(false);
        let (outbound_tx, outbound_rx) = mpsc::channel(32);
        Self {
            bot_id,
            secret: Zeroizing::new(secret),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            connected_tx: Arc::new(connected_tx),
            connected_rx,
            pending_replies: Arc::new(Mutex::new(HashMap::new())),
            chat_info: Arc::new(Mutex::new(HashMap::new())),
            msg_dedup: Arc::new(DedupCache::new(DEDUP_CACHE_SIZE)),
            outbound_tx,
            outbound_rx: Arc::new(Mutex::new(Some(outbound_rx))),
        }
    }

    fn generate_req_id() -> String {
        Uuid::new_v4().to_string()
    }
}

// ─── ChannelAdapter impl ─────────────────────────────────────────────────────

#[async_trait]
impl ChannelAdapter for WeComStreamAdapter {
    fn name(&self) -> &str {
        "wecom_stream"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("wecom_stream".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let bot_id = self.bot_id.clone();
        let secret = self.secret.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();
        let connected_tx = Arc::clone(&self.connected_tx);
        let pending_replies = Arc::clone(&self.pending_replies);
        let chat_info = Arc::clone(&self.chat_info);
        let msg_dedup = Arc::clone(&self.msg_dedup);
        let outbound_rx = {
            let mut guard = self.outbound_rx.lock().await;
            guard.take().ok_or("outbound_rx already taken")?
        };

        info!("WeCom Stream adapter starting WebSocket connection");

        tokio::spawn(async move {
            let mut attempt: u32 = 0;
            let mut outbound_rx = outbound_rx;

            loop {
                if *shutdown_rx.borrow() {
                    let _ = connected_tx.send(false);
                    break;
                }

                let _ = connected_tx.send(false);

                // 1. Connect WebSocket
                info!("WeCom Stream: connecting to {}", WECOM_WS_URL);
                let ws_stream = match connect_async(WECOM_WS_URL).await {
                    Ok((ws, _)) => ws,
                    Err(e) => {
                        warn!("WeCom Stream: WS connect failed: {e}");
                        attempt += 1;
                        tokio::time::sleep(backoff(attempt)).await;
                        continue;
                    }
                };

                info!("WeCom Stream: connected, sending subscribe");
                let (mut sink, mut source) = ws_stream.split();

                // 2. Send subscribe
                let req_id = WeComStreamAdapter::generate_req_id();
                let subscribe_frame = WsFrame {
                    cmd: "aibot_subscribe".to_string(),
                    headers: WsHeaders { req_id },
                    body: serde_json::to_value(SubscribeBody {
                        bot_id: &bot_id,
                        secret: &secret,
                    })
                    .unwrap_or_default(),
                    errcode: None,
                    errmsg: None,
                };

                if let Err(e) = sink
                    .send(Message::Text(
                        serde_json::to_string(&subscribe_frame).unwrap_or_default(),
                    ))
                    .await
                {
                    warn!("WeCom Stream: subscribe send failed: {e}");
                    attempt += 1;
                    tokio::time::sleep(backoff(attempt)).await;
                    continue;
                }

                // Wait for subscribe response and run message loop
                let mut subscribed = false;
                let mut ping_interval =
                    tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));
                ping_interval.tick().await;

                loop {
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() {
                                info!("WeCom Stream: graceful shutdown");
                                let _ = connected_tx.send(false);
                                return;
                            }
                        }
                        _ = ping_interval.tick() => {
                            if subscribed {
                                let ping_req_id = WeComStreamAdapter::generate_req_id();
                                let ping_frame = WsFrame {
                                    cmd: "ping".to_string(),
                                    headers: WsHeaders { req_id: ping_req_id },
                                    body: serde_json::Value::Null,
                                    errcode: None,
                                    errmsg: None,
                                };
                                if let Err(e) = sink.send(Message::Text(
                                    serde_json::to_string(&ping_frame).unwrap_or_default()
                                )).await {
                                    warn!("WeCom Stream: ping send failed: {e}");
                                    break;
                                }
                            }
                        }
                        msg = source.next() => {
                            match msg {
                                None => {
                                    warn!("WeCom Stream: connection closed");
                                    break;
                                }
                                Some(Err(e)) => {
                                    warn!("WeCom Stream: WS error: {e}");
                                    break;
                                }
                                Some(Ok(Message::Text(text))) => {
                                    if let Err(e) = handle_frame(
                                        &text,
                                        &tx,
                                        &pending_replies,
                                        &chat_info,
                                        &msg_dedup,
                                        &mut subscribed,
                                        &connected_tx,
                                    ).await {
                                        warn!("WeCom Stream: handle frame error: {e}");
                                    }
                                }
                                Some(Ok(Message::Ping(d))) => {
                                    let _ = sink.send(Message::Pong(d)).await;
                                }
                                Some(Ok(Message::Close(_))) => {
                                    info!("WeCom Stream: close frame");
                                    break;
                                }
                                _ => {}
                            }
                        }
                        outbound = outbound_rx.recv() => {
                            match outbound {
                                Some(msg) => {
                                    if let Err(e) = send_outbound(&mut sink, msg).await {
                                        warn!("WeCom Stream: send outbound failed: {e}");
                                    }
                                }
                                None => {
                                    info!("WeCom Stream: outbound channel closed, shutting down");
                                    let _ = connected_tx.send(false);
                                    return;
                                }
                            }
                        }
                    }
                }

                // Connection closed, update status
                let _ = connected_tx.send(false);

                // Reconnect
                attempt += 1;
                let delay = backoff(attempt);
                info!("WeCom Stream: reconnecting in {delay:?}");
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
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
        let text = match &content {
            ChannelContent::Text(t) => t.as_str(),
            ChannelContent::Command { name, args } => &format!("/{} {}", name, args.join(" ")),
            _ => "(unsupported content type)",
        };

        let platform_id_lower = user.platform_id.to_lowercase();
        info!(
            "WeCom Stream: send() called for user platform_id: {} (lower: {}), display_name: {}",
            user.platform_id, platform_id_lower, user.display_name
        );

        // First try to find a pending reply for this user (using lowercase for case-insensitive matching)
        {
            let pending = self.pending_replies.lock().await;
            info!(
                "WeCom Stream: pending_replies has {} entries: {:?}",
                pending.len(),
                pending.keys().collect::<Vec<_>>()
            );
            if let Some(reply) = pending.get(&platform_id_lower) {
                info!(
                    "WeCom Stream: found pending reply for user {} (lower: {}), req_id: {}, stream_id: {}",
                    user.platform_id, platform_id_lower, reply.req_id, reply.stream_id
                );
                let chunks = split_message(text, MAX_MESSAGE_LEN);
                for chunk in &chunks {
                    self.outbound_tx
                        .send(OutboundMsg::Respond {
                            req_id: reply.req_id.clone(),
                            stream_id: reply.stream_id.clone(),
                            text: chunk.to_string(),
                        })
                        .await?;
                }
                return Ok(());
            }
        }

        // Fallback: try to use stored chat info for active send (using lowercase for case-insensitive matching)
        let chat_info = {
            let guard = self.chat_info.lock().await;
            info!(
                "WeCom Stream: chat_info has {} entries: {:?}",
                guard.len(),
                guard.keys().collect::<Vec<_>>()
            );
            guard.get(&platform_id_lower).cloned()
        };

        if let Some((chatid, chat_type)) = chat_info {
            info!(
                "WeCom Stream: found chat info for user {} (lower: {}), chatid: {}, chat_type: {}",
                user.platform_id, platform_id_lower, chatid, chat_type
            );
            let chunks = split_message(text, MAX_MESSAGE_LEN);
            for chunk in &chunks {
                self.outbound_tx
                    .send(OutboundMsg::Send {
                        chatid: chatid.clone(),
                        chat_type,
                        text: chunk.to_string(),
                    })
                    .await?;
            }
            return Ok(());
        }

        Err(format!(
            "WeCom Stream: no pending reply or chat info available for user (platform_id: {}, lower: {})",
            user.platform_id, platform_id_lower
        )
        .into())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }

    fn status(&self) -> ChannelStatus {
        ChannelStatus {
            connected: *self.connected_rx.borrow(),
            ..Default::default()
        }
    }
}

// ─── Frame handling ──────────────────────────────────────────────────────────

async fn send_outbound<S>(sink: &mut S, msg: OutboundMsg) -> Result<(), Box<dyn std::error::Error>>
where
    S: SinkExt<Message> + Unpin,
    <S as futures::Sink<Message>>::Error: std::fmt::Display + std::error::Error + 'static,
{
    let frame = match msg {
        OutboundMsg::Respond {
            req_id,
            stream_id,
            text,
        } => WsFrame {
            cmd: "aibot_respond_msg".to_string(),
            headers: WsHeaders { req_id },
            body: serde_json::to_value(RespondMsgBody {
                msgtype: "stream",
                stream: StreamContentSer {
                    id: &stream_id,
                    finish: true,
                    content: &text,
                },
            })
            .unwrap_or_default(),
            errcode: None,
            errmsg: None,
        },
        OutboundMsg::Send {
            chatid,
            chat_type,
            text,
        } => WsFrame {
            cmd: "aibot_send_msg".to_string(),
            headers: WsHeaders {
                req_id: WeComStreamAdapter::generate_req_id(),
            },
            body: serde_json::to_value(SendMsgBody {
                chatid: &chatid,
                chat_type,
                msgtype: "markdown",
                markdown: StreamContentSer {
                    content: &text,
                    finish: true,
                    id: "",
                },
            })
            .unwrap_or_default(),
            errcode: None,
            errmsg: None,
        },
    };

    let json_str = serde_json::to_string(&frame)?;
    info!("WeCom Stream: sending message: {}", json_str);
    sink.send(Message::Text(json_str)).await?;
    Ok(())
}

async fn handle_frame(
    text: &str,
    tx: &mpsc::Sender<ChannelMessage>,
    pending_replies: &Arc<Mutex<HashMap<String, PendingReply>>>,
    chat_info: &Arc<Mutex<HashMap<String, (String, u8)>>>,
    msg_dedup: &Arc<DedupCache>,
    subscribed: &mut bool,
    connected_tx: &Arc<watch::Sender<bool>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let frame: WsFrame = match serde_json::from_str(text) {
        Ok(f) => f,
        Err(e) => {
            warn!("WeCom Stream: bad frame: {e}");
            return Ok(());
        }
    };

    match frame.cmd.as_str() {
        "" | "aibot_subscribe" | "aibot_respond_msg" | "aibot_send_msg" | "ping" | "pong" => {
            // 订阅响应或消息发送响应可能没有 cmd 字段，或者 cmd 为相应的命令
            // errcode 和 errmsg 在帧的顶层
            let errcode = frame.errcode.unwrap_or(-1);
            let errmsg = frame.errmsg.as_deref().unwrap_or("unknown error");
            let cmd = if frame.cmd.is_empty() {
                "response"
            } else {
                &frame.cmd
            };
            if errcode == 0 {
                if cmd == "aibot_subscribe" || frame.cmd.is_empty() {
                    *subscribed = true;
                    let _ = connected_tx.send(true);
                } else {
                    info!(
                        "WeCom Stream: {} successful, req_id: {}",
                        cmd, frame.headers.req_id
                    );
                }
            } else {
                warn!(
                    "WeCom Stream: {} failed: {} - {}, req_id: {}",
                    cmd, errcode, errmsg, frame.headers.req_id
                );
            }
        }
        "aibot_msg_callback" => {
            let req_id = frame.headers.req_id.clone();
            let body: MsgCallbackBody = match serde_json::from_value(frame.body) {
                Ok(b) => b,
                Err(e) => {
                    warn!("WeCom Stream: failed to parse msg_callback: {e}");
                    return Ok(());
                }
            };

            // Deduplicate by msgid
            if !body.msgid.is_empty() && msg_dedup.check_and_insert(&body.msgid).await {
                return Ok(());
            }

            let chattype = body.chattype.clone();
            let chatid = body.chatid.clone();
            let userid = body.from.as_ref().map(|f| f.userid.as_str()).unwrap_or("");

            info!(
                "WeCom Stream: received aibot_msg_callback, req_id: {}, chatid: {}, userid: {}, chattype: {}",
                req_id, chatid, userid, chattype
            );

            // Store pending reply info (use lowercase userid for case-insensitive matching)
            if !req_id.is_empty() && !userid.is_empty() {
                let userid_lower = userid.to_lowercase();
                let stream_id = Uuid::new_v4().to_string().replace("-", "");
                let mut pending = pending_replies.lock().await;
                // Keep only recent pending replies to avoid memory leak
                if pending.len() > 100 {
                    pending.clear();
                }
                pending.insert(
                    userid_lower.clone(),
                    PendingReply {
                        req_id: req_id.clone(),
                        stream_id: stream_id.clone(),
                    },
                );
                info!(
                    "WeCom Stream: stored pending reply for userid: {} (lower: {}), req_id: {}, stream_id: {}",
                    userid, userid_lower, req_id, stream_id
                );
            }

            // Store chat info for active send (use lowercase userid for case-insensitive matching)
            if !userid.is_empty() {
                let userid_lower = userid.to_lowercase();
                let chat_type_num = if chattype == "group" { 2 } else { 1 };
                // 单聊时使用 userid 作为 chatid，群聊时使用返回的 chatid
                let actual_chatid = if chattype == "group" {
                    chatid.clone()
                } else {
                    userid.to_string()
                };
                let mut info = chat_info.lock().await;
                info.insert(userid_lower.clone(), (actual_chatid.clone(), chat_type_num));
                info!(
                    "WeCom Stream: stored chat info for userid: {} (lower: {}), chatid: {}, chat_type: {}",
                    userid, userid_lower, actual_chatid, chat_type_num
                );
            }

            // Handle text message
            if body.msgtype == "text" {
                if let Some(text_content) = body.text {
                    let trimmed = text_content.content.trim().to_string();
                    if !trimmed.is_empty() && !userid.is_empty() {
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
                        // 单聊时使用 userid 作为 chatid，群聊时使用返回的 chatid
                        let meta_chatid = if chattype == "group" {
                            chatid.clone()
                        } else {
                            userid.to_string()
                        };
                        meta.insert("chatid".to_string(), serde_json::Value::String(meta_chatid));
                        meta.insert("req_id".to_string(), serde_json::Value::String(req_id));

                        let msg = ChannelMessage {
                            channel: ChannelType::Custom("wecom_stream".to_string()),
                            platform_message_id: body.msgid,
                            sender: ChannelUser {
                                platform_id: userid.to_string(),
                                display_name: userid.to_string(),
                                openfang_user: None,
                            },
                            content,
                            target_agent: None,
                            timestamp: Utc::now(),
                            is_group: chattype == "group",
                            thread_id: None,
                            metadata: meta,
                        };

                        if tx.send(msg).await.is_err() {
                            error!("WeCom Stream: channel receiver dropped");
                        }
                    }
                }
            }
        }
        "aibot_event_callback" => {
            let body: EventCallbackBody = match serde_json::from_value(frame.body) {
                Ok(b) => b,
                Err(e) => {
                    warn!("WeCom Stream: failed to parse event_callback: {e}");
                    return Ok(());
                }
            };

            let userid = body.from.as_ref().map(|f| f.userid.as_str()).unwrap_or("");
            let _chatid = body.chatid.clone();

            // Handle enter_agent event (user starts conversation)
            if body.eventtype == "enter_agent" && !userid.is_empty() {
                // Store chat info - enter_agent is single chat, use userid as chatid
                let chat_type_num = 1; // single chat
                let actual_chatid = userid.to_string();
                let userid_lower = userid.to_lowercase();
                {
                    let mut info = chat_info.lock().await;
                    info.insert(userid_lower.clone(), (actual_chatid.clone(), chat_type_num));
                }
                info!(
                    "WeCom Stream: enter_agent, stored chat info for userid: {} (lower: {}), chatid: {}",
                    userid, userid_lower, actual_chatid
                );

                let msg = ChannelMessage {
                    channel: ChannelType::Custom("wecom_stream".to_string()),
                    platform_message_id: String::new(),
                    sender: ChannelUser {
                        platform_id: userid.to_string(),
                        display_name: userid.to_string(),
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
        }
        _ => {
            debug!("WeCom Stream: unknown cmd: {}", frame.cmd);
        }
    }

    Ok(())
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
        let a = WeComStreamAdapter::new("botid".into(), "secret".into());
        assert_eq!(a.name(), "wecom_stream");
        assert_eq!(
            a.channel_type(),
            ChannelType::Custom("wecom_stream".to_string())
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
}
