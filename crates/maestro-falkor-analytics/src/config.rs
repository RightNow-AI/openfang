//! Configuration for the FalkorDB Analytics Engine.

use openfang_types::error::{OpenFangError, OpenFangResult};
use serde::Deserialize;
use std::env;

/// Configuration for connecting to FalkorDB.
///
/// This struct holds all necessary configuration parameters to connect
/// to a FalkorDB instance. Values can be loaded from environment variables.
#[derive(Debug, Clone, Deserialize)]
pub struct FalkorConfig {
    /// The database URL for FalkorDB.
    ///
    /// Supported formats:
    /// - `falkor://host:port` - TCP connection
    /// - `falkor+unix:///path/to/socket` - Unix socket
    /// - `falkor-embedded` - Embedded mode (requires redis-server + falkordb.so)
    pub database_url: String,

    /// The name of the graph to use in FalkorDB.
    pub graph_name: String,
}

impl FalkorConfig {
    /// Loads configuration from environment variables.
    ///
    /// Reads the following environment variables:
    /// - `FALKOR_DATABASE_URL` - The FalkorDB connection URL
    /// - `FALKOR_GRAPH_NAME` - The name of the graph (defaults to "main")
    ///
    /// # Errors
    /// Returns an error if required environment variables are missing.
    pub fn from_env() -> OpenFangResult<Self> {
        let database_url = env::var("FALKOR_DATABASE_URL").map_err(|_| {
            OpenFangError::Config("FALKOR_DATABASE_URL environment variable not set".into())
        })?;

        let graph_name = env::var("FALKOR_GRAPH_NAME").unwrap_or_else(|_| "main".into());

        Ok(Self {
            database_url,
            graph_name,
        })
    }

    /// Creates a configuration for testing purposes.
    #[cfg(test)]
    pub fn test_config() -> Self {
        Self {
            database_url: "falkor-embedded".to_string(),
            graph_name: "test_graph".to_string(),
        }
    }
}
