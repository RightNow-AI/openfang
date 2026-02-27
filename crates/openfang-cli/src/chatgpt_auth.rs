//! ChatGPT subscription auth state helpers.
//!
//! Phase 1 scope:
//! - Persist externally obtained ChatGPT auth tokens in ~/.openfang/.env
//! - Show auth status
//! - Clear stored auth state

use crate::dotenv;
use base64::Engine;
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const CHATGPT_ACCESS_TOKEN_ENV: &str = "CHATGPT_ACCESS_TOKEN";
pub const CHATGPT_REFRESH_TOKEN_ENV: &str = "CHATGPT_REFRESH_TOKEN";
pub const CHATGPT_ACCOUNT_ID_ENV: &str = "CHATGPT_ACCOUNT_ID";
pub const CHATGPT_PLAN_TYPE_ENV: &str = "CHATGPT_PLAN_TYPE";
pub const CHATGPT_TOKEN_EXPIRES_AT_ENV: &str = "CHATGPT_TOKEN_EXPIRES_AT";

#[derive(Debug, Clone, Default)]
pub struct ChatGptAuthStatus {
    pub has_access_token: bool,
    pub has_refresh_token: bool,
    pub account_id: Option<String>,
    pub plan_type: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChatGptTokenBundle {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    pub plan_type: Option<String>,
    pub expires_at: Option<String>,
}

pub fn load_status() -> ChatGptAuthStatus {
    ChatGptAuthStatus {
        has_access_token: std::env::var(CHATGPT_ACCESS_TOKEN_ENV)
            .ok()
            .map(|v| !v.is_empty())
            .unwrap_or(false),
        has_refresh_token: std::env::var(CHATGPT_REFRESH_TOKEN_ENV)
            .ok()
            .map(|v| !v.is_empty())
            .unwrap_or(false),
        account_id: std::env::var(CHATGPT_ACCOUNT_ID_ENV).ok(),
        plan_type: std::env::var(CHATGPT_PLAN_TYPE_ENV).ok(),
        expires_at: std::env::var(CHATGPT_TOKEN_EXPIRES_AT_ENV).ok(),
    }
}

pub fn save_token_bundle(bundle: &ChatGptTokenBundle) -> Result<(), String> {
    if bundle.access_token.trim().is_empty() {
        return Err("Access token cannot be empty".to_string());
    }

    dotenv::save_env_key(CHATGPT_ACCESS_TOKEN_ENV, bundle.access_token.trim())?;

    if let Some(ref refresh) = bundle.refresh_token {
        if !refresh.trim().is_empty() {
            dotenv::save_env_key(CHATGPT_REFRESH_TOKEN_ENV, refresh.trim())?;
        }
    }
    if let Some(ref account_id) = bundle.account_id {
        if !account_id.trim().is_empty() {
            dotenv::save_env_key(CHATGPT_ACCOUNT_ID_ENV, account_id.trim())?;
        }
    }
    if let Some(ref plan_type) = bundle.plan_type {
        if !plan_type.trim().is_empty() {
            dotenv::save_env_key(CHATGPT_PLAN_TYPE_ENV, plan_type.trim())?;
        }
    }
    if let Some(ref expires_at) = bundle.expires_at {
        if !expires_at.trim().is_empty() {
            dotenv::save_env_key(CHATGPT_TOKEN_EXPIRES_AT_ENV, expires_at.trim())?;
        }
    }

    Ok(())
}

pub fn clear_auth_state() -> Result<(), String> {
    dotenv::remove_env_key(CHATGPT_ACCESS_TOKEN_ENV)?;
    dotenv::remove_env_key(CHATGPT_REFRESH_TOKEN_ENV)?;
    dotenv::remove_env_key(CHATGPT_ACCOUNT_ID_ENV)?;
    dotenv::remove_env_key(CHATGPT_PLAN_TYPE_ENV)?;
    dotenv::remove_env_key(CHATGPT_TOKEN_EXPIRES_AT_ENV)?;
    Ok(())
}

/// Best-effort extraction of `chatgpt_account_id` from JWT payload claims.
pub fn extract_chatgpt_account_id_from_jwt(jwt: &str) -> Option<String> {
    let payload_b64 = jwt.split('.').nth(1)?;
    let payload_raw = decode_base64url(payload_b64)?;
    let value: Value = serde_json::from_slice(&payload_raw).ok()?;
    value
        .get("chatgpt_account_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Compute an absolute Unix epoch second string from now + `expires_in`.
pub fn compute_expires_at_epoch(expires_in: u64) -> Option<String> {
    if expires_in == 0 {
        return None;
    }
    let expires = SystemTime::now().checked_add(Duration::from_secs(expires_in))?;
    let epoch = expires.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(epoch.to_string())
}

fn decode_base64url(input: &str) -> Option<Vec<u8>> {
    let mut s = input.replace('-', "+").replace('_', "/");
    while s.len() % 4 != 0 {
        s.push('=');
    }
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}
