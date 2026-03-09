//! maestro-falkor-analytics - L4 FalkorDB Analytics Engine
//!
//! This crate provides the foundation for graph analytics using FalkorDB.
//! It connects to a FalkorDB instance and provides methods for health checks,
//! graph queries, and analytics operations.

pub mod config;
pub mod etl;
pub mod queries;

use etl::EtlReport;
use falkordb::{AsyncGraph, FalkorClientBuilder, FalkorConnectionInfo};
use openfang_types::error::{OpenFangError, OpenFangResult};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

/// The main struct for interacting with FalkorDB analytics.
///
/// This is the primary entry point for all crate functionality.
/// It holds an async connection to FalkorDB and configuration.
#[derive(Clone)]
pub struct FalkorAnalytics {
    /// The async FalkorDB graph handle
    graph: Arc<Mutex<AsyncGraph>>,
    /// The configuration used to connect
    config: config::FalkorConfig,
}

impl FalkorAnalytics {
    /// Creates a new FalkorAnalytics instance from the given configuration.
    ///
    /// # Errors
    /// Returns an error if the connection to FalkorDB fails or the URL is invalid.
    pub async fn new(config: config::FalkorConfig) -> OpenFangResult<Self> {
        let connection_info = FalkorConnectionInfo::try_from(config.database_url.as_str())
            .map_err(|e| OpenFangError::Memory(format!("Invalid FalkorDB URL: {}", e)))?;

        let client = FalkorClientBuilder::new_async()
            .with_connection_info(connection_info)
            .build()
            .await
            .map_err(|e| {
                OpenFangError::Memory(format!("Failed to build FalkorDB client: {}", e))
            })?;

        let graph = client.select_graph(&config.graph_name);

        Ok(Self {
            graph: Arc::new(Mutex::new(graph)),
            config,
        })
    }

    /// Performs a health check on the FalkorDB connection.
    ///
    /// Executes a simple Cypher query to verify connectivity.
    /// Returns `true` if the connection is healthy.
    ///
    /// # Errors
    /// Returns an error if the query fails or the connection is lost.
    pub async fn health_check(&self) -> OpenFangResult<bool> {
        let mut graph = self.graph.lock().await;
        let result = graph
            .query("RETURN 1")
            .execute()
            .await
            .map_err(|e| OpenFangError::Memory(format!("Health check query failed: {}", e)))?;

        let has_results = !result.data.is_empty();

        Ok(has_results)
    }

    /// Executes a Cypher query against the FalkorDB graph.
    ///
    /// Returns a boolean indicating if the query executed successfully.
    /// For retrieving query results, use the `query_with_results` method.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn execute(&self, cypher: &str) -> OpenFangResult<()> {
        let mut graph = self.graph.lock().await;
        graph
            .query(cypher)
            .execute()
            .await
            .map_err(|e| OpenFangError::Memory(format!("Query failed: {}", e)))?;

        Ok(())
    }

    /// Executes a parameterized Cypher query against the FalkorDB graph.
    ///
    /// Parameters are passed as a HashMap of key-value pairs.
    /// Uses FalkorDB's native `.with_params()` method which sends parameters
    /// separately from the query, preventing Cypher injection.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn execute_with_params(
        &self,
        cypher: &str,
        params: std::collections::HashMap<String, String>,
    ) -> OpenFangResult<()> {
        let mut graph = self.graph.lock().await;

        graph
            .query(cypher)
            .with_params(&params)
            .execute()
            .await
            .map_err(|e| OpenFangError::Memory(format!("Parameterized query failed: {}", e)))?;

        Ok(())
    }

    /// Executes a Cypher query and returns the result count.
    ///
    /// Returns the number of rows returned by the query.
    ///
    /// # Errors
    /// Returns an error if the query fails.
    pub async fn query(&self, cypher: &str) -> OpenFangResult<usize> {
        let mut graph = self.graph.lock().await;
        let result = graph
            .query(cypher)
            .execute()
            .await
            .map_err(|e| OpenFangError::Memory(format!("Query failed: {}", e)))?;

        Ok(result.data.len())
    }

    /// Returns a reference to the configuration.
    pub fn config(&self) -> &config::FalkorConfig {
        &self.config
    }
}

/// Spawns a background task that runs ETL once and returns a JoinHandle.
///
/// This is a one-shot spawn that runs ETL from SurrealDB to FalkorDB once
/// and returns immediately with a JoinHandle.
///
/// # Arguments
/// * `memory` - The memory substrate to extract data from
/// * `analytics` - The FalkorAnalytics instance to load data into
///
/// # Returns
/// A JoinHandle that can be awaited to get the result of the ETL run.
pub fn spawn_etl(
    memory: Arc<dyn openfang_types::memory::Memory>,
    analytics: FalkorAnalytics,
) -> tokio::task::JoinHandle<Option<EtlReport>> {
    tokio::spawn(async move {
        info!("Starting background ETL run");

        match etl::run_etl(memory.as_ref(), &analytics).await {
            Ok(report) => {
                info!(
                    "ETL completed: {} entities, {} relations, {} memories",
                    report.entities_loaded, report.relations_loaded, report.memories_loaded
                );
                Some(report)
            }
            Err(e) => {
                error!("ETL failed: {}", e);
                None
            }
        }
    })
}

#[cfg(test)]
mod tests {

    // Unit tests can be added here when not using the embedded feature
    // Integration tests are in tests/integration.rs
}
