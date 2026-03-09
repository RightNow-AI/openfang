//! # maestro-sdk
//!
//! Rust embedding SDK for the Maestro-OpenFang platform.
//!
//! Provides a typed async HTTP client for the REST API, agent lifecycle
//! management, session handling, and observability queries.

use anyhow::Context;
use chrono::{DateTime, Utc};
use reqwest::{header, Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("HTTP error {status}: {message}")]
    Http { status: u16, message: String },
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Unauthorized: check your API key")]
    Unauthorized,
    #[error("Rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
}

pub type SdkResult<T> = Result<T, SdkError>;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub api_key: String,
    pub timeout_secs: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:3000".to_string(),
            api_key: String::new(),
            timeout_secs: 30,
        }
    }
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: AgentStatus,
    pub capabilities: Vec<String>,
    pub model: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Active,
    Idle,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub agent_id: String,
    pub created_at: DateTime<Utc>,
    pub message_count: u32,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub session_id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub tokens_used: Option<u32>,
    pub latency_ms: Option<u64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageResponse {
    pub message: Message,
    pub session: Session,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_context: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceInfo {
    pub trace_id: String,
    pub session_id: Uuid,
    pub operation: String,
    pub duration_ms: u64,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub agents_active: u32,
    pub sessions_active: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallSkillRequest {
    pub name: String,
    pub version: String,
}

// ---------------------------------------------------------------------------
// MaestroClient
// ---------------------------------------------------------------------------

/// Typed async HTTP client for the Maestro-OpenFang REST API.
#[derive(Clone)]
pub struct MaestroClient {
    config: ClientConfig,
    http: Client,
}

impl MaestroClient {
    pub fn new(config: ClientConfig) -> Self {
        let mut headers = header::HeaderMap::new();
        if !config.api_key.is_empty() {
            if let Ok(val) = header::HeaderValue::from_str(&format!("Bearer {}", config.api_key)) {
                headers.insert(header::AUTHORIZATION, val);
            }
        }
        headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
        let http = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers(headers)
            .build()
            .expect("Failed to build HTTP client");
        Self { config, http }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    async fn handle_response<T: for<'de> Deserialize<'de>>(resp: reqwest::Response) -> SdkResult<T> {
        let status = resp.status();
        match status {
            StatusCode::UNAUTHORIZED => return Err(SdkError::Unauthorized),
            StatusCode::TOO_MANY_REQUESTS => {
                let retry_after = resp.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60);
                return Err(SdkError::RateLimited { retry_after_secs: retry_after });
            }
            StatusCode::NOT_FOUND => {
                let body = resp.text().await.unwrap_or_default();
                return Err(SdkError::NotFound(body));
            }
            s if !s.is_success() => {
                let body = resp.text().await.unwrap_or_default();
                return Err(SdkError::Http { status: s.as_u16(), message: body });
            }
            _ => {}
        }
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    // Health
    pub async fn health(&self) -> SdkResult<HealthStatus> {
        let resp = self.http.get(self.url("/health")).send().await?;
        Self::handle_response(resp).await
    }

    // Agents
    pub async fn list_agents(&self) -> SdkResult<Vec<AgentInfo>> {
        let resp = self.http.get(self.url("/api/agents")).send().await?;
        Self::handle_response(resp).await
    }

    pub async fn get_agent(&self, agent_id: &str) -> SdkResult<AgentInfo> {
        let resp = self.http.get(self.url(&format!("/api/agents/{}", agent_id))).send().await?;
        Self::handle_response(resp).await
    }

    pub fn agent(&self, agent_id: &str) -> AgentHandle {
        AgentHandle { client: self.clone(), agent_id: agent_id.to_string() }
    }

    // Sessions
    pub async fn create_session(&self, req: CreateSessionRequest) -> SdkResult<Session> {
        let resp = self.http.post(self.url("/api/sessions")).json(&req).send().await?;
        Self::handle_response(resp).await
    }

    pub async fn get_session(&self, session_id: Uuid) -> SdkResult<Session> {
        let resp = self.http.get(self.url(&format!("/api/sessions/{}", session_id))).send().await?;
        Self::handle_response(resp).await
    }

    pub async fn list_sessions(&self, agent_id: Option<&str>) -> SdkResult<Vec<Session>> {
        let url = match agent_id {
            Some(id) => self.url(&format!("/api/sessions?agent_id={}", id)),
            None => self.url("/api/sessions"),
        };
        let resp = self.http.get(url).send().await?;
        Self::handle_response(resp).await
    }

    pub async fn delete_session(&self, session_id: Uuid) -> SdkResult<()> {
        let resp = self.http.delete(self.url(&format!("/api/sessions/{}", session_id))).send().await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(SdkError::Http { status: resp.status().as_u16(), message: resp.text().await.unwrap_or_default() })
        }
    }

    // Messages
    pub async fn send_message(&self, session_id: Uuid, req: SendMessageRequest) -> SdkResult<SendMessageResponse> {
        let resp = self.http
            .post(self.url(&format!("/api/sessions/{}/messages", session_id)))
            .json(&req).send().await?;
        Self::handle_response(resp).await
    }

    pub async fn get_messages(&self, session_id: Uuid) -> SdkResult<Vec<Message>> {
        let resp = self.http.get(self.url(&format!("/api/sessions/{}/messages", session_id))).send().await?;
        Self::handle_response(resp).await
    }

    // Observability
    pub async fn get_traces(&self, session_id: Uuid) -> SdkResult<Vec<TraceInfo>> {
        let resp = self.http
            .get(self.url(&format!("/api/observability/traces?session_id={}", session_id)))
            .send().await?;
        Self::handle_response(resp).await
    }

    // Marketplace
    pub async fn install_skill(&self, req: InstallSkillRequest) -> SdkResult<()> {
        let resp = self.http.post(self.url("/api/marketplace/install")).json(&req).send().await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(SdkError::Http { status: resp.status().as_u16(), message: resp.text().await.unwrap_or_default() })
        }
    }
}

// ---------------------------------------------------------------------------
// AgentHandle
// ---------------------------------------------------------------------------

pub struct AgentHandle {
    client: MaestroClient,
    agent_id: String,
}

impl AgentHandle {
    pub fn agent_id(&self) -> &str { &self.agent_id }

    pub async fn info(&self) -> SdkResult<AgentInfo> {
        self.client.get_agent(&self.agent_id).await
    }

    pub async fn start_session(&self) -> SdkResult<SessionHandle> {
        let session = self.client.create_session(CreateSessionRequest {
            agent_id: self.agent_id.clone(),
            initial_context: None,
            metadata: Default::default(),
        }).await?;
        Ok(SessionHandle { client: self.client.clone(), session })
    }

    pub async fn start_session_with_context(&self, context: &str) -> SdkResult<SessionHandle> {
        let session = self.client.create_session(CreateSessionRequest {
            agent_id: self.agent_id.clone(),
            initial_context: Some(context.to_string()),
            metadata: Default::default(),
        }).await?;
        Ok(SessionHandle { client: self.client.clone(), session })
    }
}

// ---------------------------------------------------------------------------
// SessionHandle
// ---------------------------------------------------------------------------

pub struct SessionHandle {
    client: MaestroClient,
    session: Session,
}

impl SessionHandle {
    pub fn session_id(&self) -> Uuid { self.session.id }
    pub fn session(&self) -> &Session { &self.session }

    pub async fn chat(&self, message: &str) -> SdkResult<String> {
        let resp = self.client.send_message(self.session.id, SendMessageRequest {
            content: message.to_string(),
            system_prompt: None,
            max_tokens: None,
            temperature: None,
        }).await?;
        Ok(resp.message.content)
    }

    pub async fn chat_with_config(&self, req: SendMessageRequest) -> SdkResult<SendMessageResponse> {
        self.client.send_message(self.session.id, req).await
    }

    pub async fn history(&self) -> SdkResult<Vec<Message>> {
        self.client.get_messages(self.session.id).await
    }

    pub async fn traces(&self) -> SdkResult<Vec<TraceInfo>> {
        self.client.get_traces(self.session.id).await
    }

    pub async fn end(self) -> SdkResult<()> {
        self.client.delete_session(self.session.id).await
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct MaestroClientBuilder {
    base_url: Option<String>,
    api_key: Option<String>,
    timeout_secs: Option<u64>,
}

impl MaestroClientBuilder {
    pub fn new() -> Self { Self::default() }

    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = Some(url.into()); self
    }

    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into()); self
    }

    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs); self
    }

    pub fn build(self) -> anyhow::Result<MaestroClient> {
        let base_url = self.base_url.context("base_url is required")?;
        Ok(MaestroClient::new(ClientConfig {
            base_url,
            api_key: self.api_key.unwrap_or_default(),
            timeout_secs: self.timeout_secs.unwrap_or(30),
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_builder() {
        let client = MaestroClientBuilder::new()
            .base_url("http://localhost:3000")
            .api_key("test-key")
            .timeout_secs(60)
            .build()
            .unwrap();
        assert_eq!(client.config.base_url, "http://localhost:3000");
        assert_eq!(client.config.timeout_secs, 60);
    }

    #[test]
    fn test_url_construction() {
        let client = MaestroClient::new(ClientConfig {
            base_url: "http://localhost:3000".to_string(),
            api_key: "key".to_string(),
            timeout_secs: 30,
        });
        assert_eq!(client.url("/api/agents"), "http://localhost:3000/api/agents");
    }

    #[test]
    fn test_url_trailing_slash() {
        let client = MaestroClient::new(ClientConfig {
            base_url: "http://localhost:3000/".to_string(),
            api_key: "key".to_string(),
            timeout_secs: 30,
        });
        assert_eq!(client.url("/api/agents"), "http://localhost:3000/api/agents");
    }

    #[test]
    fn test_send_message_request_serialization() {
        let req = SendMessageRequest {
            content: "Hello".to_string(),
            system_prompt: None,
            max_tokens: Some(1000),
            temperature: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("Hello"));
        assert!(!json.contains("system_prompt")); // skip_serializing_if
        assert!(json.contains("max_tokens"));
    }

    #[test]
    fn test_agent_status_deserialization() {
        let s: AgentStatus = serde_json::from_str(r#""active""#).unwrap();
        assert_eq!(s, AgentStatus::Active);
    }

    #[test]
    fn test_agent_handle() {
        let client = MaestroClient::new(ClientConfig::default());
        let handle = client.agent("my-agent");
        assert_eq!(handle.agent_id(), "my-agent");
    }
}
