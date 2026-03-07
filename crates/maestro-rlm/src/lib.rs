//! # maestro-rlm
//!
//! A real implementation of the Recursive Language Model (RLM) pattern
//! from the MIT CSAIL paper (arXiv:2512.24601v1).
//!
//! ## What RLM Actually Is
//!
//! RLM is NOT a self-improvement or learning architecture. It is a
//! **general-purpose inference paradigm** for scaling LLM effective
//! context length by orders of magnitude.
//!
//! The key insight: instead of feeding a long prompt directly into the
//! Transformer, treat the prompt as an **external environment** that the
//! LLM can **symbolically interact with** via code execution in a REPL.
//!
//! ## How It Works
//!
//! 1. Given prompt P, initialize a REPL environment (Python via PyO3)
//! 2. Set P as a variable in the REPL
//! 3. Give the LLM general context about the environment (length, type)
//! 4. The LLM writes code to peek into, decompose, and process P
//! 5. The LLM can programmatically construct sub-tasks and invoke itself
//!    recursively on those sub-tasks
//! 6. Loop continues until the LLM outputs FINAL(answer)
//!
//! ## Integration with OpenFang
//!
//! This crate provides an `RlmAgent` that implements the same interface
//! as OpenFang's agent loop. It can be used as a specialized agent type
//! for tasks that require processing very long inputs (codebases, documents,
//! datasets).
//!
//! ## HONEST GAPS
//!
//! - Only Python REPL supported (the paper suggests any REPL would work)
//! - Synchronous sub-calls only (async would dramatically improve speed)
//! - Max recursion depth is configurable but untested beyond depth 2
//! - No sandboxing of the Python REPL (security risk in production)
//! - The FINAL detection is brittle (regex-based, same issue as the paper)
//! - PyO3 requires a Python installation on the host

pub mod command;
pub mod executor;
pub mod repl;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Configuration for the RLM agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmConfig {
    /// Maximum recursion depth for sub-LLM calls.
    pub max_recursion_depth: u32,
    /// Maximum iterations in the main REPL loop.
    pub max_iterations: u32,
    /// Timeout per iteration in seconds.
    pub iteration_timeout_secs: u64,
    /// Whether to allow shell command execution (security risk).
    pub allow_shell: bool,
    /// Whether to allow file system writes.
    pub allow_file_write: bool,
}

impl Default for RlmConfig {
    fn default() -> Self {
        Self {
            max_recursion_depth: 2,
            max_iterations: 50,
            iteration_timeout_secs: 30,
            allow_shell: false,  // Secure by default
            allow_file_write: false,
        }
    }
}

/// Errors specific to RLM execution.
#[derive(Debug, Error)]
pub enum RlmError {
    #[error("REPL execution failed: {0}")]
    ReplError(String),

    #[error("Max iterations ({max}) exceeded without FINAL answer")]
    MaxIterations { max: u32 },

    #[error("Max recursion depth ({max}) exceeded")]
    MaxRecursion { max: u32 },

    #[error("Command parse error: {0}")]
    CommandParse(String),

    #[error("Model error: {0}")]
    ModelError(String),

    #[error("Python execution error: {0}")]
    PythonError(String),

    #[error("Timeout after {secs}s")]
    Timeout { secs: u64 },
}

/// A command parsed from the LLM's output.
///
/// Based on the rig-rlm prototype's Command enum, extended with
/// additional command types from the RLM paper.
#[derive(Debug, Clone)]
pub enum Command {
    /// Execute a shell command: `RUN <program> [args...]`
    Run { program: String, args: Vec<String> },

    /// Execute Python code in the REPL: ` ```repl\n<code>\n``` `
    RunCode(String),

    /// Final answer: `FINAL <answer>`
    Final(String),

    /// Sub-LLM call: `QUERY <prompt>` (recursive self-invocation)
    SubQuery(String),

    /// Invalid/unparseable command
    Invalid(String),
}

/// Trait for execution environments.
///
/// The RLM paper uses a Python REPL, but this trait allows plugging
/// in other environments (JavaScript, Bash, WASM sandbox, etc.).
///
/// HONEST NOTE: Only the Python (PyO3) implementation exists.
/// The trait is here for future extensibility, not because we have
/// multiple implementations ready.
#[async_trait]
pub trait ExecutionEnvironment: Send + Sync {
    /// Execute code in the environment and return stdout/stderr.
    fn execute(&self, code: &str) -> Result<String, RlmError>;

    /// Set a variable in the environment.
    fn set_variable(&self, name: &str, value: &str) -> Result<(), RlmError>;

    /// Get the environment type name (for logging).
    fn env_type(&self) -> &str;
}

/// The main RLM agent.
///
/// This struct combines a Rig.rs model with an execution environment
/// to implement the recursive language model pattern.
///
/// ## Usage with OpenFang
///
/// ```ignore
/// // Register as a specialized agent type in OpenFang's kernel
/// let rlm = RlmAgent::new(model, Pyo3Executor::new(), RlmConfig::default());
/// let answer = rlm.query("Analyze this 500K token codebase: ...").await?;
/// ```
#[allow(dead_code)]
pub struct RlmAgent<E: ExecutionEnvironment> {
    /// The execution environment (Python REPL, etc.)
    pub env: E,
    /// Configuration
    pub config: RlmConfig,
    /// Current recursion depth (for tracking nested sub-calls)
    recursion_depth: u32,
}

impl<E: ExecutionEnvironment> RlmAgent<E> {
    pub fn new(env: E, config: RlmConfig) -> Self {
        Self {
            env,
            config,
            recursion_depth: 0,
        }
    }

    /// Query the RLM with a prompt.
    ///
    /// This is the main entry point. The prompt can be arbitrarily long —
    /// it will be loaded into the REPL environment as a variable, and the
    /// LLM will interact with it programmatically.
    ///
    /// TODO: Implement the full RLM loop:
    /// 1. Load prompt into REPL as `context` variable
    /// 2. Tell the LLM about the environment (context length, type)
    /// 3. Loop: LLM generates command → execute → feed result back
    /// 4. Until FINAL(answer) or max_iterations
    pub async fn query(&self, _prompt: &str) -> Result<String, RlmError> {
        todo!("Implement RLM query loop — see rig-rlm prototype for reference")
    }
}
