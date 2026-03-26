//! Twitch IRC channel adapter.
//!
//! Connects to Twitch's IRC gateway (`irc.chat.twitch.tv`) over plain TCP and
//! implements the IRC protocol for sending and receiving chat messages. Handles
//! PING/PONG keepalive, channel joins, and PRIVMSG parsing.

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
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use zeroize::Zeroizing;

const TWITCH_IRC_HOST: &str = "irc.chat.twitch.tv";
const TWITCH_IRC_PORT: u16 = 6667;
const MAX_MESSAGE_LEN: usize = 500;

/// Twitch IRC channel adapter.
///
/// Connects to Twitch chat via the IRC protocol and bridges messages to the
/// OpenFang channel system. Supports multiple channels simultaneously.
pub struct TwitchAdapter {
    /// Unique identifier for this adapter instance.
    id: String,
    /// SECURITY: OAuth token is zeroized on drop.
    oauth_token: Zeroizing<String>,
    /// Twitch channels to join (without the '#' prefix).
    channels: Vec<String>,
    /// Bot's IRC nickname.
    nick: String,
    /// Shutdown signal.
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
}

impl TwitchAdapter {
    /// Create a new Twitch adapter.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this adapter instance.
    /// * `oauth_token` - Twitch OAuth token (without the "oauth:" prefix; it will be added).
    /// * `channels` - Channel names to join (without '#' prefix).
    /// * `nick` - Bot's IRC nickname (must match the token owner's Twitch username).
    pub fn new(id: String, oauth_token: String, channels: Vec<String>, nick: String) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            id,
            oauth_token: Zeroizing::new(oauth_token),
            channels,
            nick,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Format the OAuth token for the IRC PASS command.
    fn pass_string(&self) -> String {
        let token = self.oauth_token.as_str();
        if token.starts_with("oauth:") {
            format!("PASS {token}\r\n")
        } else {
            format!("PASS oauth:{token}\r\n")
        }
    }
}

/// Parse an IRC PRIVMSG line into its components.
///
/// Expected format: `:nick!user@host PRIVMSG #channel :message text`
/// Returns `(nick, channel, message)` on success.
fn parse_privmsg(line: &str) -> Option<(String, String, String)> {
    // Must start with ':'
    if !line.starts_with(':') {
        return None;
    }

    let without_prefix = &line[1..];
    let parts: Vec<&str> = without_prefix.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return None;
    }

    let nick = parts[0].split('!').next()?.to_string();
    let rest = parts[1];

    // Expect "PRIVMSG #channel :message"
    if !rest.starts_with("PRIVMSG ") {
        return None;
    }

    let after_cmd = &rest[8..]; // skip "PRIVMSG "
    let channel_end = after_cmd.find(' ')?;
    let channel = after_cmd[..channel_end].to_string();
    let msg_start = after_cmd[channel_end..].find(':')?;
    let message = after_cmd[channel_end + msg_start + 1..].to_string();

    Some((nick, channel, message))
}

#[async_trait]
impl ChannelAdapter for TwitchAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        "twitch"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("twitch".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        info!("Twitch adapter connecting to {TWITCH_IRC_HOST}:{TWITCH_IRC_PORT}");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let pass = self.pass_string();
        let nick_cmd = format!("NICK {}\r\n", self.nick);
        let join_cmds: Vec<String> = self
            .channels
            .iter()
            .map(|ch| {
                let ch = ch.trim_start_matches('#');
                format!("JOIN #{ch}\r\n")
            })
            .collect();
        let bot_nick = self.nick.to_lowercase();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);

            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                // Connect to Twitch IRC
                let stream = match TcpStream::connect((TWITCH_IRC_HOST, TWITCH_IRC_PORT)).await {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Twitch: connection failed: {e}, retrying in {backoff:?}");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(60));
                        continue;
                    }
                };

                let (read_half, mut write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half);

                // Authenticate
                if write_half.write_all(pass.as_bytes()).await.is_err() {
                    warn!("Twitch: failed to send PASS");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                    continue;
                }
                if write_half.write_all(nick_cmd.as_bytes()).await.is_err() {
                    warn!("Twitch: failed to send NICK");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_secs(60));
                    continue;
                }

                // Join channels
                for join in &join_cmds {
                    if write_half.write_all(join.as_bytes()).await.is_err() {
                        warn!("Twitch: failed to send JOIN");
                        break;
                    }
                }

                info!("Twitch IRC connected and joined channels");
                backoff = Duration::from_secs(1);

                // Read loop
                let should_reconnect = loop {
                    let mut line = String::new();
                    let read_result = tokio::select! {
                        _ = shutdown_rx.changed() => {
                            info!("Twitch adapter shutting down");
                            let _ = write_half.write_all(b"QUIT :Shutting down\r\n").await;
                            return;
                        }
                        result = reader.read_line(&mut line) => result,
                    };

                    match read_result {
                        Ok(0) => {
                            info!("Twitch IRC connection closed");
                            break true;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            warn!("Twitch IRC read error: {e}");
                            break true;
                        }
                    }

                    let line = line.trim_end_matches('\n').trim_end_matches('\r');

                    // Handle PING
                    if line.starts_with("PING") {
                        let pong = line.replacen("PING", "PONG", 1);
                        let _ = write_half.write_all(format!("{pong}\r\n").as_bytes()).await;
                        continue;
                    }

                    // Parse PRIVMSG
                    if let Some((sender_nick, channel, message)) = parse_privmsg(line) {
                        // Skip own messages
                        if sender_nick.to_lowercase() == bot_nick {
                            continue;
                        }

                        if message.is_empty() {
                            continue;
                        }

                        let msg_content = if message.starts_with('/') || message.starts_with('!') {
                            let trimmed = message.trim_start_matches('/').trim_start_matches('!');
                            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
                            let cmd = parts[0];
                            let args: Vec<String> = parts
                                .get(1)
                                .map(|a| a.split_whitespace().map(String::from).collect())
                                .unwrap_or_default();
                            ChannelContent::Command {
                                name: cmd.to_string(),
                                args,
                            }
                        } else {
                            ChannelContent::Text(message.clone())
                        };

                        let channel_msg = ChannelMessage {
                            channel: ChannelType::Custom("twitch".to_string()),
                            platform_message_id: uuid::Uuid::new_v4().to_string(),
                            sender: ChannelUser {
                                platform_id: channel.clone(),
                                display_name: sender_nick,
                                openfang_user: None,
                            },
                            content: msg_content,
                            target_agent: None,
                            timestamp: Utc::now(),
                            is_group: true, // Twitch channels are always group
                            thread_id: None,
                            metadata: HashMap::new(),
                        };

                        if tx.send(channel_msg).await.is_err() {
                            return;
                        }
                    }
                };

                if !should_reconnect || *shutdown_rx.borrow() {
                    break;
                }

                warn!("Twitch: reconnecting in {backoff:?}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }

            info!("Twitch IRC loop stopped");
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let channel = &user.platform_id;
        let text = match content {
            ChannelContent::Text(text) => text,
            _ => "(Unsupported content type)".to_string(),
        };

        // Connect briefly to send the message
        // In production, a persistent write connection would be maintained.
        let stream = TcpStream::connect((TWITCH_IRC_HOST, TWITCH_IRC_PORT)).await?;
        let (_reader, mut writer) = stream.into_split();

        writer.write_all(self.pass_string().as_bytes()).await?;
        writer
            .write_all(format!("NICK {}\r\n", self.nick).as_bytes())
            .await?;

        // Wait briefly for auth to complete
        tokio::time::sleep(Duration::from_millis(500)).await;

        let chunks = split_message(&text, MAX_MESSAGE_LEN);
        for chunk in chunks {
            let msg = format!("PRIVMSG {channel} :{chunk}\r\n");
            writer.write_all(msg.as_bytes()).await?;
        }

        writer.write_all(b"QUIT\r\n").await?;
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
    fn test_twitch_adapter_creation() {
        let adapter = TwitchAdapter::new(
            "test-oauth-token".to_string(),
            vec!["testchannel".to_string()],
            "openfang_bot".to_string(),
        );
        assert_eq!(adapter.name(), "twitch");
        assert_eq!(
            adapter.channel_type(),
            ChannelType::Custom("twitch".to_string())
        );
    }

    #[test]
    fn test_twitch_pass_string_with_prefix() {
        let adapter = TwitchAdapter::new("oauth:abc123".to_string(), vec![], "bot".to_string());
        assert_eq!(adapter.pass_string(), "PASS oauth:abc123\r\n");
    }

    #[test]
    fn test_twitch_pass_string_without_prefix() {
        let adapter = TwitchAdapter::new("abc123".to_string(), vec![], "bot".to_string());
        assert_eq!(adapter.pass_string(), "PASS oauth:abc123\r\n");
    }

    #[test]
    fn test_parse_privmsg_valid() {
        let line = ":nick123!user@host PRIVMSG #channel :Hello world!";
        let (nick, channel, message) = parse_privmsg(line).unwrap();
        assert_eq!(nick, "nick123");
        assert_eq!(channel, "#channel");
        assert_eq!(message, "Hello world!");
    }

    #[test]
    fn test_parse_privmsg_no_message() {
        // Missing colon before message
        let line = ":nick!user@host PRIVMSG #channel";
        assert!(parse_privmsg(line).is_none());
    }

    #[test]
    fn test_parse_privmsg_not_privmsg() {
        let line = ":server 001 bot :Welcome";
        assert!(parse_privmsg(line).is_none());
    }

    #[test]
    fn test_parse_privmsg_command() {
        let line = ":user!u@h PRIVMSG #ch :!help me";
        let (nick, channel, message) = parse_privmsg(line).unwrap();
        assert_eq!(nick, "user");
        assert_eq!(channel, "#ch");
        assert_eq!(message, "!help me");
    }

    #[test]
    fn test_parse_privmsg_empty_prefix() {
        let line = "PING :tmi.twitch.tv";
        assert!(parse_privmsg(line).is_none());
    }
}
