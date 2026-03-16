//! MongoDB backend for the memory substrate.
//!
//! Provides the same functionality as the SQLite backend but backed by MongoDB.
//! Each store maps to one or more MongoDB collections.

pub mod consolidation;
pub mod indexes;
pub mod knowledge;
pub mod semantic;
pub mod session;
pub mod structured;
pub mod usage;

use bson::doc;
use chrono::Utc;
use futures::TryStreamExt;
use mongodb::Collection;
use openfang_types::error::{OpenFangError, OpenFangResult};

use self::consolidation::MongoConsolidationEngine;
use self::knowledge::MongoKnowledgeStore;
use self::semantic::MongoSemanticStore;
use self::session::MongoSessionStore;
use self::structured::MongoStructuredStore;
use self::usage::MongoUsageStore;

/// Composed MongoDB backend holding all store handles.
#[derive(Clone)]
pub struct MongoBackend {
    /// The underlying MongoDB database handle.
    pub db: mongodb::Database,
    /// KV + agent persistence.
    pub structured: MongoStructuredStore,
    /// Semantic memory with embeddings.
    pub semantic: MongoSemanticStore,
    /// Knowledge graph (entities + relations).
    pub knowledge: MongoKnowledgeStore,
    /// Session management.
    pub sessions: MongoSessionStore,
    /// Usage / cost tracking.
    pub usage: MongoUsageStore,
    /// Memory decay engine.
    pub consolidation: MongoConsolidationEngine,
    /// Paired devices collection.
    paired_devices: Collection<bson::Document>,
    /// Task queue collection.
    task_queue: Collection<bson::Document>,
}

impl MongoBackend {
    /// Create a new MongoDB backend from a database handle.
    pub fn new(db: mongodb::Database, decay_rate: f32) -> Self {
        Self {
            structured: MongoStructuredStore::new(db.clone()),
            semantic: MongoSemanticStore::new(db.clone()),
            knowledge: MongoKnowledgeStore::new(db.clone()),
            sessions: MongoSessionStore::new(db.clone()),
            usage: MongoUsageStore::new(db.clone()),
            consolidation: MongoConsolidationEngine::new(db.clone(), decay_rate),
            paired_devices: db.collection("paired_devices"),
            task_queue: db.collection("task_queue"),
            db,
        }
    }

    // -----------------------------------------------------------------
    // Paired devices
    // -----------------------------------------------------------------

    pub async fn load_paired_devices(&self) -> OpenFangResult<Vec<serde_json::Value>> {
        let mut cursor = self
            .paired_devices
            .find(doc! {})
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut devices = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            devices.push(serde_json::json!({
                "device_id": d.get_str("_id").unwrap_or_default(),
                "display_name": d.get_str("display_name").unwrap_or_default(),
                "platform": d.get_str("platform").unwrap_or_default(),
                "paired_at": d.get_str("paired_at").unwrap_or_default(),
                "last_seen": d.get_str("last_seen").unwrap_or_default(),
                "push_token": d.get_str("push_token").ok(),
            }));
        }
        Ok(devices)
    }

    pub async fn save_paired_device(
        &self,
        device_id: &str,
        display_name: &str,
        platform: &str,
        paired_at: &str,
        last_seen: &str,
        push_token: Option<&str>,
    ) -> OpenFangResult<()> {
        let filter = doc! { "_id": device_id };
        let update = doc! {
            "$set": {
                "display_name": display_name,
                "platform": platform,
                "paired_at": paired_at,
                "last_seen": last_seen,
                "push_token": push_token,
            }
        };
        self.paired_devices
            .update_one(filter, update)
            .upsert(true)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    pub async fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()> {
        self.paired_devices
            .delete_one(doc! { "_id": device_id })
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    // -----------------------------------------------------------------
    // Task queue
    // -----------------------------------------------------------------

    pub async fn task_post(
        &self,
        title: &str,
        description: &str,
        assigned_to: Option<&str>,
        created_by: Option<&str>,
    ) -> OpenFangResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = bson::DateTime::from_chrono(Utc::now());
        let doc = doc! {
            "_id": &id,
            "agent_id": created_by.unwrap_or(""),
            "task_type": title,
            "payload": bson::Binary { subtype: bson::spec::BinarySubtype::Generic, bytes: vec![] },
            "status": "pending",
            "priority": 0_i32,
            "created_at": now,
            "title": title,
            "description": description,
            "assigned_to": assigned_to.unwrap_or(""),
            "created_by": created_by.unwrap_or(""),
        };
        self.task_queue
            .insert_one(doc)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(id)
    }

    pub async fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<serde_json::Value>> {
        let filter = doc! {
            "status": "pending",
            "$or": [
                { "assigned_to": agent_id },
                { "assigned_to": "" },
            ]
        };
        let opts = mongodb::options::FindOneOptions::builder()
            .sort(doc! { "priority": -1, "created_at": 1 })
            .build();
        let doc = self
            .task_queue
            .find_one(filter)
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        match doc {
            Some(d) => {
                let id = d.get_str("_id").unwrap_or_default().to_string();
                let title = d.get_str("title").unwrap_or_default().to_string();
                let description = d.get_str("description").unwrap_or_default().to_string();
                let assigned = d.get_str("assigned_to").unwrap_or_default().to_string();
                let created_by = d.get_str("created_by").unwrap_or_default().to_string();
                let created_at = d
                    .get_datetime("created_at")
                    .ok()
                    .map(|dt| dt.to_chrono().to_rfc3339())
                    .unwrap_or_default();

                self.task_queue
                    .update_one(
                        doc! { "_id": &id },
                        doc! { "$set": { "status": "in_progress", "assigned_to": agent_id } },
                    )
                    .await
                    .map_err(|e| OpenFangError::Memory(e.to_string()))?;

                Ok(Some(serde_json::json!({
                    "id": id,
                    "title": title,
                    "description": description,
                    "status": "in_progress",
                    "assigned_to": if assigned.is_empty() { agent_id } else { &assigned },
                    "created_by": created_by,
                    "created_at": created_at,
                })))
            }
            None => Ok(None),
        }
    }

    pub async fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()> {
        let now = bson::DateTime::from_chrono(Utc::now());
        let res = self
            .task_queue
            .update_one(
                doc! { "_id": task_id },
                doc! { "$set": { "status": "completed", "result": result, "completed_at": now } },
            )
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        if res.matched_count == 0 {
            return Err(OpenFangError::Internal(format!("Task not found: {task_id}")));
        }
        Ok(())
    }

    pub async fn task_list(
        &self,
        status: Option<&str>,
    ) -> OpenFangResult<Vec<serde_json::Value>> {
        let filter = match status {
            Some(s) => doc! { "status": s },
            None => doc! {},
        };
        let opts = mongodb::options::FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .build();
        let mut cursor = self
            .task_queue
            .find(filter)
            .with_options(opts)
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut tasks = Vec::new();
        while let Some(d) = cursor
            .try_next()
            .await
            .map_err(|e| OpenFangError::Memory(e.to_string()))?
        {
            let created_at = d
                .get_datetime("created_at")
                .ok()
                .map(|dt| dt.to_chrono().to_rfc3339())
                .unwrap_or_default();
            let completed_at = d
                .get_datetime("completed_at")
                .ok()
                .map(|dt| dt.to_chrono().to_rfc3339());

            tasks.push(serde_json::json!({
                "id": d.get_str("_id").unwrap_or_default(),
                "title": d.get_str("title").unwrap_or_default(),
                "description": d.get_str("description").unwrap_or_default(),
                "status": d.get_str("status").unwrap_or_default(),
                "assigned_to": d.get_str("assigned_to").unwrap_or_default(),
                "created_by": d.get_str("created_by").unwrap_or_default(),
                "created_at": created_at,
                "completed_at": completed_at,
                "result": d.get_str("result").ok(),
            }));
        }
        Ok(tasks)
    }
}
