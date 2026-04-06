//! Qdrant-backed semantic store for vector similarity search.
//!
//! Uses the Qdrant gRPC client to store and search memory embeddings.
//! Non-vector metadata (content, source, scope, etc.) is stored as payload.
//!
//! Enable with `cargo build --features qdrant`.

use crate::helpers;
use openfang_types::agent::AgentId;
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::memory::{MemoryFilter, MemoryFragment, MemoryId, MemorySource};
use openfang_types::storage::SemanticBackend;
use qdrant_client::qdrant::{
    point_id::PointIdOptions, Condition, CreateCollectionBuilder, DeletePointsBuilder, Distance,
    Filter, PointId, PointStruct, SearchPointsBuilder, UpsertPointsBuilder, VectorParamsBuilder,
};
use qdrant_client::Qdrant;
use std::collections::HashMap;
use tracing::info;

/// Extract a string from a Qdrant payload Value.
fn payload_str<'a>(
    payload: &'a HashMap<String, qdrant_client::qdrant::Value>,
    key: &str,
) -> Option<&'a str> {
    payload.get(key).and_then(|v| {
        if let Some(qdrant_client::qdrant::value::Kind::StringValue(s)) = &v.kind {
            Some(s.as_str())
        } else {
            None
        }
    })
}

/// Extract a double from a Qdrant payload Value.
fn payload_double(
    payload: &HashMap<String, qdrant_client::qdrant::Value>,
    key: &str,
) -> Option<f64> {
    payload.get(key).and_then(|v| {
        if let Some(qdrant_client::qdrant::value::Kind::DoubleValue(d)) = &v.kind {
            Some(*d)
        } else {
            None
        }
    })
}

/// Extract an integer from a Qdrant payload Value.
fn payload_int(
    payload: &HashMap<String, qdrant_client::qdrant::Value>,
    key: &str,
) -> Option<i64> {
    payload.get(key).and_then(|v| {
        if let Some(qdrant_client::qdrant::value::Kind::IntegerValue(i)) = &v.kind {
            Some(*i)
        } else {
            None
        }
    })
}

/// Qdrant-backed semantic store.
pub struct QdrantSemanticStore {
    client: Qdrant,
    collection: String,
    /// Embedding dimensions (detected from first insert, then cached).
    dims: std::sync::Mutex<Option<u64>>,
}

impl QdrantSemanticStore {
    /// Create a new Qdrant semantic store.
    ///
    /// `url` is the Qdrant gRPC endpoint (e.g., `http://localhost:6334`).
    /// `api_key` is optional for authenticated deployments.
    /// `collection` is the Qdrant collection name.
    pub fn new(url: &str, api_key: Option<&str>, collection: &str) -> OpenFangResult<Self> {
        let mut builder = Qdrant::from_url(url);
        if let Some(key) = api_key {
            builder = builder.api_key(key);
        }
        let client = builder
            .build()
            .map_err(|e| OpenFangError::Memory(format!("Failed to create Qdrant client: {e}")))?;

        Ok(Self {
            client,
            collection: collection.to_string(),
            dims: std::sync::Mutex::new(None),
        })
    }

    /// Ensure the collection exists, creating it if needed.
    fn ensure_collection(&self, dims: u64) -> OpenFangResult<()> {
        {
            let mut cached = self
                .dims
                .lock()
                .map_err(|e| OpenFangError::Memory(e.to_string()))?;
            if *cached == Some(dims) {
                return Ok(());
            }
            *cached = Some(dims);
        }

        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(async {
            let exists = self
                .client
                .collection_exists(&self.collection)
                .await
                .map_err(|e| {
                    OpenFangError::Memory(format!("Qdrant collection check failed: {e}"))
                })?;

            if !exists {
                self.client
                    .create_collection(
                        CreateCollectionBuilder::new(&self.collection)
                            .vectors_config(VectorParamsBuilder::new(dims, Distance::Cosine)),
                    )
                    .await
                    .map_err(|e| {
                        OpenFangError::Memory(format!("Qdrant create collection failed: {e}"))
                    })?;
                info!(collection = %self.collection, dims, "Created Qdrant collection");
            }

            Ok(())
        }))
    }

    fn block_on<F, T>(&self, f: F) -> OpenFangResult<T>
    where
        F: std::future::Future<Output = OpenFangResult<T>>,
    {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(f)
        })
    }
}

impl SemanticBackend for QdrantSemanticStore {
    fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, serde_json::Value>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        let id = MemoryId::new();

        let embedding = match embedding {
            Some(e) => e,
            None => {
                return Err(OpenFangError::Memory(
                    "Qdrant backend requires embeddings for remember()".to_string(),
                ));
            }
        };

        self.ensure_collection(embedding.len() as u64)?;

        let source_str = helpers::serialize_source(&source)?;
        let meta_str = helpers::serialize_metadata(&metadata)?;
        let now = chrono::Utc::now().to_rfc3339();

        let payload: HashMap<String, qdrant_client::qdrant::Value> = HashMap::from([
            ("agent_id".into(), agent_id.0.to_string().into()),
            ("content".into(), content.to_string().into()),
            ("source".into(), source_str.into()),
            ("scope".into(), scope.to_string().into()),
            ("confidence".into(), (1.0f64).into()),
            ("metadata".into(), meta_str.into()),
            ("created_at".into(), now.clone().into()),
            ("accessed_at".into(), now.into()),
            ("access_count".into(), 0i64.into()),
        ]);

        let point = PointStruct::new(id.0.to_string(), embedding.to_vec(), payload);

        self.block_on(async {
            self.client
                .upsert_points(UpsertPointsBuilder::new(&self.collection, vec![point]))
                .await
                .map_err(|e| OpenFangError::Memory(format!("Qdrant upsert failed: {e}")))?;
            Ok(id)
        })
    }

    fn recall(
        &self,
        _query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        let embedding = match query_embedding {
            Some(e) => e.to_vec(),
            None => return Ok(vec![]),
        };

        let mut conditions = Vec::new();
        if let Some(ref f) = filter {
            if let Some(agent_id) = f.agent_id {
                conditions.push(Condition::matches("agent_id", agent_id.0.to_string()));
            }
            if let Some(ref scope) = f.scope {
                conditions.push(Condition::matches("scope", scope.clone()));
            }
        }

        let qdrant_filter = if conditions.is_empty() {
            None
        } else {
            Some(Filter::must(conditions))
        };

        self.block_on(async {
            let mut search =
                SearchPointsBuilder::new(&self.collection, embedding, limit as u64)
                    .with_payload(true);
            if let Some(f) = qdrant_filter {
                search = search.filter(f);
            }

            let results = self
                .client
                .search_points(search)
                .await
                .map_err(|e| OpenFangError::Memory(format!("Qdrant search failed: {e}")))?;

            let mut fragments = Vec::new();
            for point in &results.result {
                let payload = &point.payload;

                // Extract UUID from PointId
                let id_str = point
                    .id
                    .as_ref()
                    .and_then(|pid| match &pid.point_id_options {
                        Some(PointIdOptions::Uuid(u)) => Some(u.clone()),
                        Some(PointIdOptions::Num(n)) => Some(n.to_string()),
                        None => None,
                    })
                    .unwrap_or_default();
                let id = helpers::parse_memory_id(&id_str)
                    .unwrap_or_else(|_| MemoryId::new());

                let agent_str = payload_str(payload, "agent_id").unwrap_or("");
                let agent_id = helpers::parse_agent_id(agent_str)
                    .unwrap_or_else(|_| AgentId::new());

                let content = payload_str(payload, "content")
                    .unwrap_or("")
                    .to_string();
                let source_str = payload_str(payload, "source").unwrap_or("\"System\"");
                let source: MemorySource = helpers::deserialize_source(source_str);
                let scope = payload_str(payload, "scope")
                    .unwrap_or("episodic")
                    .to_string();
                let confidence = payload_double(payload, "confidence").unwrap_or(1.0) as f32;
                let meta_str = payload_str(payload, "metadata").unwrap_or("{}");
                let metadata: HashMap<String, serde_json::Value> =
                    helpers::deserialize_metadata(meta_str);
                let created_at = payload_str(payload, "created_at")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);
                let accessed_at = payload_str(payload, "accessed_at")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(chrono::Utc::now);
                let access_count = payload_int(payload, "access_count").unwrap_or(0) as u64;

                fragments.push(MemoryFragment {
                    id,
                    agent_id,
                    content,
                    embedding: None,
                    metadata,
                    source,
                    confidence,
                    created_at,
                    accessed_at,
                    access_count,
                    scope,
                });
            }

            Ok(fragments)
        })
    }

    fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        self.block_on(async {
            let point_id = PointId {
                point_id_options: Some(PointIdOptions::Uuid(id.0.to_string())),
            };
            self.client
                .delete_points(
                    DeletePointsBuilder::new(&self.collection).points(vec![point_id]),
                )
                .await
                .map_err(|e| OpenFangError::Memory(format!("Qdrant delete failed: {e}")))?;
            Ok(())
        })
    }

    fn update_embedding(&self, id: MemoryId, embedding: &[f32]) -> OpenFangResult<()> {
        self.ensure_collection(embedding.len() as u64)?;

        self.block_on(async {
            let point = PointStruct::new(
                id.0.to_string(),
                embedding.to_vec(),
                HashMap::<String, qdrant_client::qdrant::Value>::new(),
            );
            self.client
                .upsert_points(UpsertPointsBuilder::new(&self.collection, vec![point]))
                .await
                .map_err(|e| {
                    OpenFangError::Memory(format!("Qdrant update embedding failed: {e}"))
                })?;
            Ok(())
        })
    }
}
