//! MongoDB collection index setup.

use mongodb::Database;
use mongodb::IndexModel;
use mongodb::options::IndexOptions;
use bson::doc;
use openfang_types::error::{OpenFangError, OpenFangResult};

/// Ensure all collections exist and indexes are created.
pub async fn ensure_indexes(db: &Database) -> OpenFangResult<()> {
    // agents
    db.collection::<bson::Document>("agents")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "name": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (agents): {e}")))?;

    // kv_store
    db.collection::<bson::Document>("kv_store")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "agent_id": 1, "key": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (kv_store): {e}")))?;

    // sessions
    let sessions = db.collection::<bson::Document>("sessions");
    sessions
        .create_index(IndexModel::builder().keys(doc! { "agent_id": 1 }).build())
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (sessions): {e}")))?;
    sessions
        .create_index(
            IndexModel::builder()
                .keys(doc! { "agent_id": 1, "label": 1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (sessions label): {e}")))?;

    // memories
    let memories = db.collection::<bson::Document>("memories");
    memories
        .create_index(IndexModel::builder().keys(doc! { "agent_id": 1 }).build())
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (memories agent): {e}")))?;
    memories
        .create_index(
            IndexModel::builder()
                .keys(doc! { "deleted": 1, "scope": 1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (memories deleted): {e}")))?;
    memories
        .create_index(
            IndexModel::builder()
                .keys(doc! { "accessed_at": -1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (memories accessed): {e}")))?;

    // entities
    db.collection::<bson::Document>("entities")
        .create_index(IndexModel::builder().keys(doc! { "name": 1 }).build())
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (entities): {e}")))?;

    // relations
    let relations = db.collection::<bson::Document>("relations");
    relations
        .create_index(
            IndexModel::builder()
                .keys(doc! { "source_entity": 1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (relations src): {e}")))?;
    relations
        .create_index(
            IndexModel::builder()
                .keys(doc! { "target_entity": 1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (relations tgt): {e}")))?;
    relations
        .create_index(
            IndexModel::builder()
                .keys(doc! { "relation_type": 1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (relations type): {e}")))?;

    // usage_events
    let usage = db.collection::<bson::Document>("usage_events");
    usage
        .create_index(
            IndexModel::builder()
                .keys(doc! { "agent_id": 1, "timestamp": -1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (usage agent): {e}")))?;
    usage
        .create_index(
            IndexModel::builder()
                .keys(doc! { "timestamp": -1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (usage ts): {e}")))?;

    // task_queue
    db.collection::<bson::Document>("task_queue")
        .create_index(
            IndexModel::builder()
                .keys(doc! { "status": 1, "priority": -1, "created_at": 1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (task_queue): {e}")))?;

    // audit_entries
    let audit = db.collection::<bson::Document>("audit_entries");
    audit
        .create_index(
            IndexModel::builder()
                .keys(doc! { "seq": 1 })
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (audit seq): {e}")))?;
    audit
        .create_index(
            IndexModel::builder()
                .keys(doc! { "agent_id": 1 })
                .build(),
        )
        .await
        .map_err(|e| OpenFangError::Memory(format!("Index creation failed (audit agent): {e}")))?;

    Ok(())
}
