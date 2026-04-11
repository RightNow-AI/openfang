//! Bundled skills — compile-time embedded SKILL.md files.
//!
//! Ships 60 prompt-only skills inside the OpenFang binary via `include_str!()`.
//! User-installed skills with the same name override bundled ones.

use crate::openclaw_compat::convert_skillmd_str;
use crate::SkillManifest;

/// Return all bundled (name, raw SKILL.md content) pairs.
pub fn bundled_skills() -> Vec<(&'static str, &'static str)> {
    vec![
        // Tier 1 (8)
        ("github", include_str!("../bundled/github/SKILL.md")),
        ("docker", include_str!("../bundled/docker/SKILL.md")),
        ("web-search", include_str!("../bundled/web-search/SKILL.md")),
        (
            "code-reviewer",
            include_str!("../bundled/code-reviewer/SKILL.md"),
        ),
        (
            "sql-analyst",
            include_str!("../bundled/sql-analyst/SKILL.md"),
        ),
        ("git-expert", include_str!("../bundled/git-expert/SKILL.md")),
        ("sysadmin", include_str!("../bundled/sysadmin/SKILL.md")),
        (
            "writing-coach",
            include_str!("../bundled/writing-coach/SKILL.md"),
        ),
        // Tier 2 (6)
        ("kubernetes", include_str!("../bundled/kubernetes/SKILL.md")),
        ("terraform", include_str!("../bundled/terraform/SKILL.md")),
        ("aws", include_str!("../bundled/aws/SKILL.md")),
        ("jira", include_str!("../bundled/jira/SKILL.md")),
        (
            "data-analyst",
            include_str!("../bundled/data-analyst/SKILL.md"),
        ),
        ("api-tester", include_str!("../bundled/api-tester/SKILL.md")),
        // Tier 3 (6)
        ("pdf-reader", include_str!("../bundled/pdf-reader/SKILL.md")),
        (
            "slack-tools",
            include_str!("../bundled/slack-tools/SKILL.md"),
        ),
        ("notion", include_str!("../bundled/notion/SKILL.md")),
        ("sentry", include_str!("../bundled/sentry/SKILL.md")),
        ("mongodb", include_str!("../bundled/mongodb/SKILL.md")),
        (
            "regex-expert",
            include_str!("../bundled/regex-expert/SKILL.md"),
        ),
        // Tier 4 — Wave 1 (20)
        ("ci-cd", include_str!("../bundled/ci-cd/SKILL.md")),
        ("ansible", include_str!("../bundled/ansible/SKILL.md")),
        ("prometheus", include_str!("../bundled/prometheus/SKILL.md")),
        ("nginx", include_str!("../bundled/nginx/SKILL.md")),
        (
            "rust-expert",
            include_str!("../bundled/rust-expert/SKILL.md"),
        ),
        (
            "python-expert",
            include_str!("../bundled/python-expert/SKILL.md"),
        ),
        (
            "typescript-expert",
            include_str!("../bundled/typescript-expert/SKILL.md"),
        ),
        (
            "react-expert",
            include_str!("../bundled/react-expert/SKILL.md"),
        ),
        (
            "postgres-expert",
            include_str!("../bundled/postgres-expert/SKILL.md"),
        ),
        (
            "redis-expert",
            include_str!("../bundled/redis-expert/SKILL.md"),
        ),
        (
            "security-audit",
            include_str!("../bundled/security-audit/SKILL.md"),
        ),
        (
            "prompt-engineer",
            include_str!("../bundled/prompt-engineer/SKILL.md"),
        ),
        (
            "technical-writer",
            include_str!("../bundled/technical-writer/SKILL.md"),
        ),
        (
            "shell-scripting",
            include_str!("../bundled/shell-scripting/SKILL.md"),
        ),
        (
            "golang-expert",
            include_str!("../bundled/golang-expert/SKILL.md"),
        ),
        ("gcp", include_str!("../bundled/gcp/SKILL.md")),
        ("azure", include_str!("../bundled/azure/SKILL.md")),
        ("helm", include_str!("../bundled/helm/SKILL.md")),
        (
            "linear-tools",
            include_str!("../bundled/linear-tools/SKILL.md"),
        ),
        (
            "crypto-expert",
            include_str!("../bundled/crypto-expert/SKILL.md"),
        ),
        // Tier 5 — Wave 2 (20)
        (
            "nextjs-expert",
            include_str!("../bundled/nextjs-expert/SKILL.md"),
        ),
        ("css-expert", include_str!("../bundled/css-expert/SKILL.md")),
        (
            "linux-networking",
            include_str!("../bundled/linux-networking/SKILL.md"),
        ),
        (
            "elasticsearch",
            include_str!("../bundled/elasticsearch/SKILL.md"),
        ),
        (
            "graphql-expert",
            include_str!("../bundled/graphql-expert/SKILL.md"),
        ),
        (
            "sqlite-expert",
            include_str!("../bundled/sqlite-expert/SKILL.md"),
        ),
        (
            "data-pipeline",
            include_str!("../bundled/data-pipeline/SKILL.md"),
        ),
        ("compliance", include_str!("../bundled/compliance/SKILL.md")),
        (
            "oauth-expert",
            include_str!("../bundled/oauth-expert/SKILL.md"),
        ),
        ("confluence", include_str!("../bundled/confluence/SKILL.md")),
        (
            "figma-expert",
            include_str!("../bundled/figma-expert/SKILL.md"),
        ),
        (
            "presentation",
            include_str!("../bundled/presentation/SKILL.md"),
        ),
        (
            "email-writer",
            include_str!("../bundled/email-writer/SKILL.md"),
        ),
        (
            "interview-prep",
            include_str!("../bundled/interview-prep/SKILL.md"),
        ),
        (
            "project-manager",
            include_str!("../bundled/project-manager/SKILL.md"),
        ),
        (
            "ml-engineer",
            include_str!("../bundled/ml-engineer/SKILL.md"),
        ),
        (
            "llm-finetuning",
            include_str!("../bundled/llm-finetuning/SKILL.md"),
        ),
        ("vector-db", include_str!("../bundled/vector-db/SKILL.md")),
        (
            "openapi-expert",
            include_str!("../bundled/openapi-expert/SKILL.md"),
        ),
        (
            "wasm-expert",
            include_str!("../bundled/wasm-expert/SKILL.md"),
        ),
    ]
}

/// Parse a bundled SKILL.md into a `SkillManifest`.
pub fn parse_bundled(name: &str, content: &str) -> Result<SkillManifest, crate::SkillError> {
    let converted = convert_skillmd_str(name, content)?;
    Ok(converted.manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundled_skills_count() {
        let skills = bundled_skills();
        assert_eq!(skills.len(), 60, "Expected 60 bundled skills");
    }

    #[test]
    fn test_all_bundled_skills_parse() {
        let skills = bundled_skills();
        for (name, content) in &skills {
            let result = parse_bundled(name, content);
            assert!(
                result.is_ok(),
                "Failed to parse bundled skill '{}': {:?}",
                name,
                result.err()
            );
            let manifest = result.unwrap();
            assert!(
                !manifest.skill.name.is_empty(),
                "Bundled skill '{}' has empty name",
                name
            );
            assert!(
                !manifest.skill.description.is_empty(),
                "Bundled skill '{}' has empty description",
                name
            );
            assert!(
                manifest.prompt_context.is_some(),
                "Bundled skill '{}' has no prompt context",
                name
            );
            assert_eq!(
                manifest.source,
                Some(crate::SkillSource::Bundled),
                "Bundled skill '{}' should have Bundled source",
                name
            );
        }
    }

    #[test]
    fn test_bundled_skills_pass_security_scan() {
        use crate::verify::SkillVerifier;

        let skills = bundled_skills();
        for (name, content) in &skills {
            let manifest = parse_bundled(name, content).unwrap();
            if let Some(ref ctx) = manifest.prompt_context {
                let warnings = SkillVerifier::scan_prompt_content(ctx);
                let has_critical = warnings
                    .iter()
                    .any(|w| matches!(w.severity, crate::verify::WarningSeverity::Critical));
                assert!(
                    !has_critical,
                    "Bundled skill '{}' has critical security warnings: {:?}",
                    name, warnings
                );
            }
        }
    }

    #[test]
    fn test_user_skill_overrides_bundled() {
        use crate::registry::SkillRegistry;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let mut registry = SkillRegistry::new(dir.path().to_path_buf());

        // Load bundled
        let bundled_count = registry.load_bundled();
        assert!(bundled_count > 0);

        // Create a user skill with the same name as a bundled one
        let skill_dir = dir.path().join("github");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("skill.toml"),
            r#"
[skill]
name = "github"
version = "99.0.0"
description = "User-customized GitHub skill"

[runtime]
type = "promptonly"
entry = ""
"#,
        )
        .unwrap();

        // Load user skills — should override the bundled one
        registry.load_all().unwrap();

        let skill = registry.get("github").unwrap();
        assert_eq!(
            skill.manifest.skill.version, "99.0.0",
            "User skill should override bundled skill"
        );
    }
}
