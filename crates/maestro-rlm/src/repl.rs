//! Python REPL execution environment via PyO3.
//!
//! HONEST NOTE: This is the only execution environment implemented.
//! The trait in lib.rs allows for others, but none exist yet.
//! The PyO3 executor requires a Python installation on the host.
//! There is NO sandboxing — the Python code runs with full host access.
//! For production use, this MUST be wrapped in OpenFang's docker_sandbox.

use crate::{ExecutionEnvironment, RlmError};

/// Python execution environment using PyO3.
pub struct Pyo3Executor;

impl Pyo3Executor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Pyo3Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEnvironment for Pyo3Executor {
    fn execute(&self, _code: &str) -> Result<String, RlmError> {
        // TODO: Port from rig-rlm's Pyo3Executor implementation
        // Key: capture stdout via io.StringIO, return errors as strings
        // (not panics) so the LLM can self-correct
        todo!("PyO3 executor implementation")
    }

    fn set_variable(&self, _name: &str, _value: &str) -> Result<(), RlmError> {
        // TODO: Set a Python variable in the REPL globals
        todo!("PyO3 variable setting")
    }

    fn env_type(&self) -> &str {
        "python-pyo3"
    }
}
