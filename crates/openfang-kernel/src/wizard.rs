//! NL Auto-Bootstrap Wizard — generates agent configs from natural language.
//!
//! The wizard takes a user's natural language description of what they want
//! an agent to do, extracts structured intent, and generates a complete
//! agent manifest (TOML config) ready to spawn.

use openfang_types::agent::{
    AgentManifest, ManifestCapabilities, ModelConfig, Priority, ResourceQuota, ScheduleMode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The extracted intent from a user's natural language description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIntent {
    /// Agent name (slug-style).
    pub name: String,
    /// Short description.
    pub description: String,
    /// What the agent should do (summarized task).
    pub task: String,
    /// What skills/tools it needs.
    pub skills: Vec<String>,
    /// Suggested model tier (simple, medium, complex).
    pub model_tier: String,
    /// Whether it runs on a schedule.
    pub scheduled: bool,
    /// Schedule expression (cron or interval).
    pub schedule: Option<String>,
    /// Suggested capabilities.
    pub capabilities: Vec<String>,
}

/// A generated setup plan from the wizard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupPlan {
    /// The extracted intent.
    pub intent: AgentIntent,
    /// Generated agent manifest (ready to write as TOML).
    pub manifest: AgentManifest,
    /// Skills to install (if not already installed).
    pub skills_to_install: Vec<String>,
    /// Human-readable summary of what will be created.
    pub summary: String,
}

/// The setup wizard builds agent configurations from natural language.
pub struct SetupWizard;

impl SetupWizard {
    /// Build a setup plan from an extracted intent.
    ///
    /// This maps the intent into a concrete agent manifest with appropriate
    /// model configuration, capabilities, and schedule.
    pub fn build_plan(intent: AgentIntent) -> SetupPlan {
        // Map model tier to provider/model
        let (provider, model) = match intent.model_tier.as_str() {
            "simple" => ("groq", "llama-3.3-70b-versatile"),
            "complex" => ("anthropic", "claude-sonnet-4-20250514"),
            _ => ("groq", "llama-3.3-70b-versatile"), // medium default
        };

        // Build capabilities from intent
        let mut caps = ManifestCapabilities::default();
        for cap in &intent.capabilities {
            match cap.as_str() {
                "web" | "network" => caps.network.push("*".to_string()),
                "file_read" => caps.tools.push("file_read".to_string()),
                "file_write" => caps.tools.push("file_write".to_string()),
                "file" | "files" => {
                    for t in &["file_read", "file_write", "file_list"] {
                        let s = t.to_string();
                        if !caps.tools.contains(&s) {
                            caps.tools.push(s);
                        }
                    }
                }
                "shell" => caps.shell.push("*".to_string()),
                "memory" => {
                    caps.memory_read.push("*".to_string());
                    caps.memory_write.push("*".to_string());
                    for t in &["memory_store", "memory_recall"] {
                        let s = t.to_string();
                        if !caps.tools.contains(&s) {
                            caps.tools.push(s);
                        }
                    }
                }
                "browser" | "browse" => {
                    caps.network.push("*".to_string());
                    for t in &[
                        "browser_navigate",
                        "browser_click",
                        "browser_type",
                        "browser_read_page",
                        "browser_screenshot",
                        "browser_close",
                    ] {
                        let s = t.to_string();
                        if !caps.tools.contains(&s) {
                            caps.tools.push(s);
                        }
                    }
                }
                other => caps.tools.push(other.to_string()),
            }
        }

        // Add web_search + web_fetch if web/network capability is needed
        if caps.network.contains(&"*".to_string()) {
            for t in &["web_search", "web_fetch"] {
                let s = t.to_string();
                if !caps.tools.contains(&s) {
                    caps.tools.push(s);
                }
            }
        }

        // Build schedule
        let schedule = if intent.scheduled {
            if let Some(ref cron) = intent.schedule {
                ScheduleMode::Periodic { cron: cron.clone() }
            } else {
                ScheduleMode::default()
            }
        } else {
            ScheduleMode::default()
        };

        // Build system prompt — rich enough to guide the agent on its task.
        // The prompt_builder will wrap this with tool descriptions, memory protocol,
        // safety guidelines, etc. at execution time.
        let tool_hints = Self::tool_hints_for(&caps.tools);
        let system_prompt = format!(
            "You are {name}, an AI agent running inside the OpenFang Agent OS.\n\
             \n\
             YOUR TASK: {task}\n\
             \n\
             APPROACH:\n\
             - Understand the request fully before acting.\n\
             - Use your tools to accomplish the task rather than just describing what to do.\n\
             - If you need information, search for it. If you need to read a file, read it.\n\
             - Be concise in your responses. Lead with results, not process narration.\n\
             {tool_hints}",
            name = intent.name,
            task = intent.task,
            tool_hints = tool_hints,
        );

        let manifest = AgentManifest {
            name: intent.name.clone(),
            version: "0.1.0".to_string(),
            description: intent.description.clone(),
            author: "wizard".to_string(),
            module: "builtin:chat".to_string(),
            schedule,
            model: ModelConfig {
                provider: provider.to_string(),
                model: model.to_string(),
                max_tokens: 4096,
                temperature: 0.7,
                system_prompt,
                api_key_env: None,
                base_url: None,
            },
            resources: ResourceQuota::default(),
            priority: Priority::default(),
            capabilities: caps,
            tools: HashMap::new(),
            skills: intent.skills.clone(),
            mcp_servers: vec![],
            metadata: HashMap::new(),
            tags: vec![],
            routing: None,
            autonomous: None,
            pinned_model: None,
            workspace: None,
            generate_identity_files: true,
            profile: None,
            fallback_models: vec![],
            exec_policy: None,
            tool_allowlist: vec![],
            tool_blocklist: vec![],
        };

        let skills_to_install: Vec<String> = intent
            .skills
            .iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect();

        let summary = format!(
            "Agent '{}': {}\n  Model: {}/{}\n  Skills: {}\n  Schedule: {}",
            intent.name,
            intent.description,
            provider,
            model,
            if skills_to_install.is_empty() {
                "none".to_string()
            } else {
                skills_to_install.join(", ")
            },
            if intent.scheduled {
                intent.schedule.as_deref().unwrap_or("on-demand")
            } else {
                "on-demand"
            }
        );

        SetupPlan {
            intent,
            manifest,
            skills_to_install,
            summary,
        }
    }

    /// Build a short tool usage hint block for the system prompt based on granted tools.
    fn tool_hints_for(tools: &[String]) -> String {
        let mut hints = Vec::new();
        let has = |name: &str| tools.iter().any(|t| t == name);

        if has("web_search") {
            hints.push("- Use web_search to find current information on any topic.");
        }
        if has("web_fetch") {
            hints.push("- Use web_fetch to read the full content of a specific URL as markdown.");
        }
        if has("browser_navigate") {
            hints.push("- Use browser_navigate/click/type/read_page to interact with websites.");
        }
        if has("file_read") {
            hints.push("- Use file_read to examine files before modifying them.");
        }
        if has("shell_exec") {
            hints.push(
                "- Use shell_exec to run commands. Explain destructive commands before running.",
            );
        }
        if has("memory_store") {
            hints.push(
                "- Use memory_store/memory_recall to persist and retrieve important context.",
            );
        }

        if hints.is_empty() {
            String::new()
        } else {
            format!("\nKEY TOOLS:\n{}", hints.join("\n"))
        }
    }

    /// Generate a TOML string from an agent manifest.
    pub fn manifest_to_toml(manifest: &AgentManifest) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(manifest)
    }

    /// Parse an intent from a JSON string (typically LLM output).
    pub fn parse_intent(json: &str) -> Result<AgentIntent, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_intent() -> AgentIntent {
        AgentIntent {
            name: "research-bot".to_string(),
            description: "Researches topics and provides summaries".to_string(),
            task: "Search the web for information and provide concise summaries".to_string(),
            skills: vec!["web-summarizer".to_string()],
            model_tier: "medium".to_string(),
            scheduled: false,
            schedule: None,
            capabilities: vec!["web".to_string(), "memory".to_string()],
        }
    }

    #[test]
    fn test_build_plan_basic() {
        let intent = sample_intent();
        let plan = SetupWizard::build_plan(intent);

        assert_eq!(plan.manifest.name, "research-bot");
        assert_eq!(plan.manifest.model.provider, "groq");
        assert!(plan
            .manifest
            .capabilities
            .network
            .contains(&"*".to_string()));
        assert!(plan.summary.contains("research-bot"));
    }

    #[test]
    fn test_build_plan_complex_tier() {
        let mut intent = sample_intent();
        intent.model_tier = "complex".to_string();
        let plan = SetupWizard::build_plan(intent);

        assert_eq!(plan.manifest.model.provider, "anthropic");
        assert!(plan.manifest.model.model.contains("sonnet"));
    }

    #[test]
    fn test_build_plan_scheduled() {
        let mut intent = sample_intent();
        intent.scheduled = true;
        intent.schedule = Some("0 */6 * * *".to_string());
        let plan = SetupWizard::build_plan(intent);

        match &plan.manifest.schedule {
            ScheduleMode::Periodic { cron } => {
                assert_eq!(cron, "0 */6 * * *");
            }
            _ => panic!("Expected periodic schedule mode"),
        }
    }

    #[test]
    fn test_parse_intent_json() {
        let json = r#"{
            "name": "code-reviewer",
            "description": "Reviews code and suggests improvements",
            "task": "Analyze pull requests and provide feedback",
            "skills": [],
            "model_tier": "complex",
            "scheduled": false,
            "schedule": null,
            "capabilities": ["file_read"]
        }"#;

        let intent = SetupWizard::parse_intent(json).unwrap();
        assert_eq!(intent.name, "code-reviewer");
        assert_eq!(intent.model_tier, "complex");
    }

    #[test]
    fn test_manifest_to_toml() {
        let intent = sample_intent();
        let plan = SetupWizard::build_plan(intent);
        let toml = SetupWizard::manifest_to_toml(&plan.manifest);
        assert!(toml.is_ok());
        let toml_str = toml.unwrap();
        assert!(toml_str.contains("research-bot"));
    }

    #[test]
    fn test_web_tools_auto_added() {
        let intent = AgentIntent {
            name: "test".to_string(),
            description: "test".to_string(),
            task: "test".to_string(),
            skills: vec![],
            model_tier: "simple".to_string(),
            scheduled: false,
            schedule: None,
            capabilities: vec!["web".to_string()],
        };
        let plan = SetupWizard::build_plan(intent);
        assert!(plan
            .manifest
            .capabilities
            .tools
            .contains(&"web_fetch".to_string()));
        assert!(plan
            .manifest
            .capabilities
            .tools
            .contains(&"web_search".to_string()));
    }

    #[test]
    fn test_memory_tools_auto_added() {
        let intent = AgentIntent {
            name: "test".to_string(),
            description: "test".to_string(),
            task: "test".to_string(),
            skills: vec![],
            model_tier: "simple".to_string(),
            scheduled: false,
            schedule: None,
            capabilities: vec!["memory".to_string()],
        };
        let plan = SetupWizard::build_plan(intent);
        assert!(plan
            .manifest
            .capabilities
            .tools
            .contains(&"memory_store".to_string()));
        assert!(plan
            .manifest
            .capabilities
            .tools
            .contains(&"memory_recall".to_string()));
    }

    #[test]
    fn test_browser_tools_auto_added() {
        let intent = AgentIntent {
            name: "test".to_string(),
            description: "test".to_string(),
            task: "test".to_string(),
            skills: vec![],
            model_tier: "simple".to_string(),
            scheduled: false,
            schedule: None,
            capabilities: vec!["browser".to_string()],
        };
        let plan = SetupWizard::build_plan(intent);
        assert!(plan
            .manifest
            .capabilities
            .tools
            .contains(&"browser_navigate".to_string()));
        assert!(plan
            .manifest
            .capabilities
            .tools
            .contains(&"browser_click".to_string()));
        assert!(plan
            .manifest
            .capabilities
            .tools
            .contains(&"browser_read_page".to_string()));
    }

    #[test]
    fn test_wizard_system_prompt_has_task() {
        let intent = sample_intent();
        let plan = SetupWizard::build_plan(intent);
        assert!(plan.manifest.model.system_prompt.contains("YOUR TASK:"));
        assert!(plan.manifest.model.system_prompt.contains("Search the web"));
    }
}
