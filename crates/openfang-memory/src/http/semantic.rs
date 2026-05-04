//! HTTP client for the memory-api gateway.
//!
//! Provides a blocking HTTP client that routes `remember` and `recall` operations
//! to the shared memory-api service (PostgreSQL + pgvector + Jina AI embeddings).
//! Designed to be called from synchronous SemanticStore methods within
//! `spawn_blocking` contexts.

use chrono::Utc;
use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{MemoryFilter, MemoryFragment, MemoryId, MemorySource};
use openfang_types::storage::SemanticBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
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

/// Parse a `serde_json::Value` returned by the memory-api into a `MemoryId`.
/// The server is expected to emit IDs as UUID strings; any other shape is a
/// protocol mismatch and is surfaced as an error rather than fabricating an ID.
fn parse_memory_id(v: &serde_json::Value) -> OpenFangResult<MemoryId> {
    let s = v.as_str().ok_or_else(|| {
        OpenFangError::Memory(format!(
            "memory-api returned non-string id; expected UUID string, got: {v}"
        ))
    })?;
    let uuid = uuid::Uuid::parse_str(s).map_err(|e| {
        OpenFangError::Memory(format!("memory-api returned invalid UUID id {s:?}: {e}"))
    })?;
    Ok(MemoryId(uuid))
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

/// HTTP semantic backend that routes remember/recall to a remote memory-api gateway.
/// Falls back to a local backend for operations the API doesn't support (forget,
/// update_embedding) and on HTTP errors.
pub struct HttpSemanticStore {
    client: MemoryApiClient,
    fallback: Arc<dyn SemanticBackend>,
}

impl HttpSemanticStore {
    pub fn new(client: MemoryApiClient, fallback: Arc<dyn SemanticBackend>) -> Self {
        Self { client, fallback }
    }
}

impl SemanticBackend for HttpSemanticStore {
    fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        let source_str = format!("{:?}", source).to_lowercase();
        let importance = metadata
            .get("importance")
            .and_then(|v| v.as_u64())
            .map(|v| v.min(10) as u8)
            .unwrap_or(5);
        let tags: Option<Vec<String>> = metadata
            .get("tags")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        match self.client.store(
            content,
            Some(scope),
            Some(&agent_id.0.to_string()),
            Some(&source_str),
            Some(importance),
            tags,
        ) {
            Ok(resp) => match parse_memory_id(&resp.id) {
                Ok(id) => {
                    debug!(id = %id, "Stored memory via HTTP backend");
                    Ok(id)
                }
                Err(e) => {
                    warn!(error = %e, "memory-api returned malformed id, falling back to local");
                    self.fallback
                        .remember(agent_id, content, source, scope, metadata, embedding)
                }
            },
            Err(e) => {
                warn!(error = %e, "HTTP memory store failed, falling back to local");
                self.fallback
                    .remember(agent_id, content, source, scope, metadata, embedding)
            }
        }
    }

    fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        let category: Option<String> = filter.as_ref().and_then(|f| f.scope.clone());

        match self
            .client
            .search(query, limit, category.as_deref())
            .map_err(|e| OpenFangError::Memory(format!("HTTP search failed: {e}")))
        {
            Ok(results) => {
                let mut fragments: Vec<MemoryFragment> = Vec::with_capacity(results.len());
                for r in results {
                    let id = match parse_memory_id(&r.id) {
                        Ok(id) => id,
                        Err(e) => {
                            warn!(error = %e, "dropping memory-api result with malformed id");
                            continue;
                        }
                    };
                    let created_at = r
                        .created_at
                        .map(|ms| {
                            chrono::DateTime::from_timestamp_millis(ms as i64)
                                .unwrap_or_else(Utc::now)
                        })
                        .unwrap_or_else(Utc::now);

                    fragments.push(MemoryFragment {
                        id,
                        agent_id: filter
                            .as_ref()
                            .and_then(|f| f.agent_id)
                            .unwrap_or_default(),
                        content: r.content,
                        embedding: None,
                        metadata: HashMap::new(),
                        source: MemorySource::System,
                        confidence: r.score as f32,
                        created_at,
                        accessed_at: Utc::now(),
                        access_count: 0,
                        scope: r.category.unwrap_or_else(|| "general".to_string()),
                    });
                }

                debug!(
                    count = fragments.len(),
                    "Recalled memories via HTTP backend"
                );
                Ok(fragments)
            }
            Err(e) => {
                warn!(error = %e, "HTTP memory search failed, falling back to local");
                self.fallback.recall(query, limit, filter, query_embedding)
            }
        }
    }

    fn forget(&self, _id: MemoryId) -> OpenFangResult<()> {
        // The HTTP memory-api does not expose a delete endpoint, and the local
        // fallback never saw the row (it was written remotely), so delegating
        // there would silently no-op against the wrong store.
        Err(OpenFangError::Memory(
            "HTTP semantic backend does not support forget(); \
             configure a local semantic_backend to use this operation"
                .into(),
        ))
    }

    fn update_embedding(&self, _id: MemoryId, _embedding: &[f32]) -> OpenFangResult<()> {
        // memory-api owns embedding generation server-side; there is no client
        // path to override it, and the local fallback does not hold this row.
        Err(OpenFangError::Memory(
            "HTTP semantic backend does not support update_embedding(); \
             the remote service manages embeddings"
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_memory_id_accepts_valid_uuid_string() {
        let raw = "550e8400-e29b-41d4-a716-446655440000";
        let id = parse_memory_id(&serde_json::json!(raw)).expect("valid uuid parses");
        assert_eq!(id.0, uuid::Uuid::parse_str(raw).unwrap());
    }

    #[test]
    fn parse_memory_id_rejects_non_string_value() {
        let err = parse_memory_id(&serde_json::json!(42)).expect_err("non-string must error");
        match err {
            OpenFangError::Memory(msg) => {
                assert!(msg.contains("memory-api"), "msg={msg}");
                assert!(msg.contains("non-string"), "msg={msg}");
            }
            other => panic!("expected OpenFangError::Memory, got {other:?}"),
        }
    }

    #[test]
    fn parse_memory_id_rejects_invalid_uuid_string() {
        let err =
            parse_memory_id(&serde_json::json!("not-a-uuid")).expect_err("bad uuid must error");
        match err {
            OpenFangError::Memory(msg) => {
                assert!(msg.contains("memory-api"), "msg={msg}");
                assert!(msg.contains("invalid UUID"), "msg={msg}");
            }
            other => panic!("expected OpenFangError::Memory, got {other:?}"),
        }
    }
}
