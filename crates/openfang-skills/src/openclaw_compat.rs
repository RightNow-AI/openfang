//! OpenClaw skill compatibility layer.
//!
//! OpenClaw skills come in two formats:
//! 1. **Node.js/TypeScript modules** — `package.json` + `index.js` (code skills)
//! 2. **SKILL.md Markdown files** — YAML frontmatter + Markdown body (prompt-only skills)
//!
//! This module detects both formats and converts them to OpenFang `SkillManifest`.

use crate::config_injection::SkillConfigVar;
use crate::{
    SkillError, SkillManifest, SkillMeta, SkillRequirements, SkillRuntime, SkillRuntimeConfig,
    SkillSource, SkillToolDef, SkillTools,
};
use openfang_types::tool_compat;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

// ---------------------------------------------------------------------------
// SKILL.md types
// ---------------------------------------------------------------------------

/// YAML frontmatter from a SKILL.md file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SkillMdFrontmatter {
    /// Skill display name.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Nested metadata block.
    pub metadata: SkillMdMetadata,
    /// Optional declared config variables. Each entry names a runtime variable
    /// the skill expects to have resolved (from user config, env, or default)
    /// before its prompt is injected. See
    /// [`crate::config_injection`] for the resolution contract.
    pub config: HashMap<String, SkillConfigVar>,
}

/// Metadata section in SKILL.md frontmatter.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SkillMdMetadata {
    /// OpenClaw-specific metadata.
    pub openclaw: Option<OpenClawMeta>,
}

/// OpenClaw-specific metadata in SKILL.md.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OpenClawMeta {
    /// Emoji icon for the skill.
    pub emoji: Option<String>,
    /// System requirements.
    pub requires: Option<OpenClawRequires>,
    /// Commands exposed by this skill.
    pub commands: Vec<OpenClawCommand>,
}

/// System requirements declared by an OpenClaw skill.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct OpenClawRequires {
    /// Required system binaries (e.g., ["git", "gh"]).
    pub bins: Vec<String>,
    /// Required environment variables.
    pub env: Vec<String>,
}

/// A command declared by an OpenClaw skill.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OpenClawCommand {
    /// Command name (e.g., "create_pr").
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Dispatch configuration.
    pub dispatch: Option<OpenClawDispatch>,
}

/// Dispatch configuration for an OpenClaw command.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OpenClawDispatch {
    /// Whether the command can be invoked by users directly.
    pub user_invocable: bool,
    /// Whether to prevent the model from invoking this command.
    pub disable_model_invocation: bool,
}

/// Result of converting a SKILL.md into OpenFang format.
#[derive(Debug, Clone)]
pub struct ConvertedSkillMd {
    /// The generated skill manifest.
    pub manifest: SkillManifest,
    /// Markdown body (prompt context for the LLM).
    pub prompt_context: String,
    /// Tool name translations applied (openclaw_name → openfang_name).
    pub tool_translations: Vec<(String, String)>,
    /// Required system binaries.
    pub required_bins: Vec<String>,
    /// Required environment variables.
    pub required_env: Vec<String>,
    /// Declared config variables (if any). Resolved by the loader against the
    /// user's `[skills.<skill-name>]` section before the prompt is served.
    pub config_vars: HashMap<String, SkillConfigVar>,
}

// ---------------------------------------------------------------------------
// SKILL.md detection and parsing
// ---------------------------------------------------------------------------

/// Check if a directory contains a SKILL.md file.
pub fn detect_skillmd(dir: &Path) -> bool {
    dir.join("SKILL.md").exists()
}

/// Parse a SKILL.md file into frontmatter and Markdown body.
///
/// The file format is:
/// ```text
/// ---
/// name: My Skill
/// description: Does something
/// metadata:
///   openclaw:
///     commands: [...]
/// ---
/// # Markdown body
/// Instructions for the LLM...
/// ```
pub fn parse_skillmd(path: &Path) -> Result<(SkillMdFrontmatter, String), SkillError> {
    let content = std::fs::read_to_string(path)?;
    parse_skillmd_str(&content)
}

/// Parse a SKILL.md string (in-memory) into frontmatter and Markdown body.
///
/// This is the core parser, usable for both file-based and compile-time embedded skills.
pub fn parse_skillmd_str(content: &str) -> Result<(SkillMdFrontmatter, String), SkillError> {
    // Handle both \r\n and \n line endings
    let content = content.replace("\r\n", "\n");

    // Find the YAML frontmatter delimiters
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(SkillError::YamlParse(
            "SKILL.md must start with YAML frontmatter (---)".to_string(),
        ));
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let close_pos = after_first.find("\n---").ok_or_else(|| {
        SkillError::YamlParse("Missing closing --- in SKILL.md frontmatter".to_string())
    })?;

    let yaml_str = &after_first[..close_pos];
    let body_start = close_pos + 4; // skip "\n---"
    let body = after_first[body_start..].trim().to_string();

    let frontmatter: SkillMdFrontmatter = serde_yaml::from_str(yaml_str)
        .map_err(|e| SkillError::YamlParse(format!("Invalid YAML frontmatter: {e}")))?;

    Ok((frontmatter, body))
}

/// Full conversion of a SKILL.md directory to OpenFang format.
///
/// Most SKILL.md skills are prompt-only (no executable code). The Markdown body
/// is stored as `prompt_context` and injected into the LLM's system prompt.
pub fn convert_skillmd(dir: &Path) -> Result<ConvertedSkillMd, SkillError> {
    let skillmd_path = dir.join("SKILL.md");
    let (frontmatter, body) = parse_skillmd(&skillmd_path)?;

    let skill_name = if frontmatter.name.is_empty() {
        // Derive name from directory
        dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed-skill")
            .to_string()
    } else {
        frontmatter.name.clone()
    };

    let mut tool_translations = Vec::new();
    let mut required_bins = Vec::new();
    let mut required_env = Vec::new();
    let mut tools = Vec::new();

    if let Some(ref meta) = frontmatter.metadata.openclaw {
        // Extract system requirements
        if let Some(ref requires) = meta.requires {
            required_bins = requires.bins.clone();
            required_env = requires.env.clone();
        }

        // Convert commands to OpenFang tool definitions
        for cmd in &meta.commands {
            if cmd.name.is_empty() {
                continue;
            }

            // Translate tool name if it's a known OpenClaw name
            let openfang_name = if let Some(mapped) = tool_compat::map_tool_name(&cmd.name) {
                tool_translations.push((cmd.name.clone(), mapped.to_string()));
                mapped.to_string()
            } else if tool_compat::is_known_openfang_tool(&cmd.name) {
                cmd.name.clone()
            } else {
                // Custom command — keep original name, normalize to snake_case
                cmd.name.replace('-', "_")
            };

            tools.push(SkillToolDef {
                name: openfang_name,
                description: if cmd.description.is_empty() {
                    format!("Execute {} command", cmd.name)
                } else {
                    cmd.description.clone()
                },
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string", "description": "Input for the command" }
                    }
                }),
            });
        }
    }

    // Determine runtime: if no executable tools, this is prompt-only
    let runtime_type = if tools.is_empty() {
        SkillRuntime::PromptOnly
    } else {
        // Has commands but no executable entry point — still prompt-only
        // (the commands just indicate which built-in tools to use)
        SkillRuntime::PromptOnly
    };

    let config_vars = frontmatter.config.clone();

    let manifest = SkillManifest {
        skill: SkillMeta {
            name: skill_name,
            version: "0.1.0".to_string(),
            description: frontmatter.description.clone(),
            author: String::new(),
            license: String::new(),
            tags: vec!["openclaw-compat".to_string(), "prompt-only".to_string()],
        },
        runtime: SkillRuntimeConfig {
            runtime_type,
            entry: String::new(),
        },
        tools: SkillTools { provided: tools },
        requirements: SkillRequirements::default(),
        prompt_context: Some(body.clone()),
        source: Some(SkillSource::OpenClaw),
        config: config_vars.clone(),
    };

    info!(
        "Converted SKILL.md: {} ({} tools, {} translations)",
        manifest.skill.name,
        manifest.tools.provided.len(),
        tool_translations.len()
    );

    Ok(ConvertedSkillMd {
        manifest,
        prompt_context: body,
        tool_translations,
        required_bins,
        required_env,
        config_vars,
    })
}

/// Convert an in-memory SKILL.md string into OpenFang format.
///
/// Same as `convert_skillmd()` but works from a string rather than a directory.
/// Used by the bundled skills system for compile-time embedded content.
pub fn convert_skillmd_str(name_hint: &str, content: &str) -> Result<ConvertedSkillMd, SkillError> {
    let (frontmatter, body) = parse_skillmd_str(content)?;

    let skill_name = if frontmatter.name.is_empty() {
        name_hint.to_string()
    } else {
        frontmatter.name.clone()
    };

    let mut tool_translations = Vec::new();
    let mut required_bins = Vec::new();
    let mut required_env = Vec::new();
    let mut tools = Vec::new();

    if let Some(ref meta) = frontmatter.metadata.openclaw {
        if let Some(ref requires) = meta.requires {
            required_bins = requires.bins.clone();
            required_env = requires.env.clone();
        }

        for cmd in &meta.commands {
            if cmd.name.is_empty() {
                continue;
            }

            let openfang_name = if let Some(mapped) = tool_compat::map_tool_name(&cmd.name) {
                tool_translations.push((cmd.name.clone(), mapped.to_string()));
                mapped.to_string()
            } else if tool_compat::is_known_openfang_tool(&cmd.name) {
                cmd.name.clone()
            } else {
                cmd.name.replace('-', "_")
            };

            tools.push(SkillToolDef {
                name: openfang_name,
                description: if cmd.description.is_empty() {
                    format!("Execute {} command", cmd.name)
                } else {
                    cmd.description.clone()
                },
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input": { "type": "string", "description": "Input for the command" }
                    }
                }),
            });
        }
    }

    let runtime_type = SkillRuntime::PromptOnly;

    let config_vars = frontmatter.config.clone();

    let manifest = SkillManifest {
        skill: SkillMeta {
            name: skill_name,
            version: "0.1.0".to_string(),
            description: frontmatter.description.clone(),
            author: "OpenFang".to_string(),
            license: "Apache-2.0".to_string(),
            tags: vec!["bundled".to_string(), "prompt-only".to_string()],
        },
        runtime: SkillRuntimeConfig {
            runtime_type,
            entry: String::new(),
        },
        tools: SkillTools { provided: tools },
        requirements: SkillRequirements::default(),
        prompt_context: Some(body.clone()),
        source: Some(SkillSource::Bundled),
        config: config_vars.clone(),
    };

    Ok(ConvertedSkillMd {
        manifest,
        prompt_context: body,
        tool_translations,
        required_bins,
        required_env,
        config_vars,
    })
}

// ---------------------------------------------------------------------------
// Node.js / package.json detection (existing)
// ---------------------------------------------------------------------------

/// Check if a directory contains a valid OpenClaw Node.js skill.
pub fn detect_openclaw_skill(dir: &Path) -> bool {
    dir.join("package.json").exists()
        && (dir.join("index.ts").exists()
            || dir.join("index.js").exists()
            || dir.join("dist").join("index.js").exists())
}

/// Convert an OpenClaw Node.js skill directory into an OpenFang SkillManifest.
///
/// Reads package.json to extract name, version, description, and infers tool definitions.
pub fn convert_openclaw_skill(dir: &Path) -> Result<SkillManifest, SkillError> {
    let package_json_path = dir.join("package.json");
    let content = std::fs::read_to_string(&package_json_path)?;
    let pkg: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| SkillError::InvalidManifest(format!("Invalid package.json: {e}")))?;

    let name = pkg["name"].as_str().unwrap_or("unnamed-skill").to_string();
    let version = pkg["version"].as_str().unwrap_or("0.1.0").to_string();
    let description = pkg["description"].as_str().unwrap_or("").to_string();
    let author = pkg["author"].as_str().unwrap_or("").to_string();

    // Determine entry point
    let entry = if dir.join("dist").join("index.js").exists() {
        "dist/index.js".to_string()
    } else if dir.join("index.js").exists() {
        "index.js".to_string()
    } else if dir.join("index.ts").exists() {
        return Err(SkillError::RuntimeNotAvailable(
            "TypeScript skill needs to be compiled first. Run `npm run build` in the skill directory.".to_string()
        ));
    } else {
        return Err(SkillError::InvalidManifest(
            "No index.js or dist/index.js found".to_string(),
        ));
    };

    // Try to extract tool definitions from OpenClaw's skill metadata
    let tools = if let Some(openclaw) = pkg.get("openclaw") {
        extract_tools_from_openclaw_meta(openclaw)
    } else {
        vec![SkillToolDef {
            name: name.replace('-', "_"),
            description: if description.is_empty() {
                format!("Execute the {name} skill")
            } else {
                description.clone()
            },
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "Input for the skill" }
                },
                "required": ["input"]
            }),
        }]
    };

    info!("Converted OpenClaw skill: {name} ({} tools)", tools.len());

    Ok(SkillManifest {
        skill: SkillMeta {
            name,
            version,
            description,
            author,
            license: pkg["license"].as_str().unwrap_or("MIT").to_string(),
            tags: vec!["openclaw-compat".to_string()],
        },
        runtime: SkillRuntimeConfig {
            runtime_type: SkillRuntime::Node,
            entry,
        },
        tools: SkillTools { provided: tools },
        requirements: SkillRequirements::default(),
        prompt_context: None,
        source: Some(SkillSource::OpenClaw),
        config: HashMap::new(),
    })
}

/// Extract tool definitions from OpenClaw's package.json metadata.
fn extract_tools_from_openclaw_meta(meta: &serde_json::Value) -> Vec<SkillToolDef> {
    let mut tools = Vec::new();

    if let Some(tool_defs) = meta.get("tools").and_then(|t| t.as_array()) {
        for def in tool_defs {
            let name = def["name"].as_str().unwrap_or("unnamed").to_string();
            let description = def["description"].as_str().unwrap_or("").to_string();
            let input_schema = def
                .get("input_schema")
                .cloned()
                .unwrap_or(serde_json::json!({"type": "object"}));

            tools.push(SkillToolDef {
                name,
                description,
                input_schema,
            });
        }
    }

    tools
}

/// Write an OpenFang skill.toml manifest for an OpenClaw skill.
pub fn write_openfang_manifest(dir: &Path, manifest: &SkillManifest) -> Result<(), SkillError> {
    let toml_str = toml::to_string_pretty(manifest)
        .map_err(|e| SkillError::InvalidManifest(format!("TOML serialize: {e}")))?;
    std::fs::write(dir.join("skill.toml"), toml_str)?;
    Ok(())
}

/// Write the prompt context Markdown body alongside a skill.toml.
pub fn write_prompt_context(dir: &Path, content: &str) -> Result<(), SkillError> {
    std::fs::write(dir.join("prompt_context.md"), content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // --- package.json tests (existing) ---

    #[test]
    fn test_detect_openclaw_skill() {
        let dir = TempDir::new().unwrap();

        assert!(!detect_openclaw_skill(dir.path()));

        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert!(!detect_openclaw_skill(dir.path()));

        std::fs::write(dir.path().join("index.js"), "").unwrap();
        assert!(detect_openclaw_skill(dir.path()));
    }

    #[test]
    fn test_convert_openclaw_skill() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{
                "name": "test-skill",
                "version": "1.0.0",
                "description": "A test skill",
                "author": "tester",
                "license": "MIT"
            }"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("index.js"), "module.exports = {}").unwrap();

        let manifest = convert_openclaw_skill(dir.path()).unwrap();
        assert_eq!(manifest.skill.name, "test-skill");
        assert_eq!(manifest.skill.version, "1.0.0");
        assert_eq!(manifest.runtime.runtime_type, SkillRuntime::Node);
        assert_eq!(manifest.tools.provided.len(), 1);
    }

    #[test]
    fn test_convert_with_openclaw_meta() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{
                "name": "meta-skill",
                "version": "2.0.0",
                "openclaw": {
                    "tools": [
                        {
                            "name": "do_thing",
                            "description": "Does the thing",
                            "input_schema": { "type": "object", "properties": { "x": { "type": "string" } } }
                        }
                    ]
                }
            }"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("index.js"), "").unwrap();

        let manifest = convert_openclaw_skill(dir.path()).unwrap();
        assert_eq!(manifest.tools.provided.len(), 1);
        assert_eq!(manifest.tools.provided[0].name, "do_thing");
    }

    // --- SKILL.md tests ---

    #[test]
    fn test_detect_skillmd() {
        let dir = TempDir::new().unwrap();
        assert!(!detect_skillmd(dir.path()));

        std::fs::write(dir.path().join("SKILL.md"), "---\nname: test\n---\nbody").unwrap();
        assert!(detect_skillmd(dir.path()));
    }

    #[test]
    fn test_parse_skillmd_valid() {
        let dir = TempDir::new().unwrap();
        let content = r#"---
name: GitHub Helper
description: Helps with GitHub operations
metadata:
  openclaw:
    emoji: "🐙"
    commands:
      - name: create_pr
        description: Create a pull request
---
# GitHub Helper

You are an expert at GitHub operations.
Use the gh CLI to manage PRs and issues."#;

        std::fs::write(dir.path().join("SKILL.md"), content).unwrap();
        let (fm, body) = parse_skillmd(&dir.path().join("SKILL.md")).unwrap();

        assert_eq!(fm.name, "GitHub Helper");
        assert_eq!(fm.description, "Helps with GitHub operations");
        assert!(fm.metadata.openclaw.is_some());
        let meta = fm.metadata.openclaw.unwrap();
        assert_eq!(meta.emoji.as_deref(), Some("🐙"));
        assert_eq!(meta.commands.len(), 1);
        assert_eq!(meta.commands[0].name, "create_pr");
        assert!(body.contains("GitHub operations"));
    }

    #[test]
    fn test_parse_skillmd_missing_delimiters() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "no frontmatter here").unwrap();
        let result = parse_skillmd(&dir.path().join("SKILL.md"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must start with YAML frontmatter"));
    }

    #[test]
    fn test_parse_skillmd_empty_body() {
        let dir = TempDir::new().unwrap();
        let content = "---\nname: Minimal\ndescription: A minimal skill\n---\n";
        std::fs::write(dir.path().join("SKILL.md"), content).unwrap();
        let (fm, body) = parse_skillmd(&dir.path().join("SKILL.md")).unwrap();

        assert_eq!(fm.name, "Minimal");
        assert!(body.is_empty());
    }

    #[test]
    fn test_convert_skillmd_prompt_only() {
        let dir = TempDir::new().unwrap();
        let content = r#"---
name: Writing Coach
description: Helps improve writing style
---
# Writing Coach

You are an expert writing coach. When reviewing text:
1. Check grammar and punctuation
2. Suggest clearer phrasing
3. Improve paragraph structure"#;

        std::fs::write(dir.path().join("SKILL.md"), content).unwrap();
        let converted = convert_skillmd(dir.path()).unwrap();

        assert_eq!(converted.manifest.skill.name, "Writing Coach");
        assert_eq!(
            converted.manifest.runtime.runtime_type,
            SkillRuntime::PromptOnly
        );
        assert!(converted.manifest.tools.provided.is_empty());
        assert!(converted.prompt_context.contains("writing coach"));
        assert!(converted.tool_translations.is_empty());
    }

    #[test]
    fn test_convert_skillmd_with_commands() {
        let dir = TempDir::new().unwrap();
        let content = r#"---
name: Git Helper
description: Git operations
metadata:
  openclaw:
    requires:
      bins:
        - git
      env:
        - GITHUB_TOKEN
    commands:
      - name: Bash
        description: Run shell commands
      - name: Read
        description: Read files
---
# Git Helper instructions"#;

        std::fs::write(dir.path().join("SKILL.md"), content).unwrap();
        let converted = convert_skillmd(dir.path()).unwrap();

        assert_eq!(converted.manifest.skill.name, "Git Helper");
        assert_eq!(
            converted.manifest.runtime.runtime_type,
            SkillRuntime::PromptOnly
        );
        // Bash -> shell_exec, Read -> file_read
        assert_eq!(converted.tool_translations.len(), 2);
        assert!(converted
            .tool_translations
            .iter()
            .any(|(from, to)| from == "Bash" && to == "shell_exec"));
        assert!(converted
            .tool_translations
            .iter()
            .any(|(from, to)| from == "Read" && to == "file_read"));
        assert_eq!(converted.required_bins, vec!["git"]);
        assert_eq!(converted.required_env, vec!["GITHUB_TOKEN"]);
    }

    #[test]
    fn test_parse_skillmd_str() {
        let content = "---\nname: test-skill\ndescription: A test\n---\n# Test\n\nBody text here.";
        let (fm, body) = parse_skillmd_str(content).unwrap();
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.description, "A test");
        assert!(body.contains("Body text here."));
    }

    #[test]
    fn test_convert_skillmd_str() {
        let content =
            "---\nname: inline-skill\ndescription: From string\n---\n# Inline\n\nInstructions.";
        let converted = convert_skillmd_str("fallback-name", content).unwrap();
        assert_eq!(converted.manifest.skill.name, "inline-skill");
        assert_eq!(
            converted.manifest.runtime.runtime_type,
            SkillRuntime::PromptOnly
        );
        assert_eq!(converted.manifest.source, Some(SkillSource::Bundled));
        assert!(converted.prompt_context.contains("Instructions."));
    }

    #[test]
    fn test_convert_skillmd_str_uses_name_hint() {
        let content = "---\ndescription: No name field\n---\n# Body";
        let converted = convert_skillmd_str("my-hint", content).unwrap();
        assert_eq!(converted.manifest.skill.name, "my-hint");
    }

    #[test]
    fn test_parse_skillmd_with_config_frontmatter_round_trips() {
        let content = r#"---
name: github-repo-helper
description: Works with GitHub repos
config:
  github_token:
    description: GitHub personal access token
    env: GITHUB_TOKEN
    required: true
  default_branch:
    description: Default branch name
    default: main
    required: false
---
# GitHub Repo Helper

Body content here.
"#;

        let (fm, body) = parse_skillmd_str(content).unwrap();
        assert_eq!(fm.name, "github-repo-helper");
        assert_eq!(fm.config.len(), 2);

        let tok = fm
            .config
            .get("github_token")
            .expect("github_token config var present");
        assert_eq!(tok.env.as_deref(), Some("GITHUB_TOKEN"));
        assert!(tok.required);
        assert!(tok.default.is_none());

        let branch = fm
            .config
            .get("default_branch")
            .expect("default_branch config var present");
        assert_eq!(branch.default.as_deref(), Some("main"));
        assert!(!branch.required);

        // Body survives
        assert!(body.contains("Body content here"));

        // Full convert also carries config vars through
        let converted = convert_skillmd_str("github-repo-helper", content).unwrap();
        assert_eq!(converted.config_vars.len(), 2);
        assert!(converted.config_vars.contains_key("github_token"));
    }

    #[test]
    fn test_parse_skillmd_without_config_still_loads() {
        // Backward-compat: skills without a `config:` key must load as before.
        let content = "---\nname: plain\ndescription: No config\n---\n# Plain body";
        let (fm, _body) = parse_skillmd_str(content).unwrap();
        assert!(fm.config.is_empty());
    }
}
