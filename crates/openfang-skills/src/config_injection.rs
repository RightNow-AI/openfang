//! Skill config injection at runtime.
//!
//! Skills may declare a `config` section in their SKILL.md frontmatter that
//! names variables the skill needs at runtime (API tokens, default branches,
//! endpoint URLs, etc.). This module resolves those variables from three
//! layers in priority order:
//!
//! 1. User-supplied config (e.g., `[skills.<skill-name>]` in `~/.openfang/config.toml`)
//! 2. Environment variable named by `var.env`
//! 3. `var.default`
//!
//! If a variable is marked `required` and none of the three sources produces a
//! value, [`resolve_skill_config`] returns [`SkillConfigError::MissingRequired`]
//! so the loader can refuse to register the skill instead of silently handing
//! the agent a broken prompt.
//!
//! [`render_config_block`] turns the resolved map into a human-readable block
//! that the loader appends to the skill's Markdown body before it is injected
//! into the LLM system prompt. Secret-looking variable names (matching
//! `*_token`, `*_key`, `*_secret`, or `password`) are redacted in the rendered
//! output. The underlying resolved map keeps full values so the skill runtime
//! can still use them.

use std::collections::HashMap;

/// A single config variable declared in a SKILL.md frontmatter `config:` block.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct SkillConfigVar {
    /// Human description of the variable (shown to the LLM).
    pub description: String,
    /// Environment variable name to read from when no user config is set.
    pub env: Option<String>,
    /// Default value used when neither user config nor env is set.
    pub default: Option<String>,
    /// If true, [`resolve_skill_config`] returns an error when nothing resolves.
    pub required: bool,
}

/// Errors produced while resolving a skill's config.
#[derive(Debug, thiserror::Error)]
pub enum SkillConfigError {
    /// A variable was marked `required: true` but no value was found in any
    /// of the user config, environment, or default slots.
    #[error("Required skill config variable not set: {0}")]
    MissingRequired(String),
    /// Filesystem / environment IO failure while resolving a variable.
    #[error("IO error while resolving skill config: {0}")]
    IoError(#[from] std::io::Error),
}

/// Resolve a skill's declared config variables into a concrete map.
///
/// Resolution order per variable:
/// 1. `user_config[name]` — usually from the `[skills.<skill-name>]` section
///    of `~/.openfang/config.toml`, passed in by the caller.
/// 2. `std::env::var(var.env)` if `var.env` is set.
/// 3. `var.default` if set.
///
/// If a `required` variable resolves to none of the above,
/// [`SkillConfigError::MissingRequired`] is returned with the variable name.
///
/// Non-required variables simply do not appear in the output map if they
/// resolve to nothing.
pub fn resolve_skill_config(
    vars: &HashMap<String, SkillConfigVar>,
    user_config: &HashMap<String, String>,
) -> Result<HashMap<String, String>, SkillConfigError> {
    let mut resolved = HashMap::with_capacity(vars.len());

    for (name, var) in vars {
        // 1. User config (highest priority)
        if let Some(value) = user_config.get(name) {
            resolved.insert(name.clone(), value.clone());
            continue;
        }

        // 2. Environment variable
        if let Some(env_name) = &var.env {
            if let Ok(value) = std::env::var(env_name) {
                if !value.is_empty() {
                    resolved.insert(name.clone(), value);
                    continue;
                }
            }
        }

        // 3. Default value
        if let Some(default) = &var.default {
            resolved.insert(name.clone(), default.clone());
            continue;
        }

        // 4. Not resolved — error if required
        if var.required {
            return Err(SkillConfigError::MissingRequired(name.clone()));
        }
        // Otherwise silently drop — downstream code treats absence as "unset".
    }

    Ok(resolved)
}

/// Return true if a config variable name looks like it holds a secret.
///
/// Names matching `*_token`, `*_key`, `*_secret`, or exactly `password` (case
/// insensitive) are considered secret and will be redacted by
/// [`render_config_block`]. This rule is intentionally defined in exactly
/// one place so every consumer redacts the same way.
pub fn is_secret_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower == "password" {
        return true;
    }
    lower.ends_with("_token")
        || lower.ends_with("_key")
        || lower.ends_with("_secret")
        || lower.ends_with("password")
}

/// Redact a value when its name looks secret. Non-secret values pass through.
///
/// The redaction keeps a short non-secret prefix hint so the user can tell
/// "this was set" from "this was missing", without leaking the full secret.
fn redact_value(name: &str, value: &str) -> String {
    if !is_secret_name(name) {
        return value.to_string();
    }
    // Keep up to 4 leading chars then replace the rest with a sentinel so it's
    // obvious to the LLM that the secret has been redacted and it shouldn't
    // try to echo the value back.
    let prefix: String = value.chars().take(4).collect();
    if prefix.is_empty() {
        "***redacted***".to_string()
    } else {
        format!("{prefix}***redacted***")
    }
}

/// Render a resolved config map into a Markdown block suitable for injection
/// into a skill's prompt body.
///
/// Secret-looking names (see [`is_secret_name`]) are redacted in the output.
/// The raw input map is NOT mutated — callers that need full values for the
/// skill runtime should continue to use the input map directly.
///
/// Empty input produces an empty string so callers can concatenate safely.
pub fn render_config_block(resolved: &HashMap<String, String>) -> String {
    if resolved.is_empty() {
        return String::new();
    }

    // Sort by name for deterministic output.
    let mut keys: Vec<&String> = resolved.keys().collect();
    keys.sort();

    let mut out =
        String::from("[Skill config from ~/.openfang/config.toml:\n");
    for key in keys {
        let raw = &resolved[key];
        let shown = redact_value(key, raw);
        out.push_str("  ");
        out.push_str(key);
        out.push_str(": ");
        out.push_str(&shown);
        out.push('\n');
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(env: Option<&str>, default: Option<&str>, required: bool) -> SkillConfigVar {
        SkillConfigVar {
            description: "test".to_string(),
            env: env.map(str::to_string),
            default: default.map(str::to_string),
            required,
        }
    }

    #[test]
    fn resolve_user_config_takes_priority() {
        let mut vars = HashMap::new();
        vars.insert(
            "default_branch".to_string(),
            var(Some("SHOULD_NOT_READ"), Some("main"), false),
        );

        let mut user = HashMap::new();
        user.insert("default_branch".to_string(), "develop".to_string());

        let resolved = resolve_skill_config(&vars, &user).unwrap();
        assert_eq!(resolved.get("default_branch").unwrap(), "develop");
    }

    #[test]
    fn resolve_env_when_no_user_config() {
        // Use a random-looking env var name to minimise collisions with the
        // host environment running the test.
        let env_name = "OPENFANG_TEST_CFG_ENV_7f3a";
        // SAFETY: the env var name is unique to this test.
        unsafe { std::env::set_var(env_name, "from-env") };

        let mut vars = HashMap::new();
        vars.insert(
            "some_key".to_string(),
            var(Some(env_name), Some("fallback"), false),
        );

        let user = HashMap::new();
        let resolved = resolve_skill_config(&vars, &user).unwrap();
        assert_eq!(resolved.get("some_key").unwrap(), "from-env");

        unsafe { std::env::remove_var(env_name) };
    }

    #[test]
    fn resolve_default_when_no_user_no_env() {
        let mut vars = HashMap::new();
        vars.insert(
            "default_branch".to_string(),
            var(Some("OPENFANG_UNSET_9z1"), Some("main"), false),
        );

        let user = HashMap::new();
        let resolved = resolve_skill_config(&vars, &user).unwrap();
        assert_eq!(resolved.get("default_branch").unwrap(), "main");
    }

    #[test]
    fn resolve_missing_required_returns_error() {
        let mut vars = HashMap::new();
        vars.insert(
            "github_token".to_string(),
            var(Some("OPENFANG_UNSET_a2b3"), None, true),
        );

        let user = HashMap::new();
        let err = resolve_skill_config(&vars, &user).unwrap_err();
        match err {
            SkillConfigError::MissingRequired(name) => assert_eq!(name, "github_token"),
            e => panic!("expected MissingRequired, got {e:?}"),
        }
    }

    #[test]
    fn resolve_missing_non_required_is_dropped() {
        let mut vars = HashMap::new();
        vars.insert(
            "optional".to_string(),
            var(Some("OPENFANG_UNSET_c4d5"), None, false),
        );

        let user = HashMap::new();
        let resolved = resolve_skill_config(&vars, &user).unwrap();
        assert!(!resolved.contains_key("optional"));
    }

    #[test]
    fn render_redacts_secrets_but_not_plain_vars() {
        let mut resolved = HashMap::new();
        resolved.insert("github_token".to_string(), "ghp_abcdef1234".to_string());
        resolved.insert("default_branch".to_string(), "main".to_string());

        let block = render_config_block(&resolved);
        // Secret redacted
        assert!(
            !block.contains("ghp_abcdef1234"),
            "secret token leaked into rendered block:\n{block}"
        );
        assert!(block.contains("redacted"), "expected redaction marker");
        // Non-secret preserved
        assert!(block.contains("default_branch: main"));
        // Format marker present
        assert!(block.contains("[Skill config"));
    }

    #[test]
    fn render_raw_map_retains_full_secret_values() {
        let mut vars = HashMap::new();
        vars.insert(
            "api_key".to_string(),
            var(None, Some("sk-live-secret-value"), true),
        );
        let user = HashMap::new();

        let resolved = resolve_skill_config(&vars, &user).unwrap();
        // Raw map must still contain the full value — only rendering redacts.
        assert_eq!(resolved.get("api_key").unwrap(), "sk-live-secret-value");

        let block = render_config_block(&resolved);
        assert!(
            !block.contains("sk-live-secret-value"),
            "full secret must not appear in rendered block"
        );
    }

    #[test]
    fn render_empty_produces_empty_string() {
        let map = HashMap::new();
        assert_eq!(render_config_block(&map), "");
    }

    #[test]
    fn is_secret_name_matches_suffixes() {
        assert!(is_secret_name("github_token"));
        assert!(is_secret_name("api_key"));
        assert!(is_secret_name("client_secret"));
        assert!(is_secret_name("password"));
        assert!(is_secret_name("GITHUB_TOKEN"));
        assert!(is_secret_name("stripePassword"));

        assert!(!is_secret_name("default_branch"));
        assert!(!is_secret_name("endpoint"));
        assert!(!is_secret_name("username"));
        assert!(!is_secret_name("timeout_seconds"));
    }

    #[test]
    fn render_output_is_deterministic() {
        let mut resolved = HashMap::new();
        resolved.insert("b".to_string(), "2".to_string());
        resolved.insert("a".to_string(), "1".to_string());
        resolved.insert("c".to_string(), "3".to_string());

        let first = render_config_block(&resolved);
        let second = render_config_block(&resolved);
        assert_eq!(first, second);
        // Sorted order
        let pos_a = first.find("a:").unwrap();
        let pos_b = first.find("b:").unwrap();
        let pos_c = first.find("c:").unwrap();
        assert!(pos_a < pos_b && pos_b < pos_c);
    }
}
