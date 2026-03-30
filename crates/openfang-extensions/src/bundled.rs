//! Compile-time embedded integration templates.
//!
//! All 25 integration TOML files are baked into the binary via `include_str!()`,
//! ensuring they ship with every OpenFang build with zero filesystem dependencies.

/// Returns all bundled integration templates as `(id, TOML content)` pairs.
pub fn bundled_integrations() -> Vec<(&'static str, &'static str)> {
    vec![
        // ── DevTools (6) ────────────────────────────────────────────────────
        ("github", include_str!("../integrations/github.toml")),
        ("gitlab", include_str!("../integrations/gitlab.toml")),
        ("linear", include_str!("../integrations/linear.toml")),
        ("jira", include_str!("../integrations/jira.toml")),
        ("bitbucket", include_str!("../integrations/bitbucket.toml")),
        ("sentry", include_str!("../integrations/sentry.toml")),
        // ── Productivity (6) ────────────────────────────────────────────────
        (
            "google-calendar",
            include_str!("../integrations/google-calendar.toml"),
        ),
        ("gmail", include_str!("../integrations/gmail.toml")),
        ("notion", include_str!("../integrations/notion.toml")),
        ("todoist", include_str!("../integrations/todoist.toml")),
        (
            "google-drive",
            include_str!("../integrations/google-drive.toml"),
        ),
        ("dropbox", include_str!("../integrations/dropbox.toml")),
        // ── Communication (3) ───────────────────────────────────────────────
        ("slack", include_str!("../integrations/slack.toml")),
        (
            "discord-mcp",
            include_str!("../integrations/discord-mcp.toml"),
        ),
        ("teams-mcp", include_str!("../integrations/teams-mcp.toml")),
        // ── Data (5) ────────────────────────────────────────────────────────
        (
            "postgresql",
            include_str!("../integrations/postgresql.toml"),
        ),
        (
            "sqlite-mcp",
            include_str!("../integrations/sqlite-mcp.toml"),
        ),
        ("mongodb", include_str!("../integrations/mongodb.toml")),
        ("redis", include_str!("../integrations/redis.toml")),
        (
            "elasticsearch",
            include_str!("../integrations/elasticsearch.toml"),
        ),
        // ── Cloud (3) ───────────────────────────────────────────────────────
        ("aws", include_str!("../integrations/aws.toml")),
        ("gcp-mcp", include_str!("../integrations/gcp-mcp.toml")),
        ("azure-mcp", include_str!("../integrations/azure-mcp.toml")),
        // ── AI & Search (2) ─────────────────────────────────────────────────
        (
            "brave-search",
            include_str!("../integrations/brave-search.toml"),
        ),
        (
            "exa-search",
            include_str!("../integrations/exa-search.toml"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IntegrationTemplate;

    #[test]
    fn bundled_count() {
        assert_eq!(bundled_integrations().len(), 25);
    }

    #[test]
    fn all_bundled_parse() {
        for (id, content) in bundled_integrations() {
            let t: IntegrationTemplate = toml::from_str(content)
                .unwrap_or_else(|e| panic!("Failed to parse '{}': {}", id, e));
            assert_eq!(t.id, id);
            assert!(!t.name.is_empty());
            assert!(!t.description.is_empty());
        }
    }

    #[test]
    fn category_counts() {
        let templates: Vec<IntegrationTemplate> = bundled_integrations()
            .iter()
            .map(|(_, c)| toml::from_str(c).unwrap())
            .collect();
        let devtools = templates
            .iter()
            .filter(|t| t.category == crate::IntegrationCategory::DevTools)
            .count();
        let productivity = templates
            .iter()
            .filter(|t| t.category == crate::IntegrationCategory::Productivity)
            .count();
        let communication = templates
            .iter()
            .filter(|t| t.category == crate::IntegrationCategory::Communication)
            .count();
        let data = templates
            .iter()
            .filter(|t| t.category == crate::IntegrationCategory::Data)
            .count();
        let cloud = templates
            .iter()
            .filter(|t| t.category == crate::IntegrationCategory::Cloud)
            .count();
        let ai = templates
            .iter()
            .filter(|t| t.category == crate::IntegrationCategory::AI)
            .count();
        assert_eq!(devtools, 6);
        assert_eq!(productivity, 6);
        assert_eq!(communication, 3);
        assert_eq!(data, 5);
        assert_eq!(cloud, 3);
        assert_eq!(ai, 2);
    }

    #[test]
    fn no_duplicate_ids() {
        let integrations = bundled_integrations();
        let mut seen = std::collections::HashSet::new();
        for (id, _) in &integrations {
            assert!(seen.insert(id), "Duplicate integration id: {}", id);
        }
    }

    #[test]
    fn all_have_transport() {
        for (id, content) in bundled_integrations() {
            let t: IntegrationTemplate = toml::from_str(content)
                .unwrap_or_else(|e| panic!("Failed to parse '{}': {}", id, e));
            // All bundled integrations use stdio transport via npx
            match &t.transport {
                crate::McpTransportTemplate::Stdio { command, args } => {
                    assert_eq!(command, "npx", "{} should use npx", id);
                    assert!(!args.is_empty(), "{} should have args", id);
                }
                crate::McpTransportTemplate::Sse { .. } => {
                    panic!("{} unexpectedly uses SSE transport", id);
                }
                crate::McpTransportTemplate::Http { .. } => {
                    panic!("{} unexpectedly uses HTTP transport", id);
                }
            }
        }
    }

    #[test]
    fn oauth_integrations() {
        let templates: Vec<(String, IntegrationTemplate)> = bundled_integrations()
            .iter()
            .map(|(id, c)| (id.to_string(), toml::from_str(c).unwrap()))
            .collect();
        let oauth_ids: Vec<&str> = templates
            .iter()
            .filter(|(_, t)| t.oauth.is_some())
            .map(|(id, _)| id.as_str())
            .collect();
        // Expected OAuth integrations: github, google-calendar, gmail, google-drive, slack, teams-mcp
        assert!(oauth_ids.contains(&"github"));
        assert!(oauth_ids.contains(&"google-calendar"));
        assert!(oauth_ids.contains(&"gmail"));
        assert!(oauth_ids.contains(&"google-drive"));
        assert!(oauth_ids.contains(&"slack"));
        assert!(oauth_ids.contains(&"teams-mcp"));
        assert_eq!(oauth_ids.len(), 6);
    }
}
