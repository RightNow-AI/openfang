//! RLM execution loop — the core recursive agent loop.

use crate::{Command, ExecutionEnvironment, RlmConfig, RlmError};
use std::fmt::Write;

#[derive(Debug, Clone)]
pub struct Turn {
    pub iteration: u32,
    pub command: String,
    pub result: String,
}

pub struct RlmLoop<E: ExecutionEnvironment> {
    pub env: E,
    pub config: RlmConfig,
    pub history: Vec<Turn>,
}

impl<E: ExecutionEnvironment> RlmLoop<E> {
    pub fn new(env: E, config: RlmConfig) -> Self {
        Self {
            env,
            config,
            history: Vec::new(),
        }
    }

    pub fn step(&mut self, iteration: u32, raw_command: &str) -> Result<StepResult, RlmError> {
        let cmd = Command::parse(raw_command);
        let result = match &cmd {
            Command::RunCode(code) => {
                if !self.config.allow_shell && code.contains("import subprocess") {
                    Err(RlmError::ReplError("Shell access disabled".to_string()))
                } else {
                    self.env.execute(code)
                }
            }
            Command::Run {
                program: _,
                args: _,
            } => {
                if !self.config.allow_shell {
                    Err(RlmError::ReplError(
                        "Shell execution disabled in secure mode".to_string(),
                    ))
                } else {
                    Ok("shell execution not implemented".to_string())
                }
            }
            Command::Final(answer) => {
                self.history.push(Turn {
                    iteration,
                    command: raw_command.to_string(),
                    result: answer.clone(),
                });
                return Ok(StepResult::Final(answer.clone()));
            }
            Command::SubQuery(prompt) => Ok(format!("[SubQuery: {}]", prompt)),
            Command::Invalid(s) => Err(RlmError::CommandParse(format!("Invalid command: {}", s))),
        };

        let result_str = match result {
            Ok(s) => s,
            Err(e) => format!("ERROR: {}", e),
        };
        self.history.push(Turn {
            iteration,
            command: raw_command.to_string(),
            result: result_str.clone(),
        });
        Ok(StepResult::Continue(result_str))
    }

    pub fn build_prompt(&self, initial_prompt: &str, context_var: &str) -> String {
        let mut prompt = String::new();
        let _ = writeln!(
            prompt,
            "You are an RLM agent. Context is in Python variable `{}`.",
            context_var
        );
        let _ = writeln!(prompt, "Context length: {} chars.", initial_prompt.len());
        let _ = writeln!(prompt, "\nAvailable commands:");
        let _ = writeln!(prompt, "  ```repl\\n<code>\\n```  — Execute Python");
        let _ = writeln!(prompt, "  FINAL <answer>       — Return final answer");
        let _ = writeln!(prompt, "  QUERY <prompt>       — Sub-LLM call");
        if !self.history.is_empty() {
            let _ = writeln!(prompt, "\n## History");
            for t in &self.history {
                let _ = writeln!(
                    prompt,
                    "Iter {}: {} => {}",
                    t.iteration, t.command, t.result
                );
            }
        }
        let _ = writeln!(prompt, "\n## Task\n{}", initial_prompt);
        prompt
    }

    pub fn compress_history(&mut self, keep_last: usize) {
        if self.history.len() > keep_last * 2 {
            let to_remove = self.history.len() - keep_last;
            let summary = format!("[Compressed {} turns]", to_remove);
            self.history.drain(0..to_remove);
            self.history.insert(
                0,
                Turn {
                    iteration: 0,
                    command: "[compressed]".to_string(),
                    result: summary,
                },
            );
        }
    }
}

#[derive(Debug)]
pub enum StepResult {
    Continue(String),
    Final(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RlmConfig;

    struct EchoEnv;
    impl ExecutionEnvironment for EchoEnv {
        fn execute(&self, code: &str) -> Result<String, RlmError> {
            Ok(format!("ECHO: {}", code))
        }
        fn set_variable(&self, _: &str, _: &str) -> Result<(), RlmError> {
            Ok(())
        }
        fn env_type(&self) -> &str {
            "echo"
        }
    }

    #[test]
    fn test_step_final() {
        let mut lp = RlmLoop::new(EchoEnv, RlmConfig::default());
        let r = lp.step(1, "FINAL The answer is 42").unwrap();
        assert!(matches!(r, StepResult::Final(s) if s.contains("42")));
    }

    #[test]
    fn test_step_code() {
        let mut lp = RlmLoop::new(EchoEnv, RlmConfig::default());
        let r = lp.step(1, "```repl\nprint('hello')\n```").unwrap();
        assert!(matches!(r, StepResult::Continue(_)));
        assert_eq!(lp.history.len(), 1);
    }

    #[test]
    fn test_history_compression() {
        let mut lp = RlmLoop::new(EchoEnv, RlmConfig::default());
        for i in 0..20u32 {
            lp.history.push(Turn {
                iteration: i,
                command: format!("cmd-{}", i),
                result: format!("res-{}", i),
            });
        }
        lp.compress_history(5);
        assert!(lp.history.len() <= 6);
    }

    #[test]
    fn test_build_prompt() {
        let lp = RlmLoop::new(EchoEnv, RlmConfig::default());
        let p = lp.build_prompt("Analyze data", "context");
        assert!(p.contains("context") && p.contains("FINAL") && p.contains("Analyze data"));
    }

    #[test]
    fn test_shell_disabled() {
        let mut lp = RlmLoop::new(
            EchoEnv,
            RlmConfig {
                allow_shell: false,
                ..Default::default()
            },
        );
        let r = lp.step(1, "RUN ls -la").unwrap();
        assert!(matches!(r, StepResult::Continue(s) if s.contains("ERROR")));
    }
}
