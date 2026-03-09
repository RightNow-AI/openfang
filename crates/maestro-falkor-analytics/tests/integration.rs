//! Integration tests for maestro-falkor-analytics
//!
//! These tests require Docker to be running. We use testcontainers
//! to spin up an ephemeral FalkorDB container for testing.

use maestro_falkor_analytics::config::FalkorConfig;
use maestro_falkor_analytics::etl::run_etl;
use maestro_falkor_analytics::FalkorAnalytics;
use openfang_memory::MemorySubstrate;
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
    let surreal = MemorySubstrate::connect_in_memory()
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

    let surreal = MemorySubstrate::connect_in_memory()
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

// ---------------------------------------------------------------------------
// Task 6.4: Analytics Query API tests
// ---------------------------------------------------------------------------

/// Helper: seed a FalkorDB graph with test entities, relations, and memories
/// via ETL from SurrealDB, then return the analytics handle.
async fn seed_analytics_graph(
    host: &str,
    port: u16,
    graph_name: &str,
) -> (FalkorAnalytics, openfang_types::agent::AgentId) {
    let surreal = MemorySubstrate::connect_in_memory()
        .await
        .expect("Failed to connect to SurrealDB in-memory");

    let agent_id = openfang_types::agent::AgentId::new();

    // Entities
    for (id, name, etype) in [
        ("alice-001", "Alice", EntityType::Person),
        ("bob-002", "Bob", EntityType::Person),
        ("techcorp-001", "TechCorp", EntityType::Organization),
        ("rust-001", "Rust", EntityType::Concept),
        ("sf-001", "San Francisco", EntityType::Location),
    ] {
        surreal
            .add_entity(Entity {
                id: id.to_string(),
                entity_type: etype,
                name: name.to_string(),
                properties: std::collections::HashMap::new(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            })
            .await
            .expect("Failed to add entity");
    }

    // Relations
    for (src, rel, tgt, conf) in [
        ("alice-001", RelationType::WorksAt, "techcorp-001", 0.95),
        ("bob-002", RelationType::WorksAt, "techcorp-001", 0.90),
        ("alice-001", RelationType::KnowsAbout, "rust-001", 0.85),
        ("techcorp-001", RelationType::LocatedIn, "sf-001", 0.99),
        ("bob-002", RelationType::KnowsAbout, "rust-001", 0.70),
    ] {
        surreal
            .add_relation(Relation {
                source: src.to_string(),
                relation: rel,
                target: tgt.to_string(),
                properties: std::collections::HashMap::new(),
                confidence: conf,
                created_at: chrono::Utc::now(),
            })
            .await
            .expect("Failed to add relation");
    }

    // Memories
    for content in [
        "Alice is a senior Rust developer at TechCorp",
        "Bob joined TechCorp last month",
        "TechCorp is headquartered in San Francisco",
    ] {
        surreal
            .remember(
                agent_id,
                content,
                MemorySource::Observation,
                "test",
                std::collections::HashMap::new(),
            )
            .await
            .expect("Failed to add memory");
    }

    let analytics = connect_analytics(host, port, graph_name).await;
    let report = run_etl(&surreal, &analytics).await.expect("ETL failed");
    assert!(report.entities_loaded >= 5);
    assert!(report.relations_loaded >= 5);
    assert!(report.memories_loaded >= 3);

    (analytics, agent_id)
}

#[tokio::test]
async fn test_graph_stats() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_stats").await;

    let stats = analytics.graph_stats().await.expect("graph_stats failed");
    assert!(stats.total_entities >= 5, "Expected >= 5 entities, got {}", stats.total_entities);
    assert!(stats.total_memories >= 3, "Expected >= 3 memories, got {}", stats.total_memories);
    assert!(stats.total_relations >= 5, "Expected >= 5 relations, got {}", stats.total_relations);
    assert!(!stats.entity_type_distribution.is_empty(), "Entity type distribution should not be empty");
    assert!(!stats.relation_type_distribution.is_empty(), "Relation type distribution should not be empty");
}

#[tokio::test]
async fn test_get_entity() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_get_entity").await;

    let entity = analytics
        .get_entity("alice-001")
        .await
        .expect("get_entity failed");
    assert!(entity.is_some(), "Alice should exist");
    let alice = entity.unwrap();
    assert_eq!(alice.name, "Alice");
    assert_eq!(alice.entity_type, "Person");

    let missing = analytics
        .get_entity("nonexistent-999")
        .await
        .expect("get_entity failed");
    assert!(missing.is_none(), "Nonexistent entity should return None");
}

#[tokio::test]
async fn test_entity_neighbors() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_neighbors").await;

    let neighbors = analytics
        .entity_neighbors("alice-001")
        .await
        .expect("entity_neighbors failed");
    assert!(
        neighbors.len() >= 2,
        "Alice should have at least 2 neighbors (TechCorp, Rust), got {}",
        neighbors.len()
    );

    let neighbor_names: Vec<&str> = neighbors.iter().map(|n| n.entity.name.as_str()).collect();
    assert!(neighbor_names.contains(&"TechCorp"), "Alice should be connected to TechCorp");
    assert!(neighbor_names.contains(&"Rust"), "Alice should be connected to Rust");
}

#[tokio::test]
async fn test_search_entities() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_search").await;

    let results = analytics
        .search_entities("alice", 10)
        .await
        .expect("search_entities failed");
    assert!(!results.is_empty(), "Search for 'alice' should return results");
    assert_eq!(results[0].name, "Alice");

    let results = analytics
        .search_entities("tech", 10)
        .await
        .expect("search_entities failed");
    assert!(!results.is_empty(), "Search for 'tech' should return results");
    assert_eq!(results[0].name, "TechCorp");
}

#[tokio::test]
async fn test_top_hubs() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_hubs").await;

    let hubs = analytics.top_hubs(3).await.expect("top_hubs failed");
    assert!(!hubs.is_empty(), "Should have at least one hub");
    // TechCorp or Rust should be top hubs (most connections)
    assert!(
        hubs[0].connection_count >= 2,
        "Top hub should have at least 2 connections, got {}",
        hubs[0].connection_count
    );
}

#[tokio::test]
async fn test_entity_type_distribution() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_type_dist").await;

    let dist = analytics
        .entity_type_distribution()
        .await
        .expect("entity_type_distribution failed");
    assert!(!dist.is_empty(), "Distribution should not be empty");

    let person_count: i64 = dist
        .iter()
        .filter(|tc| tc.label == "Person")
        .map(|tc| tc.count)
        .sum();
    assert!(person_count >= 2, "Should have at least 2 Person entities");
}

#[tokio::test]
async fn test_agent_memories() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, agent_id) = seed_analytics_graph(&host, port, "test_agent_mem").await;

    let memories = analytics
        .agent_memories(&agent_id.0.to_string(), 10)
        .await
        .expect("agent_memories failed");
    assert!(
        memories.len() >= 3,
        "Should have at least 3 memories, got {}",
        memories.len()
    );
}

#[tokio::test]
async fn test_agent_memory_stats() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_mem_stats").await;

    let stats = analytics
        .agent_memory_stats()
        .await
        .expect("agent_memory_stats failed");
    assert!(!stats.is_empty(), "Should have at least one agent's stats");
    assert!(stats[0].memory_count >= 3, "Agent should have at least 3 memories");
}

#[tokio::test]
async fn test_high_confidence_relations() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_high_conf").await;

    let relations = analytics
        .high_confidence_relations(0.9, 10)
        .await
        .expect("high_confidence_relations failed");
    assert!(
        !relations.is_empty(),
        "Should have at least one high-confidence relation"
    );
    for rel in &relations {
        assert!(
            rel.confidence >= 0.9,
            "All returned relations should have confidence >= 0.9, got {}",
            rel.confidence
        );
    }
}

#[tokio::test]
async fn test_shortest_path() {
    let (_container, host, port) = start_falkordb().await;
    let (analytics, _agent_id) = seed_analytics_graph(&host, port, "test_path").await;

    // Alice -> TechCorp -> SF should be a valid path
    let path = analytics
        .shortest_path("alice-001", "sf-001")
        .await
        .expect("shortest_path failed");
    assert!(path.is_some(), "Should find a path from Alice to San Francisco");
    let path = path.unwrap();
    assert!(
        path.length >= 2,
        "Path from Alice to SF should be at least 2 hops, got {}",
        path.length
    );
    assert!(
        path.steps.len() >= 3,
        "Path should have at least 3 nodes (Alice, TechCorp, SF), got {}",
        path.steps.len()
    );
}
