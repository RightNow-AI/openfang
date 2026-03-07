//! Command parsing for RLM output.
//! Port from rig-rlm's repl.rs Command enum with extensions.

use crate::Command;

impl Command {
    /// Parse an LLM output string into a Command.
    ///
    /// Supported formats:
    /// - `RUN <program> [args...]` — shell execution
    /// - ` ```repl\n<code>\n``` ` — Python code execution
    /// - `FINAL <answer>` — terminal answer
    /// - `QUERY <prompt>` — recursive sub-LLM call
    ///
    /// HONEST NOTE: This is regex-based and fragile. The RLM paper
    /// acknowledges this as a limitation. A more robust approach would
    /// use structured tool-calling (Rig.rs supports this natively).
    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();

        if trimmed.starts_with("FINAL") {
            let answer = trimmed
                .split_once(char::is_whitespace)
                .map(|x| x.1)
                .unwrap_or("")
                .to_string();
            return Command::Final(answer);
        }

        if trimmed.starts_with("RUN") {
            let parts: Vec<&str> = trimmed.split_whitespace().skip(1).collect();
            if let Some(program) = parts.first() {
                return Command::Run {
                    program: program.to_string(),
                    args: parts[1..].iter().map(|s| s.to_string()).collect(),
                };
            }
        }

        if trimmed.starts_with("QUERY") {
            let prompt = trimmed
                .split_once(char::is_whitespace)
                .map(|x| x.1)
                .unwrap_or("")
                .to_string();
            return Command::SubQuery(prompt);
        }

        if trimmed.starts_with("```repl") {
            let code = trimmed
                .trim_start_matches("```repl")
                .trim_start_matches('\n')
                .trim_end_matches("```")
                .trim_end_matches('\n')
                .to_string();
            return Command::RunCode(code);
        }

        Command::Invalid(trimmed.to_string())
    }
}
