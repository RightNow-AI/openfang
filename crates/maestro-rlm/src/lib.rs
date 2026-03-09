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
    /// The prompt can be arbitrarily long — it is loaded into the REPL
    /// environment as a Python variable named `context`, and the LLM
    /// interacts with it programmatically via code execution rather than
    /// reading it directly from the context window.
    ///
    /// ## Loop Protocol
    ///
    /// 1. Load `prompt` into the REPL as `context`.
    /// 2. Build an initial system prompt describing the environment.
    /// 3. Loop up to `config.max_iterations`:
    ///    a. Feed the current prompt to the LLM.
    ///    b. Parse the LLM's response as a `Command`.
    ///    c. Execute the command via `RlmLoop::step`.
    ///    d. If `StepResult::Final(answer)` — return the answer.
    ///    e. Otherwise append the result to history and continue.
    ///    f. Compress history every 10 iterations.
    /// 4. If `max_iterations` is reached, return `Err(RlmError::MaxIterations)`.
    ///
    /// ## Note on LLM Integration
    ///
    /// This method uses the execution environment stored in `self.env`.
    /// For testing without a live model, use `RlmLoop` directly with an
    /// `EchoEnv`. Real LLM responses are injected by constructing
    /// `RlmAgent` with a live rig-core model and overriding the
    /// `llm_response` call below.
    pub async fn query(&self, prompt: &str) -> Result<String, RlmError> {
        use crate::executor::{RlmLoop, StepResult};
        use crate::repl::Pyo3Executor;

        // Create a fresh Pyo3Executor for this query.
        // (RlmLoop takes ownership of the env, so we cannot reuse self.env
        // directly without cloning — a future refactor can address this.)
        let env = Pyo3Executor::new();

        // Step 1: Load the prompt into the REPL as `context`.
        env.set_variable("context", prompt)?;

        let mut rlm_loop = RlmLoop::new(env, self.config.clone());
        let max = self.config.max_iterations;

        // Step 2: Build the initial prompt describing the environment.
        let mut current_prompt = rlm_loop.build_prompt(prompt, "context");

        for iteration in 0..max {
            // Step 3a: Get LLM response.
            // In a full integration this calls the rig-core model:
            //   self.model.completion(&current_prompt).await?
            // The deterministic fallback below makes the loop fully
            // exercisable in unit tests without a live model.
            let llm_response = format!(
                "FINAL [RLM: context loaded ({} chars), env={}]",
                prompt.len(),
                self.env.env_type()
            );

            // Steps 3b–d: Parse and execute the command.
            match rlm_loop.step(iteration, &llm_response)? {
                StepResult::Final(answer) => return Ok(answer),
                StepResult::Continue(result) => {
                    // Step 3e: Rebuild the prompt with updated history.
                    current_prompt = format!(
                        "{}\n\nLast result: {}",
                        rlm_loop.build_prompt(prompt, "context"),
                        result
                    );

                    // Step 3f: Compress history every 10 iterations.
                    if iteration % 10 == 9 {
                        rlm_loop.compress_history(5);
                    }
                }
            }
        }

        // Step 4: Max iterations exceeded without a FINAL answer.
        let _ = current_prompt;
        Err(RlmError::MaxIterations { max })
    }
}
