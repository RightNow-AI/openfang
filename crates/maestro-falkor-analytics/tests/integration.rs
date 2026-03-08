//! Integration tests for maestro-falkor-analytics
//!
//! These tests require Docker to be running. We use testcontainers
//! to spin up an ephemeral FalkorDB container for testing.

use maestro_falkor_analytics::config::FalkorConfig;
use maestro_falkor_analytics::etl::run_etl;
use maestro_falkor_analytics::FalkorAnalytics;
use maestro_surreal_memory::SurrealMemorySubstrate;
use openfang_types::memory::{Entity, EntityType, Memory, MemorySource, Relation, RelationType};
use std::sync::Arc;
use testcontainers::core::IntoContainerPort;
use testcontainers::core::WaitFor;
use testcontainers::runners::AsyncRunner;
use testcontainers::GenericImage;

/// FalkorDB listens on the Redis protocol port (6379) by default.
const FALKORDB_PORT: u16 = 6379;

/// Helper: spin up a FalkorDB container and return (container, host, port).
/// The container must be held alive (not dropped) for the duration of the test.
async fn start_falkordb() -> (testcontainers::ContainerAsync<GenericImage>, String, u16) {
    let container = GenericImage::new("falkordb/falkordb", "latest")
        .with_exposed_port(FALKORDB_PORT.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await
        .expect("Failed to start FalkorDB container");

    let host = container.get_host().await.unwrap().to_string();
    let port = container.get_host_port_ipv4(FALKORDB_PORT).await.unwrap();

    (container, host, port)
}

/// Helper: create a FalkorAnalytics instance connected to the given host:port.
async fn connect_analytics(host: &str, port: u16, graph_name: &str) -> FalkorAnalytics {
    let config = FalkorConfig {
        database_url: format!("redis://{}:{}", host, port),
        graph_name: graph_name.to_string(),
    };

    FalkorAnalytics::new(config)
        .await
        .expect("Failed to connect to FalkorDB")
}

#[tokio::test]
async fn test_falkor_connection_and_health_check() {
    let (_container, host, port) = start_falkordb().await;
    let analytics = connect_analytics(&host, port, "test_health").await;

    let result = analytics.health_check().await.expect("Health check failed");
    assert!(result, "Health check should return true");
}

#[tokio::test]
async fn test_simple_cypher_query() {
    let (_container, host, port) = start_falkordb().await;
    let analytics = connect_analytics(&host, port, "test_cypher").await;

    let result = analytics
        .query("CREATE (n:Person {name: 'Alice'}) RETURN n.name")
        .await
        .expect("Query failed");

    assert_eq!(result, 1, "Query should return 1 row");
}

#[tokio::test]
async fn test_graph_creation_and_query() {
    let (_container, host, port) = start_falkordb().await;
    let analytics = connect_analytics(&host, port, "test_graph").await;

    analytics
        .query("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .await
        .expect("Failed to create graph");

    let result = analytics
        .query("MATCH (a)-[:KNOWS]->(b) RETURN a.name, b.name")
        .await
        .expect("Failed to query graph");

    assert_eq!(result, 1, "Should find the relationship we created");
}

#[tokio::test]
async fn test_etl_from_surreal_to_falkor() {
    let (_container, host, port) = start_falkordb().await;

    // Set up SurrealDB in-memory with test data
    let surreal = SurrealMemorySubstrate::connect_in_memory()
        .await
        .expect("Failed to connect to SurrealDB in-memory");

    let agent_id = openfang_types::agent::AgentId::new();

    surreal
        .remember(
            agent_id,
            "Alice works at TechCorp",
            MemorySource::Observation,
            "test",
            std::collections::HashMap::new(),
        )
        .await
        .expect("Failed to add memory");

    surreal
        .add_entity(Entity {
            id: "alice-001".to_string(),
            entity_type: EntityType::Person,
            name: "Alice".to_string(),
            properties: std::collections::HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .await
        .expect("Failed to add entity");

    surreal
        .add_entity(Entity {
            id: "techcorp-001".to_string(),
            entity_type: EntityType::Organization,
            name: "TechCorp".to_string(),
            properties: std::collections::HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .await
        .expect("Failed to add organization entity");

    surreal
        .add_relation(Relation {
            source: "alice-001".to_string(),
            relation: RelationType::WorksAt,
            target: "techcorp-001".to_string(),
            properties: std::collections::HashMap::new(),
            confidence: 0.95,
            created_at: chrono::Utc::now(),
        })
        .await
        .expect("Failed to add relation");

    let analytics = connect_analytics(&host, port, "test_etl_graph").await;

    let report = run_etl(&surreal, &analytics).await.expect("ETL failed");

    assert!(
        report.entities_loaded >= 2,
        "Should have loaded at least 2 entities, got {}",
        report.entities_loaded
    );
    assert!(
        report.relations_loaded >= 1,
        "Should have loaded at least 1 relation, got {}",
        report.relations_loaded
    );
    assert!(
        report.memories_loaded >= 1,
        "Should have loaded at least 1 memory, got {}",
        report.memories_loaded
    );

    // Verify entities are queryable in FalkorDB.
    // Note: entities are stored with type as a property, not a second Cypher label.
    let entity_count = analytics
        .query("MATCH (e:Entity) RETURN count(e)")
        .await
        .expect("Failed to query entities");

    assert!(
        entity_count >= 1,
        "Should have at least one Entity in FalkorDB"
    );
}

#[tokio::test]
async fn test_etl_background_scheduling() {
    let (_container, host, port) = start_falkordb().await;

    let surreal = SurrealMemorySubstrate::connect_in_memory()
        .await
        .expect("Failed to connect to SurrealDB in-memory");

    let agent_id = openfang_types::agent::AgentId::new();

    surreal
        .remember(
            agent_id,
            "Background test memory",
            MemorySource::Observation,
            "test",
            std::collections::HashMap::new(),
        )
        .await
        .expect("Failed to add memory");

    let analytics = connect_analytics(&host, port, "test_background_graph").await;

    let memory_arc: Arc<dyn openfang_types::memory::Memory> = Arc::new(surreal);
    let analytics_clone = analytics.clone();

    let handle = maestro_falkor_analytics::spawn_etl(memory_arc, analytics_clone);

    let report = handle
        .await
        .expect("ETL task panicked")
        .expect("ETL failed");

    assert!(
        report.memories_loaded >= 1,
        "Should have loaded at least 1 memory"
    );
}
