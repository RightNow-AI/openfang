//! Consolidated QQ channel implementation.
//!
//! This file combines all QQ bot source files from the legacy `src/qq/` submodule.

// -- BEGIN config.rs --
use std::env;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_TOKEN_URL: &str = "https://bots.qq.com/app/getAppAccessToken";
const DEFAULT_API_BASE_URL: &str = "https://api.sgroup.qq.com";
const DEFAULT_INTENTS: u64 = (1 << 12) | (1 << 25) | (1 << 26) | (1 << 30);
const DEFAULT_WEBHOOK_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_WEBHOOK_PATH: &str = "/qqbot/webhook";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportMode {
    Websocket,
    Webhook,
    Both,
}

impl TransportMode {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "websocket" | "ws" => Ok(Self::Websocket),
            "webhook" | "http" => Ok(Self::Webhook),
            "both" => Ok(Self::Both),
            _ => Err(anyhow!("Invalid QQ_TRANSPORT_MODE value: {value}")),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BotConfig {
    pub app_id: String,
    pub client_secret: String,
    pub token_url: String,
    pub api_base_url: String,
    pub intents: u64,
    pub shard_id: u32,
    pub total_shards: u32,
    pub token_refresh_margin: Duration,
    pub request_timeout: Duration,
    pub reconnect_initial_delay: Duration,
    pub reconnect_max_delay: Duration,
    pub session_store_path: PathBuf,
    pub transport_mode: TransportMode,
    pub webhook_bind_addr: String,
    pub webhook_path: String,
    pub bot_name: Option<String>,
    pub reply_prefix: Option<String>,
    pub admin_openids: Vec<String>,
    pub enable_inline_keyboard: bool,
}

impl BotConfig {
    pub fn from_env() -> Result<Self> {
        let app_id = required_var("QQ_APP_ID")?;
        let client_secret = required_var("QQ_CLIENT_SECRET")?;
        let token_url = env::var("QQ_TOKEN_URL").unwrap_or_else(|_| DEFAULT_TOKEN_URL.to_string());
        let api_base_url =
            env::var("QQ_API_BASE_URL").unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_string());
        let intents = parse_u64_var("QQ_INTENTS")?.unwrap_or(DEFAULT_INTENTS);
        let shard_id = parse_u32_var("QQ_SHARD_ID")?.unwrap_or(0);
        let total_shards = parse_u32_var("QQ_TOTAL_SHARDS")?.unwrap_or(1);
        if shard_id >= total_shards {
            return Err(anyhow!(
                "QQ_SHARD_ID ({shard_id}) must be smaller than QQ_TOTAL_SHARDS ({total_shards})"
            ));
        }

        let token_refresh_margin =
            Duration::from_secs(parse_u64_var("QQ_TOKEN_REFRESH_MARGIN_SECS")?.unwrap_or(60));
        let request_timeout =
            Duration::from_secs(parse_u64_var("QQ_REQUEST_TIMEOUT_SECS")?.unwrap_or(10));
        let reconnect_initial_delay =
            Duration::from_millis(parse_u64_var("QQ_RECONNECT_INITIAL_DELAY_MS")?.unwrap_or(1_000));
        let reconnect_max_delay =
            Duration::from_millis(parse_u64_var("QQ_RECONNECT_MAX_DELAY_MS")?.unwrap_or(30_000));
        let transport_mode = env::var("QQ_TRANSPORT_MODE")
            .ok()
            .as_deref()
            .map(TransportMode::parse)
            .transpose()?
            .unwrap_or(TransportMode::Websocket);

        let session_store_path = env::var("QQ_SESSION_STORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(format!(".qqbot/session-{}-{}.json", app_id, shard_id))
            });
        let webhook_bind_addr = env::var("QQ_WEBHOOK_BIND_ADDR")
            .unwrap_or_else(|_| DEFAULT_WEBHOOK_BIND_ADDR.to_string());
        let webhook_path = normalize_webhook_path(
            env::var("QQ_WEBHOOK_PATH").unwrap_or_else(|_| DEFAULT_WEBHOOK_PATH.to_string()),
        );

        let bot_name = optional_string("QQ_BOT_NAME");
        let reply_prefix = optional_string("QQ_REPLY_PREFIX");
        let admin_openids = optional_csv("QQ_ADMIN_OPENIDS");
        let enable_inline_keyboard = parse_bool_var("QQ_ENABLE_INLINE_KEYBOARD")?.unwrap_or(false);

        Ok(Self {
            app_id,
            client_secret,
            token_url,
            api_base_url,
            intents,
            shard_id,
            total_shards,
            token_refresh_margin,
            request_timeout,
            reconnect_initial_delay,
            reconnect_max_delay,
            session_store_path,
            transport_mode,
            webhook_bind_addr,
            webhook_path,
            bot_name,
            reply_prefix,
            admin_openids,
            enable_inline_keyboard,
        })
    }
}

fn required_var(key: &str) -> Result<String> {
    env::var(key).with_context(|| format!("Missing required environment variable {key}"))
}

fn optional_string(key: &str) -> Option<String> {
    env::var(key).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn optional_csv(key: &str) -> Vec<String> {
    env::var(key)
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_webhook_path(path: String) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        DEFAULT_WEBHOOK_PATH.to_string()
    } else if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    }
}

fn parse_u64_var(key: &str) -> Result<Option<u64>> {
    match env::var(key) {
        Ok(value) => value
            .trim()
            .parse::<u64>()
            .map(Some)
            .with_context(|| format!("Invalid integer value for {key}")),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(anyhow!("Failed to read {key}: {error}")),
    }
}

fn parse_u32_var(key: &str) -> Result<Option<u32>> {
    match env::var(key) {
        Ok(value) => value
            .trim()
            .parse::<u32>()
            .map(Some)
            .with_context(|| format!("Invalid integer value for {key}")),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(anyhow!("Failed to read {key}: {error}")),
    }
}

fn parse_bool_var(key: &str) -> Result<Option<bool>> {
    match env::var(key) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(Some(true)),
            "0" | "false" | "no" | "off" => Ok(Some(false)),
            _ => Err(anyhow!("Invalid boolean value for {key}")),
        },
        Err(env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(anyhow!("Failed to read {key}: {error}")),
    }
}

// -- BEGIN model.rs --
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct WsPayload {
    pub id: Option<String>,
    pub op: u8,
    pub d: Option<Value>,
    pub s: Option<u64>,
    pub t: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HelloData {
    pub heartbeat_interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadyEvent {
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct C2CAuthor {
    pub user_openid: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupAuthor {
    pub member_openid: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GuildAuthor {
    pub id: String,
    pub username: Option<String>,
    pub bot: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Attachment {
    pub content_type: Option<String>,
    pub filename: Option<String>,
    pub url: Option<String>,
    pub voice_wav_url: Option<String>,
    pub asr_refer_text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct C2CMessageEvent {
    pub id: String,
    pub author: C2CAuthor,
    pub content: String,
    pub timestamp: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupAtMessageEvent {
    pub id: String,
    pub author: GroupAuthor,
    pub content: String,
    pub group_openid: String,
    pub timestamp: String,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GuildMessageEvent {
    pub id: String,
    pub author: GuildAuthor,
    pub channel_id: String,
    pub guild_id: String,
    pub content: String,
    pub timestamp: String,
    pub seq: Option<u64>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionEvent {
    pub id: String,
    pub scene: Option<String>,
    pub chat_type: Option<u8>,
    pub guild_id: Option<String>,
    pub channel_id: Option<String>,
    pub user_openid: Option<String>,
    pub group_openid: Option<String>,
    pub group_member_openid: Option<String>,
    pub data: InteractionData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionData {
    #[serde(rename = "type")]
    pub interaction_type: Option<u32>,
    pub resolved: Option<InteractionResolved>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InteractionResolved {
    pub button_data: Option<String>,
    pub button_id: Option<String>,
    pub message_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ValidationRequest {
    pub plain_token: String,
    pub event_ts: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationResponse {
    pub plain_token: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarkdownPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_template_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub params: Vec<MarkdownParam>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarkdownParam {
    pub key: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaPayload {
    pub file_info: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Keyboard {
    pub content: KeyboardContent,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardContent {
    pub rows: Vec<KeyboardRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardRow {
    pub buttons: Vec<KeyboardButton>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardButton {
    pub id: String,
    pub render_data: KeyboardRenderData,
    pub action: KeyboardAction,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardRenderData {
    pub label: String,
    pub visited_label: String,
    pub style: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardAction {
    #[serde(rename = "type")]
    pub action_type: u8,
    pub permission: KeyboardPermission,
    pub data: String,
    pub unsupport_tips: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardPermission {
    #[serde(rename = "type")]
    pub permission_type: u8,
}

// -- BEGIN state.rs --
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use tokio::fs;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub session_id: String,
    pub seq: u64,
}

pub struct Deduper {
    ttl: Duration,
    seen: Mutex<HashMap<String, Instant>>,
}

impl Deduper {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            seen: Mutex::new(HashMap::new()),
        }
    }

    pub async fn mark_seen(&self, key: &str) -> bool {
        let mut seen = self.seen.lock().await;
        let now = Instant::now();
        seen.retain(|_, instant| now.duration_since(*instant) <= self.ttl);
        if seen.contains_key(key) {
            true
        } else {
            seen.insert(key.to_string(), now);
            false
        }
    }
}

pub async fn load_session(path: &Path) -> Result<Option<SessionState>> {
    match fs::read_to_string(path).await {
        Ok(content) => {
            let session = serde_json::from_str::<SessionState>(&content)
                .with_context(|| format!("Failed to parse session file {}", path.display()))?;
            Ok(Some(session))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to read session file {}", path.display()))
        }
    }
}

pub async fn save_session(path: &Path, session: &SessionState) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create session directory {}", parent.display()))?;
    }
    let payload = serde_json::to_vec_pretty(session)?;
    fs::write(path, payload)
        .await
        .with_context(|| format!("Failed to write session file {}", path.display()))
}

pub async fn clear_session(path: &Path) -> Result<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to remove session file {}", path.display()))
        }
    }
}

// -- BEGIN commands.rs --

#[derive(Debug, Clone, Copy)]
pub enum ChatScene {
    C2C,
    Group,
    Guild,
    DirectMessage,
}

impl ChatScene {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::C2C => "c2c",
            Self::Group => "group",
            Self::Guild => "guild",
            Self::DirectMessage => "dm",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InboundContext {
    pub scene: ChatScene,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub message_id: String,
    pub timestamp: String,
    pub group_openid: Option<String>,
    pub guild_id: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub body: OutboundBody,
    pub keyboard: Option<Keyboard>,
}

#[derive(Debug, Clone)]
pub enum OutboundBody {
    Text(String),
    Markdown {
        content: Option<String>,
        template_id: Option<String>,
        params: Vec<MarkdownParam>,
    },
    Media {
        kind: MediaKind,
        url: String,
        caption: Option<String>,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum MediaKind {
    Image,
    Video,
    Voice,
    File,
}

pub fn handle_message(
    content: &str,
    context: &InboundContext,
    config: &BotConfig,
) -> CommandOutput {
    let normalized = normalize_content(content, config.bot_name.as_deref());
    let trimmed = normalized.trim();

    if trimmed.is_empty() {
        return CommandOutput {
            body: OutboundBody::Text(prefixed(
                config,
                "I received your message, but it was empty after mention cleanup. Try /help.",
            )),
            keyboard: maybe_help_keyboard(config),
        };
    }

    let mut parts = trimmed.split_whitespace();
    let command = parts.next().unwrap_or_default();
    let args = parts.collect::<Vec<_>>().join(" ");

    let output = match command {
        "/help" | "help" => help_message(config),
        "/ping" | "ping" => CommandOutput {
            body: OutboundBody::Text(prefixed(
                config,
                &format!(
                    "pong\nscene: {}\nmessage_id: {}",
                    context.scene.as_str(),
                    context.message_id
                ),
            )),
            keyboard: maybe_help_keyboard(config),
        },
        "/echo" | "echo" => {
            let reply = if args.is_empty() {
                "Usage: /echo <text>".to_string()
            } else {
                args
            };
            CommandOutput {
                body: OutboundBody::Text(prefixed(config, &reply)),
                keyboard: None,
            }
        }
        "/info" | "info" => CommandOutput {
            body: OutboundBody::Text(prefixed(
                config,
                &format!(
                    "scene: {}\nsender_id: {}\nsender_name: {}\ngroup_openid: {}\nguild_id: {}\nchannel_id: {}\ntimestamp: {}",
                    context.scene.as_str(),
                    context.sender_id,
                    context.sender_name.as_deref().unwrap_or("unknown"),
                    context.group_openid.as_deref().unwrap_or("-"),
                    context.guild_id.as_deref().unwrap_or("-"),
                    context.channel_id.as_deref().unwrap_or("-"),
                    context.timestamp,
                ),
            )),
            keyboard: None,
        },
        "/md" | "md" => {
            if args.is_empty() {
                CommandOutput {
                    body: OutboundBody::Text(prefixed(config, "Usage: /md <markdown content>")),
                    keyboard: None,
                }
            } else {
                CommandOutput {
                    body: OutboundBody::Markdown {
                        content: Some(args),
                        template_id: None,
                        params: Vec::new(),
                    },
                    keyboard: maybe_help_keyboard(config),
                }
            }
        }
        "/mdtpl" | "mdtpl" => {
            let mut tpl_parts = trimmed.split_whitespace();
            let _ = tpl_parts.next();
            let template_id = tpl_parts.next().unwrap_or_default().to_string();
            let params = tpl_parts
                .filter_map(parse_template_param)
                .collect::<Vec<_>>();
            if template_id.is_empty() {
                CommandOutput {
                    body: OutboundBody::Text(prefixed(
                        config,
                        "Usage: /mdtpl <template_id> key=value [key=value ...]",
                    )),
                    keyboard: None,
                }
            } else {
                CommandOutput {
                    body: OutboundBody::Markdown {
                        content: None,
                        template_id: Some(template_id),
                        params,
                    },
                    keyboard: maybe_help_keyboard(config),
                }
            }
        }
        "/image" | "image" => media_command(config, MediaKind::Image, &args),
        "/video" | "video" => media_command(config, MediaKind::Video, &args),
        "/voice" | "voice" => media_command(config, MediaKind::Voice, &args),
        "/file" | "file" => media_command(config, MediaKind::File, &args),
        "/say" | "say" => {
            if !is_admin(&context.sender_id, config) {
                CommandOutput {
                    body: OutboundBody::Text(prefixed(config, "You are not allowed to use /say.")),
                    keyboard: None,
                }
            } else if args.is_empty() {
                CommandOutput {
                    body: OutboundBody::Text(prefixed(config, "Usage: /say <text>")),
                    keyboard: None,
                }
            } else {
                CommandOutput {
                    body: OutboundBody::Text(prefixed(config, &args)),
                    keyboard: None,
                }
            }
        }
        _ if trimmed.starts_with("/") => CommandOutput {
            body: OutboundBody::Text(prefixed(config, "Unknown command. Try /help.")),
            keyboard: maybe_help_keyboard(config),
        },
        _ => CommandOutput {
            body: OutboundBody::Text(prefixed(config, &format!("You said: {trimmed}"))),
            keyboard: None,
        },
    };

    output
}

pub fn handle_button_action(
    button_data: &str,
    context: &InboundContext,
    config: &BotConfig,
) -> CommandOutput {
    match button_data {
        "cmd:ping" => CommandOutput {
            body: OutboundBody::Text(prefixed(
                config,
                &format!("pong from button\nscene: {}", context.scene.as_str()),
            )),
            keyboard: maybe_help_keyboard(config),
        },
        "cmd:help" => help_message(config),
        _ => CommandOutput {
            body: OutboundBody::Text(prefixed(
                config,
                &format!("Unhandled button action: {button_data}"),
            )),
            keyboard: None,
        },
    }
}

fn help_message(config: &BotConfig) -> CommandOutput {
    CommandOutput {
        body: OutboundBody::Text(prefixed(
            config,
            "Available commands:\n/help\n/ping\n/echo <text>\n/info\n/md <markdown>\n/mdtpl <template_id> key=value\n/image <url> [caption]\n/video <url> [caption]\n/voice <url> [caption]\n/file <url> [caption]\n/say <text> (admin only)",
        )),
        keyboard: maybe_help_keyboard(config),
    }
}

fn media_command(config: &BotConfig, kind: MediaKind, args: &str) -> CommandOutput {
    let mut parts = args.split_whitespace();
    let url = parts.next().unwrap_or_default().to_string();
    let caption = parts.collect::<Vec<_>>().join(" ");
    if url.is_empty() {
        let name = match kind {
            MediaKind::Image => "image",
            MediaKind::Video => "video",
            MediaKind::Voice => "voice",
            MediaKind::File => "file",
        };
        CommandOutput {
            body: OutboundBody::Text(prefixed(config, &format!("Usage: /{name} <url> [caption]"))),
            keyboard: None,
        }
    } else {
        CommandOutput {
            body: OutboundBody::Media {
                kind,
                url,
                caption: if caption.is_empty() {
                    None
                } else {
                    Some(caption)
                },
            },
            keyboard: None,
        }
    }
}

fn parse_template_param(item: &str) -> Option<MarkdownParam> {
    let (key, value) = item.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some(MarkdownParam {
        key: key.to_string(),
        values: vec![value.to_string()],
    })
}

fn prefixed(config: &BotConfig, text: &str) -> String {
    if let Some(prefix) = &config.reply_prefix {
        format!("{prefix}{text}")
    } else {
        text.to_string()
    }
}

fn is_admin(sender_id: &str, config: &BotConfig) -> bool {
    config.admin_openids.iter().any(|item| item == sender_id)
}

fn maybe_help_keyboard(config: &BotConfig) -> Option<Keyboard> {
    if !config.enable_inline_keyboard {
        return None;
    }

    Some(Keyboard {
        content: KeyboardContent {
            rows: vec![KeyboardRow {
                buttons: vec![
                    KeyboardButton {
                        id: "help".to_string(),
                        render_data: KeyboardRenderData {
                            label: "Help".to_string(),
                            visited_label: "Help".to_string(),
                            style: 1,
                        },
                        action: KeyboardAction {
                            action_type: 1,
                            permission: KeyboardPermission { permission_type: 2 },
                            data: "cmd:help".to_string(),
                            unsupport_tips: "Your client does not support QQ bot buttons"
                                .to_string(),
                        },
                    },
                    KeyboardButton {
                        id: "ping".to_string(),
                        render_data: KeyboardRenderData {
                            label: "Ping".to_string(),
                            visited_label: "Ping".to_string(),
                            style: 0,
                        },
                        action: KeyboardAction {
                            action_type: 1,
                            permission: KeyboardPermission { permission_type: 2 },
                            data: "cmd:ping".to_string(),
                            unsupport_tips: "Your client does not support QQ bot buttons"
                                .to_string(),
                        },
                    },
                ],
            }],
        },
    })
}

fn normalize_content(content: &str, bot_name: Option<&str>) -> String {
    let mut output = String::new();
    let chars: Vec<char> = content.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '<' && index + 1 < chars.len() && chars[index + 1] == '@' {
            while index < chars.len() && chars[index] != '>' {
                index += 1;
            }
            if index < chars.len() {
                index += 1;
            }
            output.push(' ');
            continue;
        }

        output.push(chars[index]);
        index += 1;
    }

    let compact = output.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(name) = bot_name {
        compact
            .replace(name, "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        compact
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn test_config() -> BotConfig {
        BotConfig {
            app_id: "smoke-app".to_string(),
            client_secret: "smoke-secret".to_string(),
            token_url: "https://bots.qq.com/app/getAppAccessToken".to_string(),
            api_base_url: "https://api.sgroup.qq.com".to_string(),
            intents: 0,
            shard_id: 0,
            total_shards: 1,
            token_refresh_margin: Duration::from_secs(60),
            request_timeout: Duration::from_secs(10),
            reconnect_initial_delay: Duration::from_millis(1000),
            reconnect_max_delay: Duration::from_millis(30000),
            session_store_path: PathBuf::from(".qqbot/session-test.json"),
            transport_mode: TransportMode::Webhook,
            webhook_bind_addr: "127.0.0.1:18080".to_string(),
            webhook_path: "/qqbot/webhook".to_string(),
            bot_name: Some("MyBot".to_string()),
            reply_prefix: Some("[bot] ".to_string()),
            admin_openids: vec!["admin-openid".to_string()],
            enable_inline_keyboard: false,
        }
    }

    fn test_context() -> InboundContext {
        InboundContext {
            scene: ChatScene::C2C,
            sender_id: "admin-openid".to_string(),
            sender_name: Some("tester".to_string()),
            message_id: "msg-1".to_string(),
            timestamp: "1710000000".to_string(),
            group_openid: None,
            guild_id: None,
            channel_id: None,
        }
    }

    #[test]
    fn ping_returns_text_with_pong() {
        let output = handle_message("/ping", &test_context(), &test_config());
        match output.body {
            OutboundBody::Text(value) => assert!(value.contains("pong")),
            _ => panic!("/ping should return text output"),
        }
    }

    #[test]
    fn markdown_command_returns_markdown_payload() {
        let output = handle_message("/md hello **markdown**", &test_context(), &test_config());
        match output.body {
            OutboundBody::Markdown { content, .. } => {
                assert_eq!(content.as_deref(), Some("hello **markdown**"));
            }
            _ => panic!("/md should return markdown output"),
        }
    }
}

// -- BEGIN api.rs --

use anyhow::{anyhow, bail, Context, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use reqwest::{Client, Method};

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
    config: BotConfig,
    token_state: std::sync::Arc<tokio::sync::RwLock<Option<CachedToken>>>,
}

#[derive(Clone, Debug)]
struct CachedToken {
    access_token: String,
    expires_at: Instant,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    #[serde(deserialize_with = "deserialize_u64_from_string_or_number")]
    expires_in: u64,
}

fn deserialize_u64_from_string_or_number<'de, D>(
    deserializer: D,
) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("expires_in number is not u64")),
        serde_json::Value::String(s) => s
            .parse::<u64>()
            .map_err(|_| serde::de::Error::custom("expires_in string is not a valid u64")),
        _ => Err(serde::de::Error::custom(
            "expires_in must be a number or numeric string",
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct GatewayUrlResponse {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct OutgoingMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub msg_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_seq: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<MarkdownPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<Keyboard>,
}

#[derive(Debug, Deserialize)]
pub struct MessageResponse {
    pub id: String,
    pub timestamp: serde_json::Value,
}

#[derive(Debug, Clone, Copy)]
pub enum MediaFileType {
    Image = 1,
    Video = 2,
    Voice = 3,
    File = 4,
}

#[derive(Debug, Serialize)]
struct FileUploadRequest<'a> {
    file_type: u8,
    url: &'a str,
    srv_send_msg: bool,
}

#[derive(Debug, Deserialize)]
pub struct FileUploadResponse {
    pub file_uuid: String,
    pub file_info: String,
    pub ttl: u64,
    pub id: Option<String>,
}

impl ApiClient {
    pub fn new(config: BotConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static("rust-qqbot/0.1.0"));

        let http = Client::builder()
            .default_headers(headers)
            .timeout(config.request_timeout)
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            http,
            config,
            token_state: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    pub async fn get_gateway_url(&self) -> Result<String> {
        let response = self
            .request_json::<GatewayUrlResponse, &()>(Method::GET, "/gateway", None::<&()>)
            .await?;
        Ok(response.url)
    }

    pub async fn send_c2c_message(
        &self,
        openid: &str,
        message: &OutgoingMessage,
    ) -> Result<MessageResponse> {
        self.request_json(
            Method::POST,
            &format!("/v2/users/{openid}/messages"),
            Some(message),
        )
        .await
    }

    pub async fn send_group_message(
        &self,
        group_openid: &str,
        message: &OutgoingMessage,
    ) -> Result<MessageResponse> {
        self.request_json(
            Method::POST,
            &format!("/v2/groups/{group_openid}/messages"),
            Some(message),
        )
        .await
    }

    pub async fn send_channel_message(
        &self,
        channel_id: &str,
        message: &OutgoingMessage,
    ) -> Result<MessageResponse> {
        self.request_json(
            Method::POST,
            &format!("/channels/{channel_id}/messages"),
            Some(message),
        )
        .await
    }

    pub async fn send_dm_message(
        &self,
        guild_id: &str,
        message: &OutgoingMessage,
    ) -> Result<MessageResponse> {
        self.request_json(
            Method::POST,
            &format!("/dms/{guild_id}/messages"),
            Some(message),
        )
        .await
    }

    pub async fn acknowledge_interaction(&self, interaction_id: &str, code: u8) -> Result<()> {
        #[derive(Serialize)]
        struct AckBody {
            code: u8,
        }

        self.request_empty(
            Method::PUT,
            &format!("/interactions/{interaction_id}"),
            Some(&AckBody { code }),
        )
        .await?;
        Ok(())
    }

    pub async fn upload_c2c_file(
        &self,
        openid: &str,
        file_type: MediaFileType,
        url: &str,
    ) -> Result<FileUploadResponse> {
        self.request_json(
            Method::POST,
            &format!("/v2/users/{openid}/files"),
            Some(&FileUploadRequest {
                file_type: file_type as u8,
                url,
                srv_send_msg: false,
            }),
        )
        .await
    }

    pub async fn upload_group_file(
        &self,
        group_openid: &str,
        file_type: MediaFileType,
        url: &str,
    ) -> Result<FileUploadResponse> {
        self.request_json(
            Method::POST,
            &format!("/v2/groups/{group_openid}/files"),
            Some(&FileUploadRequest {
                file_type: file_type as u8,
                url,
                srv_send_msg: false,
            }),
        )
        .await
    }

    pub async fn access_token(&self) -> Result<String> {
        if let Some(token) = self.cached_token().await {
            return Ok(token);
        }

        let response = self
            .http
            .post(&self.config.token_url)
            .json(&serde_json::json!({
                "appId": self.config.app_id,
                "clientSecret": self.config.client_secret,
            }))
            .send()
            .await
            .context("Failed to request QQ access token")?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            bail!("QQ access token request failed with HTTP {status}: {body_text}");
        }

        // QQ may return HTTP 200 with an error JSON body (e.g. invalid credentials).
        // Check for `access_token` presence before attempting full deserialization so
        // we can surface the real error message.
        if !body_text.contains("access_token") {
            bail!(
                "QQ access token response missing 'access_token' field. \
                 App ID: {}. Response: {body_text}",
                self.config.app_id
            );
        }

        let token = serde_json::from_str::<AccessTokenResponse>(&body_text)
            .with_context(|| format!("Failed to parse QQ access token response: {body_text}"))?;

        let expires_at = Instant::now() + Duration::from_secs(token.expires_in);
        let access_token = token.access_token;

        *self.token_state.write().await = Some(CachedToken {
            access_token: access_token.clone(),
            expires_at,
        });

        Ok(access_token)
    }

    async fn cached_token(&self) -> Option<String> {
        let state = self.token_state.read().await;
        state.as_ref().and_then(|cached| {
            if cached.expires_at > Instant::now() + self.config.token_refresh_margin {
                Some(cached.access_token.clone())
            } else {
                None
            }
        })
    }

    async fn request_json<T, B>(&self, method: Method, path: &str, body: Option<B>) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
        B: Serialize,
    {
        let mut last_error = None;

        for attempt in 0..2 {
            let token = self.access_token().await?;
            let url = format!("{}{}", self.config.api_base_url.trim_end_matches('/'), path);

            let mut request = self
                .http
                .request(method.clone(), &url)
                .header(AUTHORIZATION, format!("QQBot {token}"));

            if let Some(payload) = &body {
                request = request.json(payload);
            }

            let response = request
                .send()
                .await
                .with_context(|| format!("QQ API request failed for {path}"))?;

            if response.status().is_success() {
                return response
                    .json::<T>()
                    .await
                    .with_context(|| format!("Failed to decode QQ API response for {path}"));
            }

            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            if status.as_u16() == 401 && attempt == 0 {
                *self.token_state.write().await = None;
                last_error = Some(anyhow!("QQ API returned 401 for {path}: {body_text}"));
                continue;
            }

            return Err(anyhow!(
                "QQ API request failed for {path} with status {status}: {body_text}"
            ));
        }

        Err(last_error.unwrap_or_else(|| anyhow!("QQ API request failed for {path}")))
    }

    async fn request_empty<B>(&self, method: Method, path: &str, body: Option<B>) -> Result<()>
    where
        B: Serialize,
    {
        let mut last_error = None;

        for attempt in 0..2 {
            let token = self.access_token().await?;
            let url = format!("{}{}", self.config.api_base_url.trim_end_matches('/'), path);

            let mut request = self
                .http
                .request(method.clone(), &url)
                .header(AUTHORIZATION, format!("QQBot {token}"));

            if let Some(payload) = &body {
                request = request.json(payload);
            }

            let response = request
                .send()
                .await
                .with_context(|| format!("QQ API request failed for {path}"))?;

            if response.status().is_success() {
                return Ok(());
            }

            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            if status.as_u16() == 401 && attempt == 0 {
                *self.token_state.write().await = None;
                last_error = Some(anyhow!("QQ API returned 401 for {path}: {body_text}"));
                continue;
            }

            return Err(anyhow!(
                "QQ API request failed for {path} with status {status}: {body_text}"
            ));
        }

        Err(last_error.unwrap_or_else(|| anyhow!("QQ API request failed for {path}")))
    }
}

// -- BEGIN service.rs --
use std::sync::Arc;

use tracing::{debug, info};

#[derive(Clone)]
pub struct BotService {
    api: ApiClient,
    config: BotConfig,
    deduper: Arc<Deduper>,
    /// When set, raw `WsPayload` events are forwarded here instead of
    /// being auto-replied to.  Used by `QqAdapter` (ChannelAdapter bridge).
    adapter_tx: Option<Arc<tokio::sync::mpsc::Sender<WsPayload>>>,
}

impl BotService {
    pub fn new(api: ApiClient, config: BotConfig) -> Self {
        Self {
            api,
            config,
            deduper: Arc::new(Deduper::new(std::time::Duration::from_secs(600))),
            adapter_tx: None,
        }
    }

    /// Create a `BotService` that forwards raw `WsPayload` events to `tx`
    /// instead of generating automatic replies.  Used by `QqAdapter`.
    pub fn with_adapter_tx(
        api: ApiClient,
        config: BotConfig,
        tx: tokio::sync::mpsc::Sender<WsPayload>,
    ) -> Self {
        Self {
            api,
            config,
            deduper: Arc::new(Deduper::new(std::time::Duration::from_secs(600))),
            adapter_tx: Some(Arc::new(tx)),
        }
    }

    pub fn api(&self) -> &ApiClient {
        &self.api
    }

    /// Send a message directly via the QQ API, using an `InboundContext` to
    /// determine the correct scene endpoint.  Used by `QqAdapter::send()`.
    pub async fn handle_send(
        &self,
        context: &InboundContext,
        message: &OutgoingMessage,
    ) -> Result<()> {
        self.send_message(context, message).await
    }

    pub async fn handle_dispatch(&self, payload: WsPayload) -> Result<()> {
        let event_type = payload.t.clone().unwrap_or_default();

        if let Some(event_id) = payload.id.as_deref() {
            if self.deduper.mark_seen(event_id).await {
                tracing::warn!("dropping duplicate event {event_id} ({event_type})");
                return Ok(());
            }
        }

        // If a ChannelAdapter forwarding channel is configured, send the raw
        // payload upstream and skip the built-in auto-reply logic entirely.
        if let Some(ref tx) = self.adapter_tx {
            let _ = tx.try_send(payload);
            return Ok(());
        }

        match event_type.as_str() {
            "READY" => {
                info!("received READY dispatch in shared handler");
            }
            "RESUMED" => {
                info!("received RESUMED dispatch in shared handler");
            }
            "C2C_MESSAGE_CREATE" => {
                let event = serde_json::from_value::<C2CMessageEvent>(
                    payload.d.context("Missing C2C event payload")?,
                )?;
                let content = enrich_content(&event.content, &event.attachments);
                let context = InboundContext {
                    scene: ChatScene::C2C,
                    sender_id: event.author.user_openid,
                    sender_name: None,
                    message_id: event.id.clone(),
                    timestamp: event.timestamp,
                    group_openid: None,
                    guild_id: None,
                    channel_id: None,
                };
                let reply = handle_message(&content, &context, &self.config);
                self.reply(&context, reply.body, reply.keyboard, Some(event.id), None)
                    .await?;
            }
            "GROUP_AT_MESSAGE_CREATE" => {
                let event = serde_json::from_value::<GroupAtMessageEvent>(
                    payload.d.context("Missing group event payload")?,
                )?;
                let content = enrich_content(&event.content, &event.attachments);
                let context = InboundContext {
                    scene: ChatScene::Group,
                    sender_id: event.author.member_openid,
                    sender_name: None,
                    message_id: event.id.clone(),
                    timestamp: event.timestamp,
                    group_openid: Some(event.group_openid),
                    guild_id: None,
                    channel_id: None,
                };
                let reply = handle_message(&content, &context, &self.config);
                self.reply(&context, reply.body, reply.keyboard, Some(event.id), None)
                    .await?;
            }
            "AT_MESSAGE_CREATE" => {
                let event = serde_json::from_value::<GuildMessageEvent>(
                    payload.d.context("Missing guild event payload")?,
                )?;
                let content = enrich_content(&event.content, &event.attachments);
                debug!(message_id = %event.id, seq = ?event.seq, is_bot = ?event.author.bot, attachments = event.attachments.len(), "received guild mention message");
                let context = InboundContext {
                    scene: ChatScene::Guild,
                    sender_id: event.author.id,
                    sender_name: event.author.username,
                    message_id: event.id.clone(),
                    timestamp: event.timestamp,
                    group_openid: None,
                    guild_id: Some(event.guild_id),
                    channel_id: Some(event.channel_id),
                };
                let reply = handle_message(&content, &context, &self.config);
                self.reply(&context, reply.body, reply.keyboard, Some(event.id), None)
                    .await?;
            }
            "DIRECT_MESSAGE_CREATE" => {
                let event = serde_json::from_value::<GuildMessageEvent>(
                    payload.d.context("Missing DM event payload")?,
                )?;
                let content = enrich_content(&event.content, &event.attachments);
                debug!(message_id = %event.id, seq = ?event.seq, is_bot = ?event.author.bot, attachments = event.attachments.len(), "received direct message");
                let context = InboundContext {
                    scene: ChatScene::DirectMessage,
                    sender_id: event.author.id,
                    sender_name: event.author.username,
                    message_id: event.id.clone(),
                    timestamp: event.timestamp,
                    group_openid: None,
                    guild_id: Some(event.guild_id),
                    channel_id: Some(event.channel_id),
                };
                let reply = handle_message(&content, &context, &self.config);
                self.reply(&context, reply.body, reply.keyboard, Some(event.id), None)
                    .await?;
            }
            "INTERACTION_CREATE" => {
                let event = serde_json::from_value::<InteractionEvent>(
                    payload.d.context("Missing interaction payload")?,
                )?;
                debug!(interaction_id = %event.id, scene = ?event.scene, chat_type = ?event.chat_type, interaction_type = ?event.data.interaction_type, button_id = ?event.data.resolved.as_ref().and_then(|value| value.button_id.as_deref()), "received interaction");
                self.api.acknowledge_interaction(&event.id, 0).await?;
                let context = InboundContext {
                    scene: interaction_scene(&event),
                    sender_id: event
                        .group_member_openid
                        .clone()
                        .or_else(|| event.user_openid.clone())
                        .or_else(|| {
                            event
                                .data
                                .resolved
                                .as_ref()
                                .and_then(|value| value.user_id.clone())
                        })
                        .unwrap_or_else(|| "unknown".to_string()),
                    sender_name: None,
                    message_id: event
                        .data
                        .resolved
                        .as_ref()
                        .and_then(|value| value.message_id.clone())
                        .unwrap_or_else(|| event.id.clone()),
                    timestamp: String::new(),
                    group_openid: event.group_openid.clone(),
                    guild_id: event.guild_id.clone(),
                    channel_id: event.channel_id.clone(),
                };

                if let Some(button_data) = event
                    .data
                    .resolved
                    .as_ref()
                    .and_then(|value| value.button_data.as_deref())
                {
                    let reply = handle_button_action(button_data, &context, &self.config);
                    self.reply(
                        &context,
                        reply.body,
                        reply.keyboard,
                        event.data.resolved.and_then(|value| value.message_id),
                        Some(event.id),
                    )
                    .await?;
                }
            }
            other => {
                debug!("ignoring dispatch event {other}");
            }
        }

        Ok(())
    }

    async fn reply(
        &self,
        context: &InboundContext,
        body: OutboundBody,
        keyboard: Option<Keyboard>,
        msg_id: Option<String>,
        event_id: Option<String>,
    ) -> Result<()> {
        match body {
            OutboundBody::Text(text) => {
                let message = OutgoingMessage {
                    content: Some(text),
                    msg_type: 0,
                    msg_id,
                    msg_seq: Some(1),
                    event_id,
                    markdown: None,
                    media: None,
                    keyboard,
                };
                self.send_message(context, &message).await?;
            }
            OutboundBody::Markdown {
                content,
                template_id,
                params,
            } => {
                let message = OutgoingMessage {
                    content: None,
                    msg_type: 2,
                    msg_id,
                    msg_seq: Some(1),
                    event_id,
                    markdown: Some(MarkdownPayload {
                        content,
                        custom_template_id: template_id,
                        params,
                    }),
                    media: None,
                    keyboard,
                };
                self.send_message(context, &message).await?;
            }
            OutboundBody::Media { kind, url, caption } => {
                let file_type = map_media_kind(kind);
                let caption = caption.unwrap_or_else(|| default_media_caption(kind));
                let file_info = match context.scene {
                    ChatScene::C2C => {
                        let upload = self
                            .api
                            .upload_c2c_file(&context.sender_id, file_type, &url)
                            .await?;
                        debug!(file_uuid = %upload.file_uuid, ttl = upload.ttl, sent_message_id = ?upload.id, "uploaded c2c media");
                        upload.file_info
                    }
                    ChatScene::Group => {
                        let group_openid = context
                            .group_openid
                            .as_deref()
                            .context("Missing group_openid for media reply")?;
                        let upload = self
                            .api
                            .upload_group_file(group_openid, file_type, &url)
                            .await?;
                        debug!(file_uuid = %upload.file_uuid, ttl = upload.ttl, sent_message_id = ?upload.id, "uploaded group media");
                        upload.file_info
                    }
                    ChatScene::Guild | ChatScene::DirectMessage => {
                        let fallback = OutboundMessageFallback::unsupported_media(context.scene);
                        let message = OutgoingMessage {
                            content: Some(fallback),
                            msg_type: 0,
                            msg_id,
                            msg_seq: Some(1),
                            event_id,
                            markdown: None,
                            media: None,
                            keyboard,
                        };
                        self.send_message(context, &message).await?;
                        return Ok(());
                    }
                };

                let message = OutgoingMessage {
                    content: Some(caption),
                    msg_type: 7,
                    msg_id,
                    msg_seq: Some(1),
                    event_id,
                    markdown: None,
                    media: Some(MediaPayload { file_info }),
                    keyboard,
                };
                self.send_message(context, &message).await?;
            }
        }

        Ok(())
    }

    async fn send_message(
        &self,
        context: &InboundContext,
        message: &OutgoingMessage,
    ) -> Result<()> {
        match context.scene {
            ChatScene::C2C => {
                let response = self
                    .api
                    .send_c2c_message(&context.sender_id, message)
                    .await?;
                debug!(message_id = %response.id, timestamp = ?response.timestamp, "sent c2c reply");
            }
            ChatScene::Group => {
                let group_openid = context
                    .group_openid
                    .as_deref()
                    .context("Missing group_openid for group reply")?;
                let response = self.api.send_group_message(group_openid, message).await?;
                debug!(message_id = %response.id, timestamp = ?response.timestamp, "sent group reply");
            }
            ChatScene::Guild => {
                let channel_id = context
                    .channel_id
                    .as_deref()
                    .context("Missing channel_id for guild reply")?;
                let response = self.api.send_channel_message(channel_id, message).await?;
                debug!(message_id = %response.id, timestamp = ?response.timestamp, "sent guild reply");
            }
            ChatScene::DirectMessage => {
                let guild_id = context
                    .guild_id
                    .as_deref()
                    .context("Missing guild_id for DM reply")?;
                let response = self.api.send_dm_message(guild_id, message).await?;
                debug!(message_id = %response.id, timestamp = ?response.timestamp, "sent dm reply");
            }
        }
        Ok(())
    }
}

struct OutboundMessageFallback;

impl OutboundMessageFallback {
    fn unsupported_media(scene: ChatScene) -> String {
        format!(
            "Media upload/send is currently supported only in c2c and group scenes, not {}.",
            scene.as_str()
        )
    }
}

fn map_media_kind(kind: MediaKind) -> MediaFileType {
    match kind {
        MediaKind::Image => MediaFileType::Image,
        MediaKind::Video => MediaFileType::Video,
        MediaKind::Voice => MediaFileType::Voice,
        MediaKind::File => MediaFileType::File,
    }
}

fn default_media_caption(kind: MediaKind) -> String {
    match kind {
        MediaKind::Image => "sent an image",
        MediaKind::Video => "sent a video",
        MediaKind::Voice => "sent a voice clip",
        MediaKind::File => "sent a file",
    }
    .to_string()
}

fn enrich_content(content: &str, attachments: &[Attachment]) -> String {
    if attachments.is_empty() {
        return content.to_string();
    }

    let mut parts = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        let kind = attachment.content_type.as_deref().unwrap_or("unknown");
        let name = attachment.filename.as_deref().unwrap_or("unnamed");
        let url = attachment
            .voice_wav_url
            .as_deref()
            .or(attachment.url.as_deref())
            .unwrap_or("no-url");
        let asr = attachment.asr_refer_text.as_deref().unwrap_or("");
        let summary = if asr.is_empty() {
            format!("{kind}:{name}:{url}")
        } else {
            format!("{kind}:{name}:{url}:asr={asr}")
        };
        parts.push(summary);
    }

    if content.trim().is_empty() {
        format!("[attachments] {}", parts.join(" | "))
    } else {
        format!("{content}\n[attachments] {}", parts.join(" | "))
    }
}

fn interaction_scene(event: &InteractionEvent) -> ChatScene {
    match event.chat_type {
        Some(1) => ChatScene::Group,
        Some(2) => ChatScene::C2C,
        _ => {
            if event.channel_id.is_some() {
                ChatScene::Guild
            } else {
                ChatScene::C2C
            }
        }
    }
}

// -- BEGIN gateway.rs --
use std::future::pending;

use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{error, warn};

pub struct GatewayRuntime {
    service: BotService,
    config: BotConfig,
}

impl GatewayRuntime {
    pub fn new(service: BotService, config: BotConfig) -> Self {
        Self { service, config }
    }

    pub async fn run(&self) -> Result<()> {
        let mut delay = self.config.reconnect_initial_delay;

        loop {
            match self.connect_once().await {
                Ok(()) => {
                    delay = self.config.reconnect_initial_delay;
                }
                Err(error) => {
                    error!("gateway connection failed: {error:#}");
                    info!("reconnecting in {} ms", delay.as_millis());
                    sleep(delay).await;
                    delay = std::cmp::min(delay.saturating_mul(2), self.config.reconnect_max_delay);
                }
            }
        }
    }

    async fn connect_once(&self) -> Result<()> {
        let gateway_url = self.service.api().get_gateway_url().await?;
        info!("connecting to gateway {gateway_url}");

        let mut request = gateway_url.into_client_request()?;
        request
            .headers_mut()
            .insert(USER_AGENT, HeaderValue::from_static("rust-qqbot/0.1.0"));

        let (mut stream, _) = connect_async(request)
            .await
            .context("Failed to connect to QQ gateway")?;

        let mut heartbeat: Option<tokio::time::Interval> = None;
        let mut last_seq: Option<u64> = None;
        let mut session = load_session(&self.config.session_store_path).await?;

        loop {
            tokio::select! {
                maybe_message = stream.next() => {
                    let message = match maybe_message {
                        Some(Ok(message)) => message,
                        Some(Err(error)) => return Err(anyhow!(error)).context("Gateway stream error"),
                        None => bail!("Gateway connection closed by remote peer"),
                    };

                    match message {
                        Message::Text(text) => {
                            let payload = serde_json::from_str::<WsPayload>(&text)
                                .with_context(|| format!("Failed to decode gateway payload: {text}"))?;
                            if let Some(seq) = payload.s {
                                last_seq = Some(seq);
                            }
                            self.handle_payload(&mut stream, payload, &mut heartbeat, &mut session, &mut last_seq).await?;
                        }
                        Message::Ping(bytes) => {
                            stream.send(Message::Pong(bytes)).await.context("Failed to answer gateway ping")?;
                        }
                        Message::Close(frame) => {
                            warn!("gateway closed: {frame:?}");
                            bail!("Gateway closed connection");
                        }
                        _ => {}
                    }
                }
                _ = async {
                    if let Some(interval) = &mut heartbeat {
                        interval.tick().await;
                    } else {
                        pending::<()>().await;
                    }
                }, if heartbeat.is_some() => {
                    let heartbeat_payload = json!({ "op": 1, "d": last_seq });
                    stream
                        .send(Message::Text(heartbeat_payload.to_string()))
                        .await
                        .context("Failed to send heartbeat")?;
                    debug!("heartbeat sent with seq {last_seq:?}");
                }
            }
        }
    }

    async fn handle_payload(
        &self,
        stream: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        payload: WsPayload,
        heartbeat: &mut Option<tokio::time::Interval>,
        session: &mut Option<SessionState>,
        last_seq: &mut Option<u64>,
    ) -> Result<()> {
        match payload.op {
            10 => {
                let hello: HelloData =
                    serde_json::from_value(payload.d.context("Missing hello payload")?)?;
                let start = Instant::now() + Duration::from_millis(hello.heartbeat_interval);
                *heartbeat = Some(tokio::time::interval_at(
                    start.into(),
                    Duration::from_millis(hello.heartbeat_interval),
                ));

                let token = self.service.api().access_token().await?;
                let identify = if let Some(saved) = session.as_ref() {
                    info!(
                        "resuming session {} with seq {}",
                        saved.session_id, saved.seq
                    );
                    json!({
                        "op": 6,
                        "d": {
                            "token": format!("QQBot {token}"),
                            "session_id": saved.session_id,
                            "seq": saved.seq,
                        }
                    })
                } else {
                    info!(
                        "identifying shard [{}, {}] with intents {}",
                        self.config.shard_id, self.config.total_shards, self.config.intents
                    );
                    json!({
                        "op": 2,
                        "d": {
                            "token": format!("QQBot {token}"),
                            "intents": self.config.intents,
                            "shard": [self.config.shard_id, self.config.total_shards],
                            "properties": {
                                "$os": "linux",
                                "$browser": "rust-qqbot",
                                "$device": "rust-qqbot",
                            }
                        }
                    })
                };

                stream
                    .send(Message::Text(identify.to_string()))
                    .await
                    .context("Failed to send identify/resume")?;
            }
            11 => {
                debug!("heartbeat ack received");
            }
            7 => {
                bail!("Gateway requested reconnect");
            }
            9 => {
                warn!("invalid session received, clearing cached session");
                clear_session(&self.config.session_store_path).await?;
                *session = None;
                *last_seq = None;
                bail!("Gateway invalidated session");
            }
            0 => {
                self.handle_dispatch(payload, session, last_seq).await?;
            }
            _ => {
                debug!("ignoring opcode {}", payload.op);
            }
        }
        Ok(())
    }

    async fn handle_dispatch(
        &self,
        payload: WsPayload,
        session: &mut Option<SessionState>,
        last_seq: &mut Option<u64>,
    ) -> Result<()> {
        let event_type = payload.t.clone().unwrap_or_default();

        match event_type.as_str() {
            "READY" => {
                let ready = serde_json::from_value::<ReadyEvent>(
                    payload.d.context("Missing READY payload")?,
                )?;
                let seq = last_seq.unwrap_or(0);
                let saved = SessionState {
                    session_id: ready.session_id,
                    seq,
                };
                save_session(&self.config.session_store_path, &saved).await?;
                *session = Some(saved);
                info!("gateway ready");
            }
            "RESUMED" => {
                if let Some(saved) = session.as_mut() {
                    saved.seq = last_seq.unwrap_or(saved.seq);
                    save_session(&self.config.session_store_path, saved).await?;
                }
                info!("gateway resumed");
            }
            other => {
                let payload = WsPayload {
                    id: payload.id,
                    op: 0,
                    d: payload.d,
                    s: payload.s,
                    t: Some(other.to_string()),
                };
                self.service.handle_dispatch(payload).await?;
            }
        }

        if let Some(saved) = session.as_mut() {
            if let Some(seq) = *last_seq {
                saved.seq = seq;
                save_session(&self.config.session_store_path, saved).await?;
            }
        }

        Ok(())
    }
}

// -- BEGIN webhook.rs --

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use tokio::net::TcpListener;

#[derive(Clone)]
pub struct WebhookRuntime {
    service: BotService,
    config: BotConfig,
}

#[derive(Clone)]
struct WebhookState {
    service: BotService,
    config: BotConfig,
    signing_key: Arc<SigningKey>,
    verifying_key: VerifyingKey,
}

impl WebhookRuntime {
    pub fn new(service: BotService, config: BotConfig) -> Self {
        Self { service, config }
    }

    pub async fn run(&self) -> Result<()> {
        let signing_key = Arc::new(derive_signing_key(&self.config.client_secret)?);
        let state = WebhookState {
            service: self.service.clone(),
            config: self.config.clone(),
            verifying_key: signing_key.verifying_key(),
            signing_key,
        };

        let app = Router::new()
            .route(&self.config.webhook_path, post(webhook_handler))
            .route("/health", get(health_handler))
            .with_state(state);

        let listener = TcpListener::bind(&self.config.webhook_bind_addr)
            .await
            .with_context(|| {
                format!(
                    "Failed to bind webhook listener on {}",
                    self.config.webhook_bind_addr
                )
            })?;
        info!(bind = %self.config.webhook_bind_addr, path = %self.config.webhook_path, "webhook server listening");
        axum::serve(listener, app)
            .await
            .context("Webhook server stopped unexpectedly")
    }
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn webhook_handler(
    State(state): State<WebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match handle_webhook_request(state, headers, body).await {
        Ok(response) => response,
        Err(error) => {
            error!("webhook request failed: {error:#}");
            (StatusCode::BAD_REQUEST, error.to_string()).into_response()
        }
    }
}

async fn handle_webhook_request(
    state: WebhookState,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response> {
    verify_request(&state, &headers, &body)?;
    if let Some(appid) = headers
        .get("x-bot-appid")
        .and_then(|value| value.to_str().ok())
    {
        if appid != state.config.app_id {
            bail!("X-Bot-Appid header does not match configured app id");
        }
    }

    let payload =
        serde_json::from_slice::<WsPayload>(&body).context("Failed to parse webhook payload")?;

    match payload.op {
        13 => {
            let validation = serde_json::from_value::<ValidationRequest>(
                payload.d.context("Missing webhook validation payload")?,
            )?;
            let response = ValidationResponse {
                plain_token: validation.plain_token.clone(),
                signature: sign_validation(
                    &state.signing_key,
                    &validation.event_ts,
                    &validation.plain_token,
                ),
            };
            Ok((StatusCode::OK, Json(response)).into_response())
        }
        0 => {
            state.service.handle_dispatch(payload).await?;
            Ok((StatusCode::OK, Json(serde_json::json!({ "op": 12 }))).into_response())
        }
        12 => Ok((StatusCode::OK, Json(serde_json::json!({ "op": 12 }))).into_response()),
        other => Ok((StatusCode::OK, Json(serde_json::json!({ "op": other }))).into_response()),
    }
}

fn verify_request(state: &WebhookState, headers: &HeaderMap, body: &Bytes) -> Result<()> {
    let signature_hex = headers
        .get("x-signature-ed25519")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| anyhow!("Missing X-Signature-Ed25519 header"))?;
    let timestamp = headers
        .get("x-signature-timestamp")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| anyhow!("Missing X-Signature-Timestamp header"))?;

    let signature_bytes = hex::decode(signature_hex).context("Invalid X-Signature-Ed25519 hex")?;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .context("Invalid Ed25519 signature length")?;

    let mut message = timestamp.as_bytes().to_vec();
    message.extend_from_slice(body);

    state
        .verifying_key
        .verify(&message, &signature)
        .map_err(|error| anyhow!("Webhook signature verification failed: {error}"))
}

fn derive_signing_key(secret: &str) -> Result<SigningKey> {
    if secret.is_empty() {
        bail!("Webhook signing secret is empty");
    }

    let mut seed = secret.to_string();
    while seed.len() < 32 {
        seed = format!("{seed}{seed}");
    }
    let seed = &seed.as_bytes()[..32];
    let secret_bytes: [u8; 32] = seed
        .try_into()
        .map_err(|_| anyhow!("Failed to derive 32-byte signing seed"))?;
    Ok(SigningKey::from_bytes(&secret_bytes))
}

fn sign_validation(signing_key: &SigningKey, event_ts: &str, plain_token: &str) -> String {
    let mut message = event_ts.as_bytes().to_vec();
    message.extend_from_slice(plain_token.as_bytes());
    hex::encode(signing_key.sign(&message).to_bytes())
}
// -- BEGIN adapter.rs (QqAdapter — OpenFang ChannelAdapter integration) --

use crate::types::{
    ChannelAdapter, ChannelContent, ChannelMessage, ChannelType, ChannelUser,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::Stream;
use std::pin::Pin;
use tokio::sync::{mpsc, watch, RwLock as TokioRwLock};
use tokio_stream::wrappers::ReceiverStream;

/// Stored reply context — enough to route a `send()` call back to the
/// correct QQ chat scene (C2C, Group, Guild, or DirectMessage).
#[derive(Clone, Debug)]
struct QqReplyCtx {
    scene: ChatScene,
    sender_id: String,
    group_openid: Option<String>,
    guild_id: Option<String>,
    channel_id: Option<String>,
    /// The ID of the original inbound message — used for passive QQ replies.
    msg_id: Option<String>,
    /// Ever-incrementing sequence counter for this user (shared across clones via Arc).
    /// QQ uses msg_seq for deduplication; each chunk in a streaming response must
    /// have a unique, incrementing seq when the same msg_id is reused.
    seq: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

/// OpenFang `ChannelAdapter` for the QQ Open Platform.
///
/// Translates QQ C2C / Group / Guild / DM events into unified
/// `ChannelMessage` events for the kernel and routes `send()` calls back
/// via the QQ API.  Supports WebSocket gateway, Webhook, and Both modes.
pub struct QqAdapter {
    config: BotConfig,
    api: ApiClient,
    /// Maps sender platform_id → latest reply context for routing outbound messages.
    reply_map: Arc<TokioRwLock<std::collections::HashMap<String, QqReplyCtx>>>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    /// Whether streaming delivery is enabled for this adapter.
    streaming: bool,
}

impl QqAdapter {
    /// Create a `QqAdapter` from explicit credentials.
    ///
    /// * `app_id`          — QQ Bot App ID from the QQ Open Platform console.
    /// * `client_secret`   — plaintext client secret (already resolved from env var by the caller).
    /// * `transport_mode`  — `"websocket"` | `"webhook"` | `"both"`.
    /// * `webhook_port`    — TCP port for the inbound webhook HTTP server.
    /// * `webhook_path`    — URL path (e.g. `"/qqbot/webhook"`).
    pub fn new(
        app_id: String,
        client_secret: String,
        transport_mode: &str,
        webhook_port: u16,
        webhook_path: String,
        streaming: bool,
    ) -> Result<Self> {
        let mode = TransportMode::parse(transport_mode).unwrap_or(TransportMode::Websocket);
        let config = BotConfig {
            app_id: app_id.clone(),
            client_secret,
            token_url: DEFAULT_TOKEN_URL.to_string(),
            api_base_url: DEFAULT_API_BASE_URL.to_string(),
            intents: DEFAULT_INTENTS,
            shard_id: 0,
            total_shards: 1,
            token_refresh_margin: Duration::from_secs(60),
            request_timeout: Duration::from_secs(10),
            reconnect_initial_delay: Duration::from_millis(1_000),
            reconnect_max_delay: Duration::from_millis(30_000),
            transport_mode: mode,
            session_store_path: std::path::PathBuf::from(format!(
                ".qqbot/session-{}-0.json",
                &app_id
            )),
            webhook_bind_addr: format!("0.0.0.0:{}", webhook_port),
            webhook_path: normalize_webhook_path(webhook_path),
            bot_name: None,
            reply_prefix: None,
            admin_openids: Vec::new(),
            enable_inline_keyboard: false,
        };
        let api = ApiClient::new(config.clone())?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Ok(Self {
            config,
            api,
            reply_map: Arc::new(TokioRwLock::new(std::collections::HashMap::new())),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            streaming,
        })
    }

    /// Parse a raw `WsPayload` into a `ChannelMessage`, storing the reply
    /// context so `send()` can route replies back to the correct QQ scene.
    async fn parse_and_store(
        payload: WsPayload,
        reply_map: &TokioRwLock<std::collections::HashMap<String, QqReplyCtx>>,
    ) -> Option<ChannelMessage> {
        let event_type = payload.t.as_deref().unwrap_or_default();
        match event_type {
            "C2C_MESSAGE_CREATE" => {
                let event: C2CMessageEvent =
                    serde_json::from_value(payload.d?).ok()?;
                let text = enrich_content(&event.content, &event.attachments);
                let uid = event.author.user_openid.clone();
                let event_id = event.id.clone();
                reply_map.write().await.insert(
                    uid.clone(),
                    QqReplyCtx {
                        scene: ChatScene::C2C,
                        sender_id: uid.clone(),
                        group_openid: None,
                        guild_id: None,
                        channel_id: None,
                        msg_id: Some(event_id.clone()),
                        seq: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1)),
                    },
                );
                Some(ChannelMessage {
                    channel: ChannelType::Custom("qq".to_string()),
                    platform_message_id: event_id,
                    sender: ChannelUser {
                        platform_id: uid,
                        display_name: String::new(),
                        openfang_user: None,
                    },
                    content: ChannelContent::Text(text),
                    target_agent: None,
                    timestamp: Utc::now(),
                    is_group: false,
                    thread_id: None,
                    metadata: std::collections::HashMap::new(),
                })
            }
            "GROUP_AT_MESSAGE_CREATE" => {
                let event: GroupAtMessageEvent =
                    serde_json::from_value(payload.d?).ok()?;
                let text = enrich_content(&event.content, &event.attachments);
                let uid = event.author.member_openid.clone();
                let grp = event.group_openid.clone();
                let event_id = event.id.clone();
                reply_map.write().await.insert(
                    uid.clone(),
                    QqReplyCtx {
                        scene: ChatScene::Group,
                        sender_id: uid.clone(),
                        group_openid: Some(grp.clone()),
                        guild_id: None,
                        channel_id: None,
                        msg_id: Some(event_id.clone()),
                        seq: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1)),
                    },
                );
                Some(ChannelMessage {
                    channel: ChannelType::Custom("qq".to_string()),
                    platform_message_id: event_id,
                    sender: ChannelUser {
                        platform_id: uid,
                        display_name: String::new(),
                        openfang_user: None,
                    },
                    content: ChannelContent::Text(text),
                    target_agent: None,
                    timestamp: Utc::now(),
                    is_group: true,
                    thread_id: Some(grp),
                    metadata: std::collections::HashMap::new(),
                })
            }
            "AT_MESSAGE_CREATE" | "DIRECT_MESSAGE_CREATE" => {
                let event: GuildMessageEvent =
                    serde_json::from_value(payload.d?).ok()?;
                let text = enrich_content(&event.content, &event.attachments);
                let uid = event.author.id.clone();
                let is_dm = event_type == "DIRECT_MESSAGE_CREATE";
                let scene = if is_dm {
                    ChatScene::DirectMessage
                } else {
                    ChatScene::Guild
                };
                let event_id = event.id.clone();
                reply_map.write().await.insert(
                    uid.clone(),
                    QqReplyCtx {
                        scene,
                        sender_id: uid.clone(),
                        group_openid: None,
                        guild_id: Some(event.guild_id.clone()),
                        channel_id: Some(event.channel_id.clone()),
                        msg_id: Some(event_id.clone()),
                        seq: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1)),
                    },
                );
                Some(ChannelMessage {
                    channel: ChannelType::Custom("qq".to_string()),
                    platform_message_id: event_id,
                    sender: ChannelUser {
                        platform_id: uid,
                        display_name: event.author.username.unwrap_or_default(),
                        openfang_user: None,
                    },
                    content: ChannelContent::Text(text),
                    target_agent: None,
                    timestamp: Utc::now(),
                    is_group: !is_dm,
                    thread_id: Some(event.channel_id),
                    metadata: std::collections::HashMap::new(),
                })
            }
            _ => None,
        }
    }
}

#[async_trait]
impl ChannelAdapter for QqAdapter {
    fn name(&self) -> &str {
        "qq"
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Custom("qq".to_string())
    }

    async fn start(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = ChannelMessage> + Send>>, Box<dyn std::error::Error>>
    {
        // Validate credentials eagerly.
        self.api.access_token().await.map_err(|e| {
            format!("QQ auth failed — check app_id / QQ_CLIENT_SECRET: {e}")
        })?;
        tracing::info!(app_id = %self.config.app_id, "QQ adapter started");

        let (tx, rx) = mpsc::channel::<ChannelMessage>(256);
        let (raw_tx, mut raw_rx) = mpsc::channel::<WsPayload>(512);

        // Task: convert raw WsPayload events into ChannelMessage.
        let tx_clone = tx.clone();
        let rm_clone = Arc::clone(&self.reply_map);
        tokio::spawn(async move {
            while let Some(payload) = raw_rx.recv().await {
                if let Some(msg) = QqAdapter::parse_and_store(payload, &rm_clone).await {
                    if tx_clone.send(msg).await.is_err() {
                        break;
                    }
                }
            }
        });

        // Build a BotService that forwards raw events to `raw_tx`.
        let service = BotService::with_adapter_tx(
            self.api.clone(),
            self.config.clone(),
            raw_tx,
        );
        let config = self.config.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        tokio::spawn(async move {
            let result = match config.transport_mode {
                TransportMode::Websocket => {
                    let rt = GatewayRuntime::new(service, config);
                    tokio::select! {
                        res = rt.run() => res,
                        _ = shutdown_rx.changed() => Ok(()),
                    }
                }
                TransportMode::Webhook => {
                    let rt = WebhookRuntime::new(service, config);
                    tokio::select! {
                        res = rt.run() => res,
                        _ = shutdown_rx.changed() => Ok(()),
                    }
                }
                TransportMode::Both => {
                    let service2 = service.clone();
                    let config2 = config.clone();
                    let ws_rt = GatewayRuntime::new(service, config);
                    let wh_rt = WebhookRuntime::new(service2, config2);
                    tokio::select! {
                        res = ws_rt.run() => res,
                        res = wh_rt.run() => res,
                        _ = shutdown_rx.changed() => Ok(()),
                    }
                }
            };
            if let Err(e) = result {
                tracing::error!("QQ adapter runtime stopped: {e:#}");
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    async fn send(
        &self,
        user: &ChannelUser,
        content: ChannelContent,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let text = match &content {
            ChannelContent::Text(t) => t.clone(),
            ChannelContent::Image { url, caption } => {
                caption.clone().unwrap_or_else(|| url.clone())
            }
            other => {
                tracing::warn!("QQ: content type unsupported, sending as text description");
                format!("{other:?}")
            }
        };

        let uid = &user.platform_id;
        let ctx_opt = self.reply_map.read().await.get(uid.as_str()).cloned();

        // For passive replies, carry the original msg_id and increment msg_seq so
        // QQ can differentiate each streaming chunk (QQ deduplicates by seq per msg_id).
        // QQ allows up to 5 passive replies per msg_id; after that we send proactively.
        let (reply_msg_id, reply_seq) = match &ctx_opt {
            Some(ctx) => {
                let seq = ctx.seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if seq <= 5 {
                    (ctx.msg_id.clone(), seq)
                } else {
                    // Exceeded passive-reply quota — fall through to proactive send
                    (None, seq)
                }
            }
            None => (None, 1),
        };

        let msg = OutgoingMessage {
            content: Some(text),
            msg_type: 0,
            msg_id: reply_msg_id,
            msg_seq: Some(reply_seq),
            event_id: None,
            markdown: None,
            media: None,
            keyboard: None,
        };
        match ctx_opt {
            Some(ctx) => {
                let ic = InboundContext {
                    scene: ctx.scene,
                    sender_id: ctx.sender_id.clone(),
                    sender_name: None,
                    message_id: String::new(),
                    timestamp: String::new(),
                    group_openid: ctx.group_openid.clone(),
                    guild_id: ctx.guild_id.clone(),
                    channel_id: ctx.channel_id.clone(),
                };
                let svc = BotService::new(self.api.clone(), self.config.clone());
                svc.handle_send(&ic, &msg).await
                    .map_err(|e| format!("QQ send error: {e}").into())
            }
            None => {
                // No stored context: best-effort C2C send.
                self.api
                    .send_c2c_message(uid, &msg)
                    .await
                    .map(|_| ())
                    .map_err(|e| format!("QQ C2C send error: {e}").into())
            }
        }
    }

    fn supports_streaming(&self) -> bool {
        self.streaming
    }

    async fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.shutdown_tx.send(true);
        tracing::info!("QQ adapter stopped");
        Ok(())
    }
}
