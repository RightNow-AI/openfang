//! Integration tests for maestro-falkor-analytics
//!
//! These tests require a running FalkorDB instance. We use testcontainers
//! to spin up an ephemeral FalkorDB container for testing.

use maestro_falkor_analytics::config::FalkorConfig;
use maestro_falkor_analytics::FalkorAnalytics;
use testcontainer_modules::redis::Redis;
use testcontainers::clients::Cli;

#[tokio::test]
async fn test_falkor_connection_and_health_check() {
    // Start a FalkorDB container
    let docker = Cli::default();
    let _container = docker.run(Redis::default().with_image("falkordb/falkordb:latest"));

    // Get the mapped port
    let port = 6379; // testcontainers-modules redis maps to 6379 by default

    let config = FalkorConfig {
        database_url: format!("falkor://127.0.0.1:{}", port),
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
    // Start a FalkorDB container
    let docker = Cli::default();
    let _container = docker.run(Redis::default().with_image("falkordb/falkordb:latest"));

    let port = 6379;

    let config = FalkorConfig {
        database_url: format!("falkor://127.0.0.1:{}", port),
        graph_name: "test_graph".to_string(),
    };

    let analytics = FalkorAnalytics::new(config)
        .await
        .expect("Failed to connect to FalkorDB");

    // Execute a simple CREATE and RETURN query
    let result = analytics
        .query("CREATE (n:Person {name: 'Alice'}) RETURN n.name")
        .await
        .expect("Query failed");

    // Verify we got results (should return 1 row)
    assert_eq!(result, 1, "Query should return 1 row");
}

#[tokio::test]
async fn test_graph_creation_and_query() {
    // Start a FalkorDB container
    let docker = Cli::default();
    let _container = docker.run(Redis::default().with_image("falkordb/falkordb:latest"));

    let port = 6379;

    let config = FalkorConfig {
        database_url: format!("falkor://127.0.0.1:{}", port),
        graph_name: "test_graph".to_string(),
    };

    let analytics = FalkorAnalytics::new(config)
        .await
        .expect("Failed to connect to FalkorDB");

    // Create nodes and relationships
    analytics
        .query("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .await
        .expect("Failed to create graph");

    // Query back the data
    let result = analytics
        .query("MATCH (a)-[:KNOWS]->(b) RETURN a.name, b.name")
        .await
        .expect("Failed to query graph");

    assert_eq!(result, 1, "Should find the relationship we created");
}
