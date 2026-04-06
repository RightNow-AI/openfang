//! HTTP backend for the OpenFang memory layer.
//!
//! Routes semantic operations (remember/recall) to a remote memory-api gateway.

mod semantic;
pub use semantic::{HttpSemanticStore, MemoryApiClient, MemoryApiError};
