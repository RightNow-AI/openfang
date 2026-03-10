//! `openfang-mesh` — Multi-agent mesh routing for OpenFang.
//!
//! This crate provides two primary types:
//!
//! - [`MeshRouter`] — given a task and a set of required capabilities, selects
//!   the best execution target from the available pool: active Hands, local
//!   agents, remote OFP peers, or a new agent spawn.
//!
//! - [`MeshClient`] — thin async client for sending tasks to remote OFP peers
//!   via the wire protocol.
//!
//! ## Routing priority
//!
//! ```text
//! 1. Active Hand with matching tool/capability tags
//! 2. Running local agent with matching tags
//! 3. Remote OFP peer with matching agents
//! 4. Spawn new agent (fallback)
//! ```
//!
//! This ordering ensures that pre-configured, specialised Hands are preferred
//! over generic agents, and that local resources are preferred over remote
//! peers to minimise latency.

pub mod client;
pub mod router;

pub use client::MeshClient;
pub use router::{ExecutionTarget, MeshRouter, MeshRouterConfig};

/// Errors produced by the mesh layer.
#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    /// No suitable execution target could be found.
    #[error("No suitable execution target found for capabilities: {0:?}")]
    NoTargetFound(Vec<String>),

    /// A remote peer returned an error.
    #[error("Remote peer error: {0}")]
    RemotePeer(String),

    /// Wire protocol error.
    #[error("Wire protocol error: {0}")]
    Wire(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
