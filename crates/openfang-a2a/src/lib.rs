//! Agent-to-Agent (A2A) Protocol Implementation
//!
//! This crate provides the core types and runtime for the A2A protocol,
//! enabling communication between software engineering agents.

pub mod protocol;
pub mod transport;
pub mod engine;
pub mod error;

pub use error::A2AError;
pub use engine::A2AEngine;
pub use protocol::{A2AMessage, A2APayload};
pub use transport::A2ATransport;