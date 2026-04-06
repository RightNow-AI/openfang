//! Integration tests for all storage backends.
//!
//! These tests verify the same operations work identically across:
//! - SQLite (always runs)
//! - PostgreSQL (requires `postgres` feature + running PG instance)
//! - Qdrant semantic store (requires `qdrant` feature + running Qdrant instance)
//!
//! Run with databases up:
//!   docker compose --profile db up -d postgres qdrant
//!   cargo test -p openfang-memory --features 'postgres,qdrant' --test backend_integration

use openfang_types::agent::AgentId;
use openfang_types::memory::MemorySource;
use openfang_types::storage::SemanticBackend;
use std::collections::HashMap;

fn agent_filter(agent_id: AgentId) -> openfang_types::memory::MemoryFilter {
    openfang_types::memory::MemoryFilter {
        agent_id: Some(agent_id),
        ..Default::default()
    }
}

// ─── SQLite backend tests ──────────────────────────────────────────────

mod sqlite {
    use super::*;
    use openfang_memory::backends::SessionBackend;
    use openfang_memory::sqlite::KnowledgeStore;
    use openfang_memory::sqlite::SemanticStore;
    use openfang_memory::sqlite::SessionStore;
    use openfang_memory::sqlite::StructuredStore;
    use openfang_memory::usage::{UsageRecord};
    use openfang_memory::sqlite::UsageStore;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn setup() -> Arc<Mutex<Connection>> {
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut rusqlite::ffi::sqlite3,
                    *mut *const u8,
                    *const rusqlite::ffi::sqlite3_api_routines,
                ) -> i32,
            >(sqlite_vec::sqlite3_vec_init as *const ())));
        }
        let conn = Connection::open_in_memory().unwrap();
        openfang_memory::sqlite::migration::run_migrations(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    #[test]
    fn structured_kv_crud() {
        let conn = setup();
        let store = StructuredStore::new(conn);
        let agent = AgentId::new();

        store.set(agent, "color", serde_json::json!("blue")).unwrap();
        assert_eq!(store.get(agent, "color").unwrap(), Some(serde_json::json!("blue")));

        store.set(agent, "color", serde_json::json!("red")).unwrap();
        assert_eq!(store.get(agent, "color").unwrap(), Some(serde_json::json!("red")));

        let pairs = store.list_kv(agent).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "color");

        store.delete(agent, "color").unwrap();
        assert_eq!(store.get(agent, "color").unwrap(), None);
        assert_eq!(store.get(agent, "nonexistent").unwrap(), None);
    }

    #[test]
    fn semantic_remember_recall_forget() {
        let conn = setup();
        let store = SemanticStore::new(conn);
        let agent = AgentId::new();

        let id = SemanticBackend::remember(
            &store, agent, "The quick brown fox jumps over the lazy dog",
            MemorySource::Conversation, "episodic", HashMap::new(), None,
        ).unwrap();

        let results = SemanticBackend::recall(&store, "quick brown fox", 10, None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);

        let results = SemanticBackend::recall(&store, "fox", 10, Some(agent_filter(AgentId::new())), None).unwrap();
        assert_eq!(results.len(), 0);

        SemanticBackend::forget(&store, id).unwrap();
        let results = SemanticBackend::recall(&store, "fox", 10, None, None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn semantic_with_embedding() {
        let conn = setup();
        let store = SemanticStore::new(conn);
        let agent = AgentId::new();

        let embedding = vec![0.1f32, 0.2, 0.3, 0.4];
        let id = SemanticBackend::remember(
            &store, agent, "vector test", MemorySource::System, "episodic",
            HashMap::new(), Some(&embedding),
        ).unwrap();

        let results = SemanticBackend::recall(&store, "", 10, None, Some(&embedding)).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);

        store.update_embedding(id, &[0.5, 0.6, 0.7, 0.8]).unwrap();
    }

    #[test]
    fn knowledge_entity_relation() {
        let conn = setup();
        let store = KnowledgeStore::new(conn);

        let alice_id = store.add_entity(openfang_types::memory::Entity {
            id: String::new(), entity_type: openfang_types::memory::EntityType::Person,
            name: "Alice".to_string(), properties: HashMap::new(),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }).unwrap();

        let acme_id = store.add_entity(openfang_types::memory::Entity {
            id: String::new(), entity_type: openfang_types::memory::EntityType::Organization,
            name: "Acme Corp".to_string(), properties: HashMap::new(),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }).unwrap();

        store.add_relation(openfang_types::memory::Relation {
            source: alice_id, relation: openfang_types::memory::RelationType::WorksAt,
            target: acme_id, properties: HashMap::new(), confidence: 0.9,
            created_at: chrono::Utc::now(),
        }).unwrap();

        let matches = store.query_graph(openfang_types::memory::GraphPattern {
            source: Some("Alice".to_string()), relation: None, target: None, max_depth: 1,
        }).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].source.name, "Alice");
        assert_eq!(matches[0].target.name, "Acme Corp");
    }

    #[test]
    fn session_crud() {
        let conn = setup();
        let store = SessionStore::new(conn);
        let agent = AgentId::new();

        let session = store.create_session(agent).unwrap();
        assert!(store.get_session(session.id).unwrap().is_some());

        let labeled = store.create_session_with_label(agent, Some("test-label")).unwrap();
        assert_eq!(labeled.label, Some("test-label".to_string()));

        let found = store.find_session_by_label(agent, "test-label").unwrap();
        assert_eq!(found.unwrap().id, labeled.id);

        assert_eq!(store.list_sessions().unwrap().len(), 2);

        store.delete_session(session.id).unwrap();
        assert!(store.get_session(session.id).unwrap().is_none());

        store.delete_agent_sessions(agent).unwrap();
        assert_eq!(store.list_agent_sessions(agent).unwrap().len(), 0);
    }

    #[test]
    fn usage_record_and_query() {
        let conn = setup();
        let store = UsageStore::new(conn);
        let agent = AgentId::new();

        store.record(&UsageRecord {
            agent_id: agent, model: "gpt-4".to_string(),
            input_tokens: 100, output_tokens: 50, cost_usd: 0.005, tool_calls: 2,
        }).unwrap();

        assert!(store.query_hourly(agent).unwrap() > 0.0);
        assert!(store.query_daily(agent).unwrap() > 0.0);
        assert!(store.query_monthly(agent).unwrap() > 0.0);
        assert!(store.query_global_hourly().unwrap() > 0.0);
        assert!(store.query_global_monthly().unwrap() > 0.0);

        let summary = store.query_summary(Some(agent)).unwrap();
        assert_eq!(summary.total_input_tokens, 100);
        assert_eq!(summary.total_output_tokens, 50);
        assert_eq!(summary.call_count, 1);

        assert!(!store.query_by_model().unwrap().is_empty());
        assert!(!store.query_daily_breakdown(7).unwrap().is_empty());
        assert!(store.query_today_cost().unwrap() > 0.0);
        assert!(store.query_first_event_date().unwrap().is_some());
    }

    #[test]
    fn canonical_session() {
        let conn = setup();
        let store = SessionStore::new(conn);
        let agent = AgentId::new();

        assert!(store.load_canonical(agent).unwrap().messages.is_empty());

        let msg = openfang_types::message::Message {
            role: openfang_types::message::Role::User,
            content: openfang_types::message::MessageContent::Text("hello".to_string()),
        };
        assert_eq!(store.append_canonical(agent, &[msg], None).unwrap().messages.len(), 1);

        let (summary, messages) = store.canonical_context(agent, None).unwrap();
        assert!(summary.is_none());
        assert_eq!(messages.len(), 1);

        store.store_llm_summary(agent, "User said hello", vec![]).unwrap();
        assert_eq!(store.canonical_context(agent, None).unwrap().0, Some("User said hello".to_string()));

        store.delete_canonical_session(agent).unwrap();
    }

    #[test]
    fn sqlite_paired_devices_crud() {
        let conn = setup();
        use openfang_memory::backends::PairedDevicesBackend;
        use openfang_memory::sqlite::SqlitePairedDevicesStore;
        let store = SqlitePairedDevicesStore::new(conn);

        // Save a device
        store.save_paired_device("dev-1", "iPhone", "ios", "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z", Some("token123")).unwrap();

        // Load — should find it
        let devices = store.load_paired_devices().unwrap();
        assert!(devices.iter().any(|d| d["device_id"] == "dev-1"));

        // Update (upsert)
        store.save_paired_device("dev-1", "iPhone Pro", "ios", "2025-01-01T00:00:00Z", "2025-06-01T00:00:00Z", None).unwrap();
        let devices = store.load_paired_devices().unwrap();
        let dev = devices.iter().find(|d| d["device_id"] == "dev-1").unwrap();
        assert_eq!(dev["display_name"], "iPhone Pro");

        // Remove
        store.remove_paired_device("dev-1").unwrap();
        let devices = store.load_paired_devices().unwrap();
        assert!(!devices.iter().any(|d| d["device_id"] == "dev-1"));
    }

    #[test]
    fn sqlite_task_queue_crud() {
        let conn = setup();
        use openfang_memory::backends::TaskQueueBackend;
        use openfang_memory::sqlite::SqliteTaskQueueStore;
        let store = SqliteTaskQueueStore::new(conn);

        // Post a task
        let task_id = store.task_post("Review code", "Check auth module", "auditor", "orchestrator").unwrap();
        assert!(!task_id.is_empty());

        // List pending
        let tasks = store.task_list(Some("pending")).unwrap();
        assert!(tasks.iter().any(|t| t["id"] == task_id));

        // Claim
        let claimed = store.task_claim("auditor").unwrap();
        assert!(claimed.is_some());
        assert_eq!(claimed.unwrap()["status"], "in_progress");

        // Complete
        store.task_complete(&task_id, "All good").unwrap();
        let tasks = store.task_list(Some("completed")).unwrap();
        assert!(tasks.iter().any(|t| t["id"] == task_id && t["result"] == "All good"));
    }

    #[test]
    fn sqlite_audit_log() {
        let conn = setup();
        use openfang_memory::backends::AuditBackend;
        use openfang_memory::sqlite::SqliteAuditStore;
        let store = SqliteAuditStore::new(conn);

        // Append entries
        store.append_entry("agent-1", "message", "sent hello", "success").unwrap();
        store.append_entry("agent-1", "tool_call", "ran search", "success").unwrap();

        // Load all
        let entries = store.load_entries(None, 10).unwrap();
        assert!(entries.len() >= 2);

        // Load filtered by agent
        let entries = store.load_entries(Some("agent-1"), 10).unwrap();
        assert!(entries.len() >= 2);

        // Load with non-matching agent
        let entries = store.load_entries(Some("agent-999"), 10).unwrap();
        assert_eq!(entries.len(), 0);
    }
}

// ─── PostgreSQL backend tests ──────────────────────────────────────────
// All PG tests use #[tokio::test] since the PG store uses block_on(Handle::current())

#[cfg(feature = "postgres")]
mod postgres {
    use super::*;
    use openfang_memory::backends::{SessionBackend, UsageBackend};
    use openfang_memory::postgres::*;
    use openfang_memory::usage::UsageRecord;
    use openfang_types::storage::{KnowledgeBackend, StructuredBackend};

    async fn setup() -> Option<deadpool_postgres::Pool> {
        let url = std::env::var("TEST_POSTGRES_URL")
            .unwrap_or_else(|_| "postgresql://openfang:openfang@localhost:5432/openfang_test".to_string());
        let pool = create_pool(&url, 2).ok()?;
        run_migrations(&pool).await.ok()?;
        // No TRUNCATE — each test uses unique AgentId/SessionId so tests
        // can run in parallel without interfering with each other.
        Some(pool)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_structured_kv_crud() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        let store = PgStructuredStore::new(pool);
        let agent = AgentId::new();

        store.set(agent, "color", serde_json::json!("blue")).unwrap();
        assert_eq!(store.get(agent, "color").unwrap(), Some(serde_json::json!("blue")));

        store.set(agent, "color", serde_json::json!("red")).unwrap();
        assert_eq!(store.get(agent, "color").unwrap(), Some(serde_json::json!("red")));

        assert_eq!(store.list_kv(agent).unwrap().len(), 1);

        store.delete(agent, "color").unwrap();
        assert_eq!(store.get(agent, "color").unwrap(), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_semantic_remember_recall_forget() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        let store = PgSemanticStore::new(pool);
        let agent = AgentId::new();

        let id = SemanticBackend::remember(
            &store, agent, "The quick brown fox jumps over the lazy dog",
            MemorySource::Conversation, "episodic", HashMap::new(), None,
        ).unwrap();

        let results = SemanticBackend::recall(&store, "quick brown fox", 10, Some(agent_filter(agent)), None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);

        let results = SemanticBackend::recall(&store, "fox", 10, Some(agent_filter(AgentId::new())), None).unwrap();
        assert_eq!(results.len(), 0);

        SemanticBackend::forget(&store, id).unwrap();
        let results = SemanticBackend::recall(&store, "fox", 10, Some(agent_filter(agent)), None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_knowledge_entity_relation() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        let store = PgKnowledgeStore::new(pool);

        let alice_id = store.add_entity(openfang_types::memory::Entity {
            id: String::new(), entity_type: openfang_types::memory::EntityType::Person,
            name: "Alice".to_string(), properties: HashMap::new(),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }).unwrap();

        let acme_id = store.add_entity(openfang_types::memory::Entity {
            id: String::new(), entity_type: openfang_types::memory::EntityType::Organization,
            name: "Acme Corp".to_string(), properties: HashMap::new(),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }).unwrap();

        store.add_relation(openfang_types::memory::Relation {
            source: alice_id.clone(), relation: openfang_types::memory::RelationType::WorksAt,
            target: acme_id, properties: HashMap::new(), confidence: 0.9,
            created_at: chrono::Utc::now(),
        }).unwrap();

        // Query by entity ID (unique) to avoid matching entities from other test runs
        let matches = store.query_graph(openfang_types::memory::GraphPattern {
            source: Some(alice_id), relation: None, target: None, max_depth: 1,
        }).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].source.name, "Alice");
        assert_eq!(matches[0].target.name, "Acme Corp");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_session_crud() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        let store = PgSessionStore::new(pool);
        let agent = AgentId::new();

        let session = store.create_session(agent).unwrap();
        assert!(store.get_session(session.id).unwrap().is_some());

        let labeled = store.create_session_with_label(agent, Some("pg-test")).unwrap();
        assert_eq!(store.find_session_by_label(agent, "pg-test").unwrap().unwrap().id, labeled.id);

        assert!(store.list_agent_sessions(agent).unwrap().len() >= 2);

        store.delete_session(session.id).unwrap();
        assert!(store.get_session(session.id).unwrap().is_none());
        store.delete_agent_sessions(agent).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_usage_record_and_query() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        let store = PgUsageStore::new(pool);
        let agent = AgentId::new();

        store.record(&UsageRecord {
            agent_id: agent, model: "gpt-4".to_string(),
            input_tokens: 100, output_tokens: 50, cost_usd: 0.005, tool_calls: 2,
        }).unwrap();

        assert!(store.query_hourly(agent).unwrap() > 0.0);
        let summary = store.query_summary(Some(agent)).unwrap();
        assert_eq!(summary.total_input_tokens, 100);
        assert_eq!(summary.call_count, 1);
        assert!(!store.query_by_model().unwrap().is_empty());
        assert!(store.query_today_cost().unwrap() > 0.0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_canonical_session() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        let store = PgSessionStore::new(pool);
        let agent = AgentId::new();

        assert!(store.load_canonical(agent).unwrap().messages.is_empty());

        let msg = openfang_types::message::Message {
            role: openfang_types::message::Role::User,
            content: openfang_types::message::MessageContent::Text("hello from pg".to_string()),
        };
        assert_eq!(store.append_canonical(agent, &[msg], None).unwrap().messages.len(), 1);

        let (summary, messages) = store.canonical_context(agent, None).unwrap();
        assert!(summary.is_none());
        assert_eq!(messages.len(), 1);

        store.store_llm_summary(agent, "PG summary", vec![]).unwrap();
        assert_eq!(store.canonical_context(agent, None).unwrap().0, Some("PG summary".to_string()));

        store.delete_canonical_session(agent).unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_paired_devices_crud() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        use openfang_memory::backends::PairedDevicesBackend;
        let store = PgPairedDevicesStore::new(pool);

        // Save a device
        store.save_paired_device("dev-1", "iPhone", "ios", "2025-01-01T00:00:00Z", "2025-01-01T00:00:00Z", Some("token123")).unwrap();

        // Load — should find it
        let devices = store.load_paired_devices().unwrap();
        assert!(devices.iter().any(|d| d["device_id"] == "dev-1"));

        // Update (upsert)
        store.save_paired_device("dev-1", "iPhone Pro", "ios", "2025-01-01T00:00:00Z", "2025-06-01T00:00:00Z", None).unwrap();
        let devices = store.load_paired_devices().unwrap();
        let dev = devices.iter().find(|d| d["device_id"] == "dev-1").unwrap();
        assert_eq!(dev["display_name"], "iPhone Pro");

        // Remove
        store.remove_paired_device("dev-1").unwrap();
        let devices = store.load_paired_devices().unwrap();
        assert!(!devices.iter().any(|d| d["device_id"] == "dev-1"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_task_queue_crud() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        use openfang_memory::backends::TaskQueueBackend;
        let store = PgTaskQueueStore::new(pool);

        // Post a task
        let task_id = store.task_post("Review code", "Check auth module", "auditor", "orchestrator").unwrap();
        assert!(!task_id.is_empty());

        // List pending
        let tasks = store.task_list(Some("pending")).unwrap();
        assert!(tasks.iter().any(|t| t["id"] == task_id));

        // Claim
        let claimed = store.task_claim("auditor").unwrap();
        assert!(claimed.is_some());
        assert_eq!(claimed.unwrap()["status"], "in_progress");

        // Complete
        store.task_complete(&task_id, "All good").unwrap();
        let tasks = store.task_list(Some("completed")).unwrap();
        assert!(tasks.iter().any(|t| t["id"] == task_id && t["result"] == "All good"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_consolidation() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        use openfang_memory::backends::ConsolidationBackend;
        let engine = PgConsolidationEngine::new(pool);
        let report = engine.consolidate().unwrap();
        // Just verify it doesn't error — no memories to decay in empty DB
        assert_eq!(report.memories_merged, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pg_audit_log() {
        let pool = match setup().await {
            Some(p) => p,
            None => { eprintln!("SKIP: PostgreSQL not available"); return; }
        };
        use openfang_memory::backends::AuditBackend;
        let store = PgAuditStore::new(pool);

        // Append entries
        store.append_entry("agent-1", "message", "sent hello", "success").unwrap();
        store.append_entry("agent-1", "tool_call", "ran search", "success").unwrap();

        // Load all
        let entries = store.load_entries(None, 10).unwrap();
        assert!(entries.len() >= 2);

        // Load filtered by agent
        let entries = store.load_entries(Some("agent-1"), 10).unwrap();
        assert!(entries.len() >= 2);

        // Load with non-matching agent
        let entries = store.load_entries(Some("agent-999"), 10).unwrap();
        assert_eq!(entries.len(), 0);
    }
}

// ─── Qdrant semantic backend tests ─────────────────────────────────────
// All Qdrant tests use #[tokio::test] since the store uses block_on(Handle::current())

#[cfg(feature = "qdrant")]
mod qdrant_tests {
    use super::*;
    use openfang_memory::qdrant::QdrantSemanticStore;

    fn setup() -> Option<QdrantSemanticStore> {
        let url = std::env::var("TEST_QDRANT_URL")
            .unwrap_or_else(|_| "http://localhost:6334".to_string());
        let collection = format!("openfang_test_{}", uuid::Uuid::new_v4().simple());
        QdrantSemanticStore::new(&url, None, &collection).ok()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn qdrant_remember_requires_embedding() {
        let store = match setup() {
            Some(s) => s,
            None => { eprintln!("SKIP: Qdrant not available"); return; }
        };

        let result = SemanticBackend::remember(
            &store, AgentId::new(), "no embedding test",
            MemorySource::Conversation, "episodic", HashMap::new(), None,
        );
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn qdrant_remember_recall_forget() {
        let store = match setup() {
            Some(s) => s,
            None => { eprintln!("SKIP: Qdrant not available"); return; }
        };
        let agent = AgentId::new();

        let embedding = vec![0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let id = SemanticBackend::remember(
            &store, agent, "Qdrant vector test content",
            MemorySource::Conversation, "episodic", HashMap::new(), Some(&embedding),
        ).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let query_emb = vec![0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let results = SemanticBackend::recall(&store, "", 10, None, Some(&query_emb)).unwrap();
        assert!(!results.is_empty(), "Expected results from Qdrant recall");
        assert_eq!(results[0].id, id);
        assert!(results[0].content.contains("Qdrant vector test"));

        // Without embedding returns empty
        let results = SemanticBackend::recall(&store, "anything", 10, None, None).unwrap();
        assert!(results.is_empty());

        // With matching agent filter
        let results = SemanticBackend::recall(&store, "", 10, Some(agent_filter(agent)), Some(&query_emb)).unwrap();
        assert!(!results.is_empty());

        // With wrong agent filter
        let results = SemanticBackend::recall(&store, "", 10, Some(agent_filter(AgentId::new())), Some(&query_emb)).unwrap();
        assert!(results.is_empty());

        // Forget
        SemanticBackend::forget(&store, id).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let results = SemanticBackend::recall(&store, "", 10, None, Some(&query_emb)).unwrap();
        assert!(results.is_empty(), "Expected 0 results after forget");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn qdrant_update_embedding() {
        let store = match setup() {
            Some(s) => s,
            None => { eprintln!("SKIP: Qdrant not available"); return; }
        };
        let agent = AgentId::new();

        let original = vec![1.0f32, 0.0, 0.0, 0.0];
        let id = SemanticBackend::remember(
            &store, agent, "update embedding test",
            MemorySource::System, "episodic", HashMap::new(), Some(&original),
        ).unwrap();

        let updated = vec![0.0f32, 1.0, 0.0, 0.0];
        store.update_embedding(id, &updated).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let results = SemanticBackend::recall(&store, "", 10, None, Some(&updated)).unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn qdrant_multiple_memories_ranked() {
        let store = match setup() {
            Some(s) => s,
            None => { eprintln!("SKIP: Qdrant not available"); return; }
        };
        let agent = AgentId::new();

        let emb1 = vec![1.0f32, 0.0, 0.0, 0.0];
        let emb2 = vec![0.0f32, 1.0, 0.0, 0.0];
        let emb3 = vec![0.9f32, 0.1, 0.0, 0.0];

        SemanticBackend::remember(&store, agent, "memory A", MemorySource::System, "episodic", HashMap::new(), Some(&emb1)).unwrap();
        SemanticBackend::remember(&store, agent, "memory B", MemorySource::System, "episodic", HashMap::new(), Some(&emb2)).unwrap();
        SemanticBackend::remember(&store, agent, "memory C", MemorySource::System, "episodic", HashMap::new(), Some(&emb3)).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let results = SemanticBackend::recall(&store, "", 10, None, Some(&emb1)).unwrap();
        assert_eq!(results.len(), 3);
        assert!(
            results[0].content == "memory A" || results[0].content == "memory C",
            "Expected A or C first, got: {}", results[0].content
        );
        assert_eq!(results[2].content, "memory B");

        let results = SemanticBackend::recall(&store, "", 1, None, Some(&emb1)).unwrap();
        assert_eq!(results.len(), 1);
    }
}

// ─── Full substrate integration test ───────────────────────────────────

// ─── Full substrate integration test ───────────────────────────────────

mod substrate {
    use super::*;
    use openfang_memory::MemorySubstrate;
    use openfang_types::memory::Memory;

    #[tokio::test]
    async fn sqlite_substrate_full_cycle() {
        let substrate = MemorySubstrate::open_in_memory(0.1).unwrap();
        let agent = AgentId::new();

        substrate.set(agent, "name", serde_json::json!("test-agent")).await.unwrap();
        assert_eq!(substrate.get(agent, "name").await.unwrap(), Some(serde_json::json!("test-agent")));

        substrate.remember(agent, "Integration test memory", MemorySource::Conversation, "episodic", HashMap::new()).await.unwrap();
        assert_eq!(substrate.recall("integration test", 10, None).await.unwrap().len(), 1);

        let eid = substrate.add_entity(openfang_types::memory::Entity {
            id: String::new(), entity_type: openfang_types::memory::EntityType::Concept,
            name: "Rust".to_string(), properties: HashMap::new(),
            created_at: chrono::Utc::now(), updated_at: chrono::Utc::now(),
        }).await.unwrap();
        assert!(!eid.is_empty());

        let session = substrate.create_session(agent).unwrap();
        substrate.delete_session(session.id).unwrap();

        assert_eq!(substrate.consolidate().await.unwrap().memories_merged, 0);
    }
}
