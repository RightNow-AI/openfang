//! MongoDB structured store for key-value pairs and agent persistence.

use bson::doc;
use chrono::Utc;
use futures::TryStreamExt;
use mongodb::Collection;
use openfang_types::agent::{AgentEntry, AgentId};
use openfang_types::error::{OpenFangError, OpenFangResult};

/// Structured store backed by MongoDB for key-value operations and agent storage.
#[derive(Clone)]
pub struct MongoStructuredStore {
    kv: Collection<bson::Document>,
    agents: Collection<bson::Document>,
}

impl MongoStructuredStore {
    pub fn new(db: mongodb::Database) -> Self {
        Self {
            kv: db.collection("kv_store"),
            agents: db.collection("agents"),
        }
    }

    pub async fn get(
        &self,
        agent_id: AgentId,
        key: &str,
    ) -> OpenFangResult<Option<serde_json::Value>> {
        let filter = doc! { "agent_id": agent_id.0.to_string(), "key": key };
        let doc = self
            .kv
            .find_one(filter)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        match doc {
            Some(d) => {
                let bson_val = d
                    .get("value")
                    .ok_or_else(|| OpenFangError::Memory("Missing value field".into()))?;
                let json_val: serde_json::Value = bson::from_bson(bson_val.clone())
                    .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
                Ok(Some(json_val))
            }
            None => Ok(None),
        }
    }

    pub async fn set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: serde_json::Value,
    ) -> OpenFangResult<()> {
        let bson_val =
            bson::to_bson(&value).map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = bson::DateTime::from_chrono(Utc::now());
        let filter = doc! { "agent_id": agent_id.0.to_string(), "key": key };
        let update = doc! {
            "$set": {
                "agent_id": agent_id.0.to_string(),
                "key": key,
                "value": bson_val,
                "updated_at": now,
            },
            "$inc": { "version": 1_i32 },
        };
        self.kv
            .update_one(filter, update)
            .upsert(true)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub async fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        let filter = doc! { "agent_id": agent_id.0.to_string(), "key": key };
        self.kv
            .delete_one(filter)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub async fn list_kv(
        &self,
        agent_id: AgentId,
    ) -> OpenFangResult<Vec<(String, serde_json::Value)>> {
        let filter = doc! { "agent_id": agent_id.0.to_string() };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "key": 1 })
            .build();
        let mut cursor = self
            .kv
            .find(filter)
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut pairs = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            let key = d.get_str("key").unwrap_or_default().to_string();
            let value: serde_json::Value = d
                .get("value")
                .and_then(|v| bson::from_bson(v.clone()).ok())
                .unwrap_or(serde_json::Value::Null);
            pairs.push((key, value));
        }
        Ok(pairs)
    }

    pub async fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()> {
        let manifest_blob = rmp_serde::to_vec_named(&entry.manifest)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let state_str = serde_json::to_string(&entry.state)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let identity_json = serde_json::to_string(&entry.identity)
            .map_err(|e| OpenFangError::Serialization(e.to_string()))?;
        let now = bson::DateTime::from_chrono(Utc::now());

        let filter = doc! { "_id": entry.id.0.to_string() };
        let update = doc! {
            "$set": {
                "name": &entry.name,
                "manifest": bson::Binary { subtype: bson::spec::BinarySubtype::Generic, bytes: manifest_blob },
                "state": &state_str,
                "updated_at": now,
                "session_id": entry.session_id.0.to_string(),
                "identity": &identity_json,
            },
            "$setOnInsert": {
                "created_at": bson::DateTime::from_chrono(entry.created_at),
            },
        };
        self.agents
            .update_one(filter, update)
            .upsert(true)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub async fn load_agent(&self, agent_id: AgentId) -> OpenFangResult<Option<AgentEntry>> {
        let filter = doc! { "_id": agent_id.0.to_string() };
        let doc = self
            .agents
            .find_one(filter)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        match doc {
            Some(d) => parse_agent_doc(agent_id, &d),
            None => Ok(None),
        }
    }

    pub async fn remove_agent(&self, agent_id: AgentId) -> OpenFangResult<()> {
        let filter = doc! { "_id": agent_id.0.to_string() };
        self.agents
            .delete_one(filter)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub async fn load_all_agents(&self) -> OpenFangResult<Vec<AgentEntry>> {
        let mut cursor = self
            .agents
            .find(doc! {})
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut agents = Vec::new();
        let mut seen_names = std::collections::HashSet::new();

        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            let id_str = d.get_str("_id").unwrap_or_default();
            let name = d.get_str("name").unwrap_or_default();
            let name_lower = name.to_lowercase();
            if !seen_names.insert(name_lower) {
                tracing::info!(agent = %name, id = %id_str, "Skipping duplicate agent name");
                continue;
            }

            let agent_id =
                match uuid::Uuid::parse_str(id_str).map(openfang_types::agent::AgentId) {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::warn!(agent = %name, "Skipping agent with bad UUID '{id_str}': {e}");
                        continue;
                    }
                };

            match parse_agent_doc(agent_id, &d) {
                Ok(Some(entry)) => agents.push(entry),
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(agent = %name, id = %id_str, "Skipping agent: {e}");
                    continue;
                }
            }
        }
        Ok(agents)
    }

    pub async fn list_agents(&self) -> OpenFangResult<Vec<(String, String, String)>> {
        let opts = mongodb::options::FindOptions::builder()
            .projection(doc! { "_id": 1, "name": 1, "state": 1 })
            .build();
        let mut cursor = self
            .agents
            .find(doc! {})
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut agents = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            let id = d.get_str("_id").unwrap_or_default().to_string();
            let name = d.get_str("name").unwrap_or_default().to_string();
            let state = d.get_str("state").unwrap_or_default().to_string();
            agents.push((id, name, state));
        }
        Ok(agents)
    }
}

fn parse_agent_doc(
    agent_id: AgentId,
    d: &bson::Document,
) -> OpenFangResult<Option<AgentEntry>> {
    let name = d.get_str("name").unwrap_or_default().to_string();

    let manifest_binary = match d.get_binary_generic("manifest") {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => {
            tracing::warn!(agent = %name, "Skipping agent with missing manifest");
            return Ok(None);
        }
    };
    let manifest: openfang_types::agent::AgentManifest = match rmp_serde::from_slice(&manifest_binary) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(
                agent = %name,
                "Skipping agent with incompatible manifest: {e}"
            );
            return Ok(None);
        }
    };

    let state_str = d.get_str("state").unwrap_or("\"created\"");
    let state: openfang_types::agent::AgentState =
        serde_json::from_str(state_str).unwrap_or(openfang_types::agent::AgentState::Created);

    let created_at = d
        .get_datetime("created_at")
        .ok()
        .map(|dt| dt.to_chrono())
        .unwrap_or_else(Utc::now);

    let session_id_str = d.get_str("session_id").unwrap_or("");
    let session_id = uuid::Uuid::parse_str(session_id_str)
        .map(openfang_types::agent::SessionId)
        .unwrap_or_else(|_| openfang_types::agent::SessionId::new());

    let identity_str = d.get_str("identity").unwrap_or("{}");
    let identity = serde_json::from_str(identity_str).unwrap_or_default();

    Ok(Some(AgentEntry {
        id: agent_id,
        name,
        manifest,
        state,
        mode: Default::default(),
        created_at,
        last_active: Utc::now(),
        parent: None,
        children: vec![],
        session_id,
        tags: vec![],
        identity,
        onboarding_completed: false,
        onboarding_completed_at: None,
    }))
}
