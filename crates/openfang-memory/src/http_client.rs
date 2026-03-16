//! HTTP client for the memory-api gateway.
//!
//! Provides a blocking HTTP client that routes `remember` and `recall` operations
//! to the shared memory-api service (PostgreSQL + pgvector + Jina AI embeddings).
//! Designed to be called from synchronous SemanticStore methods within
//! `spawn_blocking` contexts.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Error type for memory API operations.
#[derive(Debug, thiserror::Error)]
pub enum MemoryApiError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("API error (status {status}): {message}")]
    Api { status: u16, message: String },
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Missing config: {0}")]
    Config(String),
}

/// HTTP client for the memory-api gateway service.
#[derive(Clone)]
pub struct MemoryApiClient {
    base_url: String,
    token: String,
    client: reqwest::blocking::Client,
}

// -- Request/Response types matching memory-api endpoints --

#[derive(Serialize)]
struct StoreRequest<'a> {
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<&'a str>,
    #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
    agent_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    importance: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
pub struct StoreResponse {
    pub id: serde_json::Value,
    #[serde(default)]
    pub deduplicated: bool,
}

#[derive(Serialize)]
struct SearchRequest<'a> {
    query: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<&'a str>,
}

#[derive(Deserialize, Debug)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub count: usize,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SearchResult {
    pub id: serde_json::Value,
    pub content: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub score: f64,
    #[serde(rename = "createdAt", default)]
    pub created_at: Option<f64>,
}

#[derive(Deserialize, Debug)]
struct HealthResponse {
    pub status: String,
}

impl MemoryApiClient {
    /// Create a new memory-api HTTP client.
    ///
    /// `base_url`: The base URL of the memory-api service (e.g., "http://127.0.0.1:5500").
    /// `token_env`: The name of the environment variable holding the bearer token.
    pub fn new(base_url: &str, token_env: &str) -> Result<Self, MemoryApiError> {
        let token = if token_env.is_empty() {
            String::new()
        } else {
            std::env::var(token_env).unwrap_or_else(|_| {
                warn!(env = token_env, "Memory API token env var not set");
                String::new()
            })
        };

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("openfang-memory/0.4")
            .build()
            .map_err(|e| MemoryApiError::Http(e.to_string()))?;

        let base_url = base_url.trim_end_matches('/').to_string();

        Ok(Self {
            base_url,
            token,
            client,
        })
    }

    /// Check if memory-api is reachable.
    pub fn health_check(&self) -> Result<(), MemoryApiError> {
        let url = format!("{}/health", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .map_err(|e| MemoryApiError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(MemoryApiError::Api {
                status: resp.status().as_u16(),
                message: resp.text().unwrap_or_default(),
            });
        }

        let body: HealthResponse = resp
            .json()
            .map_err(|e| MemoryApiError::Parse(e.to_string()))?;

        if body.status != "ok" {
            return Err(MemoryApiError::Api {
                status: 503,
                message: format!("memory-api status: {}", body.status),
            });
        }

        debug!("memory-api health check passed");
        Ok(())
    }

    /// Store a memory via POST /memory/store.
    ///
    /// The memory-api handles embedding generation (Jina AI) and deduplication.
    pub fn store(
        &self,
        content: &str,
        category: Option<&str>,
        agent_id: Option<&str>,
        source: Option<&str>,
        importance: Option<u8>,
        tags: Option<Vec<String>>,
    ) -> Result<StoreResponse, MemoryApiError> {
        let url = format!("{}/memory/store", self.base_url);

        let body = StoreRequest {
            content,
            category,
            agent_id,
            source,
            importance,
            tags,
        };

        let mut req = self.client.post(&url).json(&body);
        if !self.token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.token));
        }

        let resp = req
            .send()
            .map_err(|e| MemoryApiError::Http(e.to_string()))?;
        let status = resp.status().as_u16();

        if status != 200 && status != 201 {
            let body_text = resp.text().unwrap_or_default();
            return Err(MemoryApiError::Api {
                status,
                message: body_text,
            });
        }

        let result: StoreResponse = resp
            .json()
            .map_err(|e| MemoryApiError::Parse(e.to_string()))?;

        debug!(
            id = %result.id,
            deduplicated = result.deduplicated,
            "Stored memory via HTTP"
        );

        Ok(result)
    }

    /// Search memories via POST /memory/search.
    ///
    /// The memory-api handles embedding the query (Jina AI) and hybrid vector+BM25 search.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
        category: Option<&str>,
    ) -> Result<Vec<SearchResult>, MemoryApiError> {
        let url = format!("{}/memory/search", self.base_url);

        let body = SearchRequest {
            query,
            limit: Some(limit),
            category,
        };

        let mut req = self.client.post(&url).json(&body);
        if !self.token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", self.token));
        }

        let resp = req
            .send()
            .map_err(|e| MemoryApiError::Http(e.to_string()))?;
        let status = resp.status().as_u16();

        if status != 200 {
            let body_text = resp.text().unwrap_or_default();
            return Err(MemoryApiError::Api {
                status,
                message: body_text,
            });
        }

        let result: SearchResponse = resp
            .json()
            .map_err(|e| MemoryApiError::Parse(e.to_string()))?;

        debug!(count = result.count, "Searched memories via HTTP");

        Ok(result.results)
    }
}
