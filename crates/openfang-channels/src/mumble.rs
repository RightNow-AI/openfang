//! Mumble text-chat channel adapter.
//!
//! Connects to a Mumble server via TCP and exchanges text messages using a
//! simplified protobuf-style framing protocol. Voice channels are ignored;
//! only `TextMessage` packets (type 11) are processed.

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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch, Mutex};
use tracing::{info, warn};
use zeroize::Zeroizing;

const MAX_MESSAGE_LEN: usize = 5000;
const DEFAULT_PORT: u16 = 64738;

// Mumble packet types (protobuf message IDs)
const MSG_TYPE_VERSION: u16 = 0;
const MSG_TYPE_AUTHENTICATE: u16 = 2;
const MSG_TYPE_PING: u16 = 3;
const MSG_TYPE_TEXT_MESSAGE: u16 = 11;

/// Mumble text-chat channel adapter.
///
/// Connects to a Mumble server using TCP and handles text messages only
/// (no voice). The protocol uses a 6-byte header: 2-byte big-endian message
/// type followed by 4-byte big-endian payload length.
pub struct MumbleAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// Mumble server hostname or IP.
    host: String,
    /// TCP port (default: 64738).
    port: u16,
    /// SECURITY: Server password is zeroized on drop.
    password: Zeroizing<String>,
    /// Username to authenticate with.
    username: String,
    /// Mumble channel to join (by name).
    channel_name: String,
    /// Shared TCP stream for sending (wrapped in Mutex for exclusive write access).
    stream: Arc<Mutex<Option<tokio::net::tcp::OwnedWriteHalf>>>,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl MumbleAdapter {
    /// Create a new Mumble text-chat adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `host` - Hostname or IP of the Mumble server.
    /// * `port` - TCP port (0 = use default 64738).
    /// * `password` - Server password (empty string if none).
    /// * `username` - Username for authentication.
    /// * `channel_name` - Mumble channel to join.
    pub fn new(
        id: String,
        host: String,
        port: u16,
        password: String,
        username: String,
        channel_name: String,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let port = if port == 0 { DEFAULT_PORT } else { port };
        Self {
            id,
            host,
            port,
            password: Zeroizing::new(password),
            username,
            channel_name,
            stream: Arc::new(Mutex::new(None)),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Encode a Mumble packet: 2-byte type (BE) + 4-byte length (BE) + payload.
    fn encode_packet(msg_type: u16, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(6 + payload.len());
        buf.extend_from_slice(&msg_type.to_be_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    /// Build a minimal Version packet (type 0).
    ///
    /// Simplified encoding: version fields as varint-like protobuf.
    /// Field 1 (version): 0x00010500 (1.5.0)
    /// Field 2 (release): "OpenFang"
    fn build_version_packet() -> Vec<u8> {
        let mut payload = Vec::new();
        // Field 1: fixed32 version = 0x00010500 (tag = 0x0D for wire type 5)
        payload.push(0x0D);
        payload.extend_from_slice(&0x0001_0500u32.to_le_bytes());
        // Field 2: string release (tag = 0x12)
        let release = b"OpenFang";
        payload.push(0x12);
        payload.push(release.len() as u8);
        payload.extend_from_slice(release);
        // Field 3: string os (tag = 0x1A)
        let os = std::env::consts::OS.as_bytes();
        payload.push(0x1A);
        payload.push(os.len() as u8);
        payload.extend_from_slice(os);
        payload
    }

    /// Build an Authenticate packet (type 2).
    ///
    /// Field 1 (username): string
    /// Field 2 (password): string
    fn build_authenticate_packet(username: &str, password: &str) -> Vec<u8> {
        let mut payload = Vec::new();
        // Field 1: string username (tag = 0x0A)
        let uname = username.as_bytes();
        payload.push(0x0A);
        Self::encode_varint(uname.len() as u64, &mut payload);
        payload.extend_from_slice(uname);
        // Field 2: string password (tag = 0x12)
        if !password.is_empty() {
            let pass = password.as_bytes();
            payload.push(0x12);
            Self::encode_varint(pass.len() as u64, &mut payload);
            payload.extend_from_slice(pass);
        }
        payload
    }

    /// Build a TextMessage packet (type 11).
    ///
    /// Field 1 (actor): uint32 (omitted — server assigns)
    /// Field 3 (channel_id): repeated uint32
    /// Field 5 (message): string
    fn build_text_message_packet(channel_id: u32, message: &str) -> Vec<u8> {
        let mut payload = Vec::new();
        // Field 3: uint32 channel_id (tag = 0x18, wire type 0 = varint)
        payload.push(0x18);
        Self::encode_varint(channel_id as u64, &mut payload);
        // Field 5: string message (tag = 0x2A, wire type 2 = length-delimited)
        let msg = message.as_bytes();
        payload.push(0x2A);
        Self::encode_varint(msg.len() as u64, &mut payload);
        payload.extend_from_slice(msg);
        payload
    }

    /// Build a Ping packet (type 3). Minimal — just a timestamp field.
    fn build_ping_packet() -> Vec<u8> {
        let mut payload = Vec::new();
        // Field 1: uint64 timestamp (tag = 0x08)
        let ts = Utc::now().timestamp() as u64;
        payload.push(0x08);
        Self::encode_varint(ts, &mut payload);
        payload
    }

    /// Encode a varint (protobuf base-128 encoding).
    fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
        loop {
            let byte = (value & 0x7F) as u8;
            value >>= 7;
            if value == 0 {
                buf.push(byte);
                break;
            } else {
                buf.push(byte | 0x80);
            }
        }
    }

    /// Decode a varint from bytes. Returns (value, bytes_consumed).
    fn decode_varint(data: &[u8]) -> (u64, usize) {
        let mut value: u64 = 0;
        let mut shift = 0;
        for (i, &byte) in data.iter().enumerate() {
            value |= ((byte & 0x7F) as u64) << shift;
            if byte & 0x80 == 0 {
                return (value, i + 1);
            }
            shift += 7;
            if shift >= 64 {
                break;
            }
        }
        (value, data.len())
    }

    /// Parse a TextMessage protobuf payload.
    /// Returns (actor, channel_ids, tree_ids, session_ids, message).
    fn parse_text_message(payload: &[u8]) -> (u32, Vec<u32>, Vec<u32>, Vec<u32>, String) {
        let mut actor: u32 = 0;
        let mut channel_ids = Vec::new();
        let mut tree_ids = Vec::new();
        let mut session_ids = Vec::new();
        let mut message = String::new();

        let mut pos = 0;
        while pos < payload.len() {
            let tag_byte = payload[pos];
            let field_number = tag_byte >> 3;
            let wire_type = tag_byte & 0x07;
            pos += 1;

            match (field_number, wire_type) {
                // Field 1: actor (uint32, varint)
                (1, 0) => {
                    let (val, consumed) = Self::decode_varint(&payload[pos..]);
                    actor = val as u32;
                    pos += consumed;
                }
                // Field 2: session (repeated uint32, varint)
                (2, 0) => {
                    let (val, consumed) = Self::decode_varint(&payload[pos..]);
                    session_ids.push(val as u32);
                    pos += consumed;
                }
                // Field 3: channel_id (repeated uint32, varint)
                (3, 0) => {
                    let (val, consumed) = Self::decode_varint(&payload[pos..]);
                    channel_ids.push(val as u32);
                    pos += consumed;
                }
                // Field 4: tree_id (repeated uint32, varint)
                (4, 0) => {
                    let (val, consumed) = Self::decode_varint(&payload[pos..]);
                    tree_ids.push(val as u32);
                    pos += consumed;
                }
                // Field 5: message (string, length-delimited)
                (5, 2) => {
                    let (len, consumed) = Self::decode_varint(&payload[pos..]);
                    pos += consumed;
                    let end = pos + len as usize;
                    if end <= payload.len() {
                        message = String::from_utf8_lossy(&payload[pos..end]).to_string();
                    }
                    pos = end;
                }
                // Unknown — skip
                (_, 0) => {
                    let (_, consumed) = Self::decode_varint(&payload[pos..]);
                    pos += consumed;
                }
                (_, 2) => {
                    let (len, consumed) = Self::decode_varint(&payload[pos..]);
                    pos += consumed + len as usize;
                }
                (_, 5) => {
                    pos += 4; // fixed32
                }
                (_, 1) => {
                    pos += 8; // fixed64
                }
                _ => {
                    break; // Unrecoverable wire type
                }
            }
        }

        (actor, channel_ids, tree_ids, session_ids, message)
    }
}

#[async_trait]
impl ChannelAdapter for MumbleAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "mumble"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("mumble".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        let addr = format!("{}:{}", self.host, self.port);
        info!("Mumble adapter connecting to {addr}");

        let tcp = TcpStream::connect(&addr).await?;
        let (mut reader, writer) = tcp.into_split();

        // Store writer for send()
        {
            let mut lock = self.stream.lock().await;
            *lock = Some(writer);
        }

        // Send Version + Authenticate
        {
            let mut lock = self.stream.lock().await;
            if let Some(ref mut w) = *lock {
                let version_pkt =
                    Self::encode_packet(MSG_TYPE_VERSION, &Self::build_version_packet());
                w.write_all(&version_pkt).await?;

                let auth_pkt = Self::encode_packet(
                    MSG_TYPE_AUTHENTICATE,
                    &Self::build_authenticate_packet(&self.username, &self.password),
                );
                w.write_all(&auth_pkt).await?;
                w.flush().await?;
            }
        }

        info!("Mumble adapter authenticated as {}", self.username);

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let channel_name = self.channel_name.clone();
        let own_username = self.username.clone();
        let stream_handle = Arc::clone(&self.stream);
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut header_buf = [0u8; 6];
            let mut backoff = Duration::from_secs(1);
            let mut ping_interval = tokio::time::interval(Duration::from_secs(20));
            ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("Mumble adapter shutting down");
                            break;
                        }
                    }
                    _ = ping_interval.tick() => {
                        // Send keepalive ping
                        let mut lock = stream_handle.lock().await;
                        if let Some(ref mut w) = *lock {
                            let pkt = Self::encode_packet(MSG_TYPE_PING, &Self::build_ping_packet());
                            if let Err(e) = w.write_all(&pkt).await {
                                warn!("Mumble: ping write error: {e}");
                            }
                        }
                    }
                    result = reader.read_exact(&mut header_buf) => {
                        match result {
                            Ok(_) => {
                                backoff = Duration::from_secs(1);
                                let msg_type = u16::from_be_bytes([header_buf[0], header_buf[1]]);
                                let msg_len = u32::from_be_bytes([
                                    header_buf[2], header_buf[3],
                                    header_buf[4], header_buf[5],
                                ]) as usize;

                                // Sanity check — reject packets larger than 1 MB
                                if msg_len > 1_048_576 {
                                    warn!("Mumble: oversized packet ({msg_len} bytes), skipping");
                                    continue;
                                }

                                let mut payload = vec![0u8; msg_len];
                                if let Err(e) = reader.read_exact(&mut payload).await {
                                    warn!("Mumble: payload read error: {e}");
                                    break;
                                }

                                if msg_type == MSG_TYPE_TEXT_MESSAGE {
                                    let (actor, _ch_ids, _tree_ids, _session_ids, message) =
                                        Self::parse_text_message(&payload);

                                    if message.is_empty() {
                                        continue;
                                    }

                                    // Strip basic HTML tags that Mumble wraps text in
                                    let clean_msg = message
                                        .replace("<br>", "\n")
                                        .replace("<br/>", "\n")
                                        .replace("<br />", "\n");
                                    // Rough tag strip
                                    let clean_msg = {
                                        let mut out = String::with_capacity(clean_msg.len());
                                        let mut in_tag = false;
                                        for ch in clean_msg.chars() {
                                            if ch == '<' { in_tag = true; continue; }
                                            if ch == '>' { in_tag = false; continue; }
                                            if !in_tag { out.push(ch); }
                                        }
                                        out
                                    };

                                    if clean_msg.is_empty() {
                                        continue;
                                    }

                                    let content = if clean_msg.starts_with('/') {
                                        let parts: Vec<&str> = clean_msg.splitn(2, ' ').collect();
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
                                        ChannelContent::Text(clean_msg)
                                    };

                                    let channel_msg = ChannelMessage {
                                        channel: ChannelType::Custom("mumble".to_string()),
                                        platform_message_id: format!(
                                            "mumble-{}-{}",
                                            actor,
                                            Utc::now().timestamp_millis()
                                        ),
                                        sender: ChannelUser {
                                            platform_id: format!("session-{actor}"),
                                            display_name: format!("user-{actor}"),
                                            openfang_user: None,
                                        },
                                        content,
                                        target_agent: None,
                                        timestamp: Utc::now(),
                                        is_group: true,
                                        thread_id: None,
                                        metadata: {
                                            let mut m = HashMap::new();
                                            m.insert(
                                                "channel".to_string(),
                                                serde_json::Value::String(channel_name.clone()),
                                            );
                                            m.insert(
                                                "actor".to_string(),
                                                serde_json::Value::Number(actor.into()),
                                            );
                                            m
                                        },
                                    };

                                    if tx.send(channel_msg).await.is_err() {
                                        return;
                                    }
                                }
                                // Other packet types (ServerSync, ChannelState, etc.) silently ignored
                            }
                            Err(e) => {
                                warn!("Mumble: read error: {e}, backing off {backoff:?}");
                                tokio::time::sleep(backoff).await;
                                backoff = (backoff * 2).min(Duration::from_secs(60));
                            }
                        }
                    }
                }

                if *shutdown_rx.borrow() {
                    break;
                }
            }

            info!("Mumble polling loop stopped");
            let _ = own_username;
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        _user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match content {
            ChannelContent::Text(t) => t,
            _ => "(Unsupported content type)".to_string(),
        };

        let chunks = split_message(&text, MAX_MESSAGE_LEN);

        let mut lock = self.stream.lock().await;
        let writer = lock
            .as_mut()
            .ok_or("Mumble: not connected — call start() first")?;

        for chunk in chunks {
            // Send to channel 0 (root). In production the channel_id would be
            // resolved from self.channel_name via a ChannelState mapping.
            let payload = Self::build_text_message_packet(0, chunk);
            let pkt = Self::encode_packet(MSG_TYPE_TEXT_MESSAGE, &payload);
            writer.write_all(&pkt).await?;
        }
        writer.flush().await?;

        Ok(())
    }

    async fn send_typing(&self, _user: &ChannelUser) -> Result<(), Box<dyn std::error::Error>> {
        // Mumble has no typing indicator in its protocol.
        Ok(())
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        // Drop the writer to close the TCP connection
        let mut lock = self.stream.lock().await;
        *lock = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mumble_adapter_creation() {
        let adapter = MumbleAdapter::new(
            "mumble-1".to_string(),
            "mumble.example.com".to_string(),
            0,
            "secret".to_string(),
            "OpenFangBot".to_string(),
            "General".to_string(),
        );
        assert_eq!(adapter.name(), "mumble");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("mumble".to_string())
        );
        assert_eq!(adapter.port, DEFAULT_PORT);
    }

    #[test]
    fn test_mumble_custom_port() {
        let adapter = MumbleAdapter::new(
            "mumble-2".to_string(),
            "localhost".to_string(),
            12345,
            "".to_string(),
            "bot".to_string(),
            "Lobby".to_string(),
        );
        assert_eq!(adapter.port, 12345);
    }

    #[test]
    fn test_mumble_packet_encoding() {
        let packet = MumbleAdapter::encode_packet(11, &[0xAA, 0xBB]);
        assert_eq!(packet.len(), 8); // 2 type + 4 len + 2 payload
        assert_eq!(packet[0..2], [0, 11]); // type = 11 (TextMessage)
        assert_eq!(packet[2..6], [0, 0, 0, 2]); // len = 2
        assert_eq!(packet[6..8], [0xAA, 0xBB]);
    }

    #[test]
    fn test_mumble_varint_encode_decode() {
        let mut buf = Vec::new();
        MumbleAdapter::encode_varint(300, &mut buf);
        let (value, consumed) = MumbleAdapter::decode_varint(&buf);
        assert_eq!(value, 300);
        assert_eq!(consumed, buf.len());
    }

    #[test]
    fn test_mumble_text_message_roundtrip() {
        let payload = MumbleAdapter::build_text_message_packet(42, "Hello Mumble!");
        let (actor, ch_ids, _tree_ids, _session_ids, message) =
            MumbleAdapter::parse_text_message(&payload);
        // actor is not set (field 1 omitted) — build only sets channel + message
        assert_eq!(actor, 0);
        assert_eq!(ch_ids, vec![42]);
        assert_eq!(message, "Hello Mumble!");
    }

    #[test]
    fn test_mumble_version_packet() {
        let payload = MumbleAdapter::build_version_packet();
        assert!(!payload.is_empty());
        // First byte should be field 1 tag
        assert_eq!(payload[0], 0x0D);
    }

    #[test]
    fn test_mumble_authenticate_packet() {
        let payload = MumbleAdapter::build_authenticate_packet("bot", "pass");
        assert!(!payload.is_empty());
        assert_eq!(payload[0], 0x0A); // field 1 tag
    }

    #[test]
    fn test_mumble_authenticate_packet_no_password() {
        let payload = MumbleAdapter::build_authenticate_packet("bot", "");
        // No field 2 tag (0x12) should be present
        assert!(!payload.contains(&0x12));
    }
}
