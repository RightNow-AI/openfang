//! Integration tests for maestro-falkor-analytics
//!
//! These tests require a running FalkorDB instance. We use testcontainers
//! to spin up an ephemeral FalkorDB container for testing.

use maestro_falkor_analytics::config::FalkorConfig;
use maestro_falkor_analytics::etl::run_etl;
use maestro_falkor_analytics::FalkorAnalytics;
use maestro_surreal_memory::SurrealMemorySubstrate;
use openfang_types::memory::{Entity, EntityType, Memory, MemorySource, Relation, RelationType};
use std::sync::Arc;
use testcontainers::runners::AsyncRunner;
use testcontainers::ImageExt;
use testcontainers_modules::redis::{Redis, REDIS_PORT};

#[tokio::test]
async fn test_falkor_connection_and_health_check() {
    let redis_instance = Redis::default()
        .with_tag("falkordb/falkordb:latest")
        .start()
        .await
        .expect("Failed to start FalkorDB container");

    let host = redis_instance.get_host().await.unwrap();
    let port = redis_instance.get_host_port_ipv4(REDIS_PORT).await.unwrap();

    let config = FalkorConfig {
        database_url: format!("falkor://{}:{}", host, port),
        graph_name: "test_graph".to_string(),
    };

    let analytics = FalkorAnalytics::new(config)
        .await
        .expect("Failed to connect to FalkorDB");

    let result = analytics.health_check().await.expect("Health check failed");

    assert!(result, "Health check should return true");
}

#[tokio::test]
async fn test_simple_cypher_query() {
    let redis_instance = Redis::default()
        .with_tag("falkordb/falkordb:latest")
        .start()
        .await
        .expect("Failed to start FalkorDB container");

    let host = redis_instance.get_host().await.unwrap();
    let port = redis_instance.get_host_port_ipv4(REDIS_PORT).await.unwrap();

    let config = FalkorConfig {
        database_url: format!("falkor://{}:{}", host, port),
        graph_name: "test_graph".to_string(),
    };

    let analytics = FalkorAnalytics::new(config)
        .await
        .expect("Failed to connect to FalkorDB");

    let result = analytics
        .query("CREATE (n:Person {name: 'Alice'}) RETURN n.name")
        .await
        .expect("Query failed");

    assert_eq!(result, 1, "Query should return 1 row");
}

#[tokio::test]
async fn test_graph_creation_and_query() {
    let redis_instance = Redis::default()
        .with_tag("falkordb/falkordb:latest")
        .start()
        .await
        .expect("Failed to start FalkorDB container");

    let host = redis_instance.get_host().await.unwrap();
    let port = redis_instance.get_host_port_ipv4(REDIS_PORT).await.unwrap();

    let config = FalkorConfig {
        database_url: format!("falkor://{}:{}", host, port),
        graph_name: "test_graph".to_string(),
    };

    let analytics = FalkorAnalytics::new(config)
        .await
        .expect("Failed to connect to FalkorDB");

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
    let redis_instance = Redis::default()
        .with_tag("falkordb/falkordb:latest")
        .start()
        .await
        .expect("Failed to start FalkorDB container");

    let host = redis_instance.get_host().await.unwrap();
    let port = redis_instance.get_host_port_ipv4(REDIS_PORT).await.unwrap();

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

    let config = FalkorConfig {
        database_url: format!("falkor://{}:{}", host, port),
        graph_name: "test_etl_graph".to_string(),
    };

    let analytics = FalkorAnalytics::new(config)
        .await
        .expect("Failed to connect to FalkorDB");

    let report = run_etl(&surreal, &analytics).await.expect("ETL failed");

    assert!(
        report.entities_loaded >= 2,
        "Should have loaded at least 2 entities"
    );
    assert!(
        report.relations_loaded >= 1,
        "Should have loaded at least 1 relation"
    );
    assert!(
        report.memories_loaded >= 1,
        "Should have loaded at least 1 memory"
    );

    let entity_count = analytics
        .query("MATCH (e:Entity:Person) RETURN count(e)")
        .await
        .expect("Failed to query entities");

    assert!(
        entity_count >= 1,
        "Should have at least one Person entity in FalkorDB"
    );
}

#[tokio::test]
async fn test_etl_background_scheduling() {
    let redis_instance = Redis::default()
        .with_tag("falkordb/falkordb:latest")
        .start()
        .await
        .expect("Failed to start FalkorDB container");

    let host = redis_instance.get_host().await.unwrap();
    let port = redis_instance.get_host_port_ipv4(REDIS_PORT).await.unwrap();

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

    let config = FalkorConfig {
        database_url: format!("falkor://{}:{}", host, port),
        graph_name: "test_background_graph".to_string(),
    };

    let analytics = FalkorAnalytics::new(config)
        .await
        .expect("Failed to connect to FalkorDB");

    let memory_arc: Arc<dyn openfang_types::memory::Memory> = Arc::new(surreal);
    let analytics_clone = analytics.clone();

    let handle = maestro_falkor_analytics::spawn_etl(memory_arc, analytics_clone);

    let report = handle.await.expect("ETL task panicked").expect("ETL failed");

    assert!(
        report.memories_loaded >= 1,
        "Should have loaded at least 1 memory"
    );
}
