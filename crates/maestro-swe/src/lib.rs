//! Maestro SWE - Software Engineering Agent Executor
//!
//! This crate provides the SWE (Software Engineering) agent executor
//! that can perform file operations and execute commands.

pub mod executor;
pub mod protocol;

pub use executor::SWEAgentExecutor;
pub use protocol::{SWEAgentAction, SWEAgentEvent};
