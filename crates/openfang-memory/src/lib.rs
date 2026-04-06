//! Memory substrate for the OpenFang Agent Operating System.
//!
//! Provides a unified memory API over three storage backends:
//! - **Structured store** (SQLite): Key-value pairs, sessions, agent state
//! - **Semantic store**: Text-based search (Phase 1: LIKE matching, Phase 2: Qdrant vectors)
//! - **Knowledge graph** (SQLite): Entities and relations
//!
//! Agents interact with a single `Memory` trait that abstracts over all three stores.

pub mod backends;
pub mod helpers;
#[cfg(feature = "http-memory")]
pub mod http;
pub mod jsonl;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "qdrant")]
pub mod qdrant;
pub mod session;
pub mod sqlite;
pub mod usage;

mod substrate;
pub use substrate::MemorySubstrate;
