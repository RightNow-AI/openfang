//! Memory substrate for the OpenFang Agent Operating System.
//!
//! Provides a unified memory API backed by either SQLite or MongoDB.
//! The backend is selected via `MemoryConfig::backend` ("sqlite" or "mongodb").
//!
//! Storage layers:
//! - **Structured store**: Key-value pairs, sessions, agent state
//! - **Semantic store**: Text-based search with optional vector embeddings
//! - **Knowledge graph**: Entities and relations
//!
//! Agents interact with a single `Memory` trait that abstracts over all stores.

pub mod consolidation;
pub mod knowledge;
pub mod migration;
pub mod mongo;
pub mod semantic;
pub mod session;
pub mod structured;
pub mod usage;

mod substrate;
pub use substrate::MemorySubstrate;
