//! TELOS context management — deep user identity for personalization.
//!
//! From PAI v4.0.3: 10 markdown files capturing user identity, goals,
//! projects, beliefs, preferences, habits, relationships, health,
//! finances, and calendar.

use crate::TelosContext;
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::path::Path;

/// The 10 standard TELOS context keys from PAI v4.0.3.
pub const TELOS_KEYS: &[&str] = &[
    "MISSION", "GOALS", "PROJECTS", "BELIEFS", "PREFERENCES",
    "HABITS", "RELATIONSHIPS", "HEALTH", "FINANCES", "CALENDAR",
];

impl TelosContext {
    /// Create an empty TELOS context.
    pub fn empty() -> Self {
        Self {
            entries: HashMap::new(),
            last_updated: Utc::now(),
        }
    }

    /// Load TELOS context from a directory of markdown files.
    pub fn load_from_dir(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let mut entries = HashMap::new();
        for key in TELOS_KEYS {
            let path = dir.join(format!("{}.md", key.to_lowercase()));
            if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                entries.insert(key.to_string(), content);
            }
        }
        Ok(Self { entries, last_updated: Utc::now() })
    }

    /// Get a specific TELOS entry.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    /// Set a TELOS entry.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.insert(key.into(), value.into());
        self.last_updated = Utc::now();
    }

    /// Build a condensed context string for LLM prompts.
    pub fn to_prompt_context(&self) -> String {
        let mut parts = Vec::new();
        for key in TELOS_KEYS {
            if let Some(value) = self.entries.get(*key) {
                let summary = value.lines().take(3).collect::<Vec<_>>().join(" ");
                parts.push(format!("**{}**: {}", key, summary));
            }
        }
        parts.join("\n")
    }

    /// Check if the context has all required keys.
    pub fn is_complete(&self) -> bool {
        TELOS_KEYS.iter().all(|k| self.entries.contains_key(*k))
    }

    /// Count populated entries.
    pub fn populated_count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context() {
        let ctx = TelosContext::empty();
        assert_eq!(ctx.populated_count(), 0);
        assert!(!ctx.is_complete());
    }

    #[test]
    fn test_set_and_get() {
        let mut ctx = TelosContext::empty();
        ctx.set("MISSION", "Build great software");
        assert_eq!(ctx.get("MISSION"), Some("Build great software"));
        assert_eq!(ctx.get("GOALS"), None);
    }

    #[test]
    fn test_to_prompt_context() {
        let mut ctx = TelosContext::empty();
        ctx.set("MISSION", "Build great software\nHelp others learn");
        ctx.set("GOALS", "Ship Phase 8 by end of week");
        let prompt = ctx.to_prompt_context();
        assert!(prompt.contains("MISSION"));
        assert!(prompt.contains("GOALS"));
    }

    #[test]
    fn test_is_complete() {
        let mut ctx = TelosContext::empty();
        for key in TELOS_KEYS {
            ctx.set(*key, format!("{} content", key));
        }
        assert!(ctx.is_complete());
    }
}
