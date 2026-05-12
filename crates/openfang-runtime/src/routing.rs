//! Model routing — auto-selects cheap/mid/expensive models by query complexity.
//!
//! The router scores each `CompletionRequest` based on heuristics (token count,
//! tool availability, code markers, conversation depth) and picks the cheapest
//! model that can handle the task.

use crate::llm_driver::CompletionRequest;
use openfang_types::agent::ModelRoutingConfig;

/// Task complexity tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    /// Quick lookup, greetings, simple Q&A — use the cheapest model.
    Simple,
    /// Standard conversational task — use a mid-tier model.
    Medium,
    /// Multi-step reasoning, code generation, complex analysis — use the best model.
    Complex,
}

impl std::fmt::Display for TaskComplexity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskComplexity::Simple => write!(f, "simple"),
            TaskComplexity::Medium => write!(f, "medium"),
            TaskComplexity::Complex => write!(f, "complex"),
        }
    }
}

/// Model router that selects the appropriate model based on query complexity.
#[derive(Debug, Clone)]
pub struct ModelRouter {
    config: ModelRoutingConfig,
}

impl ModelRouter {
    /// Create a new model router with the given routing configuration.
    pub fn new(config: ModelRoutingConfig) -> Self {
        Self { config }
    }

    /// Score a completion request and determine its complexity tier.
    ///
    /// Heuristics:
    /// - **Token count**: total characters in messages as a proxy for tokens
    /// - **Tool availability**: having tools suggests potential multi-step work
    /// - **Code markers**: backticks, `fn`, `def`, `class`, etc.
    /// - **Conversation depth**: more messages = more context = harder reasoning
    /// - **System prompt length**: longer prompts often imply complex tasks
    pub fn score(&self, request: &CompletionRequest) -> TaskComplexity {
        let mut score: u32 = 0;

        // 1. Total message content length (rough token proxy: ~4 chars per token)
        let total_chars: usize = request
            .messages
            .iter()
            .map(|m| m.content.text_length())
            .sum();
        let approx_tokens = (total_chars / 4) as u32;
        score += approx_tokens;

        // 2. Tool availability adds complexity
        let tool_count = request.tools.len() as u32;
        if tool_count > 0 {
            score += tool_count * 20;
        }

        // 3. Code markers in the last user message
        if let Some(last_msg) = request.messages.last() {
            let text = last_msg.content.text_content();
            let text_lower = text.to_lowercase();
            let code_markers = [
                "```",
                "fn ",
                "def ",
                "class ",
                "import ",
                "function ",
                "async ",
                "await ",
                "struct ",
                "impl ",
                "return ",
            ];
            let code_score: u32 = code_markers
                .iter()
                .filter(|marker| text_lower.contains(*marker))
                .count() as u32;
            score += code_score * 30;
        }

        // 4. Conversation depth
        let msg_count = request.messages.len() as u32;
        if msg_count > 10 {
            score += (msg_count - 10) * 15;
        }

        // 5. System prompt complexity
        if let Some(ref system) = request.system {
            let sys_len = system.len() as u32;
            if sys_len > 500 {
                score += (sys_len - 500) / 10;
            }
        }

        // Classify
        if score < self.config.simple_threshold {
            TaskComplexity::Simple
        } else if score >= self.config.complex_threshold {
            TaskComplexity::Complex
        } else {
            TaskComplexity::Medium
        }
    }

    /// Select the model name for a given complexity tier.
    pub fn model_for_complexity(&self, complexity: TaskComplexity) -> &str {
        match complexity {
            TaskComplexity::Simple => &self.config.simple_model,
            TaskComplexity::Medium => &self.config.medium_model,
            TaskComplexity::Complex => &self.config.complex_model,
        }
    }

    /// Score a request and return the selected model name + complexity.
    pub fn select_model(&self, request: &CompletionRequest) -> (TaskComplexity, String) {
        let complexity = self.score(request);
        let model = self.model_for_complexity(complexity).to_string();
        (complexity, model)
    }

    /// Validate that all configured models exist in the catalog.
    ///
    /// Returns a list of warning messages for models not found in the catalog.
    pub fn validate_models(&self, catalog: &crate::model_catalog::ModelCatalog) -> Vec<String> {
        let mut warnings = vec![];
        for model in [
            &self.config.simple_model,
            &self.config.medium_model,
            &self.config.complex_model,
        ] {
            if catalog.find_model(model).is_none() {
                warnings.push(format!("Model '{}' not found in catalog", model));
            }
        }
        warnings
    }

    /// Resolve aliases in the routing config using the catalog.
    ///
    /// For example, if "sonnet" is configured, resolves to "claude-sonnet-4-6".
    pub fn resolve_aliases(&mut self, catalog: &crate::model_catalog::ModelCatalog) {
        if let Some(resolved) = catalog.resolve_alias(&self.config.simple_model) {
            self.config.simple_model = resolved.to_string();
        }
        if let Some(resolved) = catalog.resolve_alias(&self.config.medium_model) {
            self.config.medium_model = resolved.to_string();
        }
        if let Some(resolved) = catalog.resolve_alias(&self.config.complex_model) {
            self.config.complex_model = resolved.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::message::{Message, MessageContent, Role};
    use openfang_types::tool::ToolDefinition;

    fn default_config() -> ModelRoutingConfig {
        ModelRoutingConfig {
            simple_model: "llama-3.3-70b-versatile".to_string(),
            medium_model: "claude-sonnet-4-6".to_string(),
            complex_model: "claude-opus-4-6".to_string(),
            simple_threshold: 200,
            complex_threshold: 800,
        }
    }

    fn make_request(messages: Vec<Message>, tools: Vec<ToolDefinition>) -> CompletionRequest {
        CompletionRequest {
            model: "placeholder".to_string(),
            messages,
            tools,
            max_tokens: 4096,
            temperature: 0.7,
            system: None,
            thinking: None,
        }
    }

    #[test]
    fn test_simple_greeting_routes_to_simple() {
        let router = ModelRouter::new(default_config());
        let request = make_request(
            vec![Message {
                role: Role::User,
                content: MessageContent::text("Hello!"),
                ..Default::default()
            }],
            vec![],
        );
        let (complexity, model) = router.select_model(&request);
        assert_eq!(complexity, TaskComplexity::Simple);
        assert_eq!(model, "llama-3.3-70b-versatile");
    }

    #[test]
    fn test_code_markers_increase_complexity() {
        let router = ModelRouter::new(default_config());
        let request = make_request(
            vec![Message {
                role: Role::User,
                content: MessageContent::text(
                    "Write a function that implements async file reading with struct and impl blocks:\n\
                     ```rust\nfn main() { }\n```"
                ),
                ..Default::default()
            }],
            vec![],
        );
        let complexity = router.score(&request);
        // Should be at least Medium due to code markers
        assert_ne!(complexity, TaskComplexity::Simple);
    }

    #[test]
    fn test_tools_increase_complexity() {
        let router = ModelRouter::new(default_config());
        let tools: Vec<ToolDefinition> = (0..15)
            .map(|i| ToolDefinition {
                name: format!("tool_{i}"),
                description: "A test tool".to_string(),
                input_schema: serde_json::json!({}),
            })
            .collect();
        let request = make_request(
            vec![Message {
                role: Role::User,
                content: MessageContent::text("Use the available tools to solve this problem."),
                ..Default::default()
            }],
            tools,
        );
        let complexity = router.score(&request);
        // 15 tools * 20 = 300 — should be at least Medium
        assert_ne!(complexity, TaskComplexity::Simple);
    }

    #[test]
    fn test_long_conversation_routes_higher() {
        let router = ModelRouter::new(default_config());
        // 20 messages with moderate content
        let messages: Vec<Message> = (0..20)
            .map(|i| Message {
                role: if i % 2 == 0 { Role::User } else { Role::Assistant },
                content: MessageContent::text(format!(
                    "This is message {} with enough content to add some token weight to the conversation.",
                    i
                )),
                ..Default::default()
            })
            .collect();
        let request = make_request(messages, vec![]);
        let complexity = router.score(&request);
        // Long conversation should be Medium or Complex
        assert_ne!(complexity, TaskComplexity::Simple);
    }

    #[test]
    fn test_model_for_complexity() {
        let router = ModelRouter::new(default_config());
        assert_eq!(
            router.model_for_complexity(TaskComplexity::Simple),
            "llama-3.3-70b-versatile"
        );
        assert_eq!(
            router.model_for_complexity(TaskComplexity::Medium),
            "claude-sonnet-4-6"
        );
        assert_eq!(
            router.model_for_complexity(TaskComplexity::Complex),
            "claude-opus-4-6"
        );
    }

    #[test]
    fn test_complexity_display() {
        assert_eq!(TaskComplexity::Simple.to_string(), "simple");
        assert_eq!(TaskComplexity::Medium.to_string(), "medium");
        assert_eq!(TaskComplexity::Complex.to_string(), "complex");
    }

    #[test]
    fn test_validate_models_all_found() {
        let catalog = crate::model_catalog::ModelCatalog::new();
        let config = ModelRoutingConfig {
            simple_model: "llama-3.3-70b-versatile".to_string(),
            medium_model: "claude-sonnet-4-6".to_string(),
            complex_model: "claude-opus-4-6".to_string(),
            simple_threshold: 200,
            complex_threshold: 800,
        };
        let router = ModelRouter::new(config);
        let warnings = router.validate_models(&catalog);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validate_models_unknown() {
        let catalog = crate::model_catalog::ModelCatalog::new();
        let config = ModelRoutingConfig {
            simple_model: "unknown-model".to_string(),
            medium_model: "claude-sonnet-4-6".to_string(),
            complex_model: "claude-opus-4-6".to_string(),
            simple_threshold: 200,
            complex_threshold: 800,
        };
        let router = ModelRouter::new(config);
        let warnings = router.validate_models(&catalog);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown-model"));
    }

    #[test]
    fn test_resolve_aliases() {
        let catalog = crate::model_catalog::ModelCatalog::new();
        let config = ModelRoutingConfig {
            simple_model: "llama".to_string(),
            medium_model: "sonnet".to_string(),
            complex_model: "opus".to_string(),
            simple_threshold: 200,
            complex_threshold: 800,
        };
        let mut router = ModelRouter::new(config);
        router.resolve_aliases(&catalog);
        assert_eq!(
            router.model_for_complexity(TaskComplexity::Simple),
            "llama-3.3-70b-versatile"
        );
        assert_eq!(
            router.model_for_complexity(TaskComplexity::Medium),
            "claude-sonnet-4-6"
        );
        assert_eq!(
            router.model_for_complexity(TaskComplexity::Complex),
            "claude-opus-4-6"
        );
    }

    #[test]
    fn test_system_prompt_adds_complexity() {
        let router = ModelRouter::new(default_config());
        let mut request = make_request(
            vec![Message {
                role: Role::User,
                content: MessageContent::text("Hi"),
                ..Default::default()
            }],
            vec![],
        );
        request.system = Some("A".repeat(2000)); // Long system prompt
        let complexity_with_long_system = router.score(&request);

        let mut request2 = make_request(
            vec![Message {
                role: Role::User,
                content: MessageContent::text("Hi"),
                ..Default::default()
            }],
            vec![],
        );
        request2.system = Some("Be helpful.".to_string());
        let complexity_short = router.score(&request2);

        // Long system prompt should score higher or equal
        assert!(complexity_with_long_system as u32 >= complexity_short as u32);
    }
}
