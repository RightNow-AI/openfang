//! Memory substrate for the OpenFang Agent Operating System.
//!
//! Provides a unified memory API backed by SurrealDB v3.
//! All memory fragments, entities, relations, sessions, usage records,
//! and agent state are stored in a single SurrealDB instance.
//!
//! Agents interact with a single `Memory` trait (defined in `openfang-types`)
//! that abstracts over the storage backend.

pub mod session;
pub mod substrate;
pub mod usage;

pub use session::Session;
pub use substrate::MemorySubstrate;
pub use usage::SurrealUsageStore;

/// Backward-compatible alias — legacy code that referenced `UsageStore`
/// (the former SQLite implementation) now resolves to `SurrealUsageStore`.
pub type UsageStore = SurrealUsageStore;
