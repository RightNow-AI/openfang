pub mod command;
pub mod executor;
pub mod repl;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Configuration for the RLM agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmConfig {
    pub max_recursion_depth: u32,
    pub max_iterations: u32,
    pub iteration_timeout_secs: u64,
    pub allow_shell: bool,
    pub allow_file_write: bool,
}

impl Default for RlmConfig {
    fn default() -> Self {
        Self {
            max_recursion_depth: 2,
            max_iterations: 50,
            iteration_timeout_secs: 30,
            allow_shell: false,
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

#[derive(Debug, Clone)]
pub enum Command {
    Run { program: String, args: Vec<String> },
    RunCode(String),
    Final(String),
    SubQuery(String),
    Invalid(String),
}

#[async_trait]
pub trait ExecutionEnvironment: Send + Sync {
    fn execute(&self, code: &str) -> Result<String, RlmError>;
    fn set_variable(&self, name: &str, value: &str) -> Result<(), RlmError>;
    fn env_type(&self) -> &str;
}

#[allow(dead_code)]
pub struct RlmAgent<E: ExecutionEnvironment> {
    pub env: E,
    pub config: RlmConfig,
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

    pub async fn query(&self, prompt: &str) -> Result<String, RlmError> {
        use crate::executor::{RlmLoop, StepResult};
        use crate::repl::Pyo3Executor;

        let env = Pyo3Executor::new();
        env.set_variable("context", prompt)?;

        let mut rlm_loop = RlmLoop::new(env, self.config.clone());
        let max = self.config.max_iterations;

        let mut current_prompt = rlm_loop.build_prompt(prompt, "context");

        for iteration in 0..max {
            // Placeholder for actual model interaction
            let llm_response = "FINAL: This is a placeholder response.".to_string();

            match rlm_loop.step(iteration, &llm_response)? {
                StepResult::Final(answer) => return Ok(answer),
                StepResult::Continue(result) => {
                    current_prompt = format!(
                        "{}\n\nLast result: {}",
                        rlm_loop.build_prompt(prompt, "context"),
                        result
                    );

                    if iteration % 10 == 9 {
                        rlm_loop.compress_history(5);
                    }
                }
            }
        }

        Err(RlmError::MaxIterations { max })
    }
}
