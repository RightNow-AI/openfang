<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-extensions/integrations

## Purpose

25 bundled MCP server integration templates in TOML format. Each template defines how to launch and configure an MCP server (transport, credentials, health checks), what tools it provides, and what credentials are required. Templates are embedded at compile-time and available immediately after installation.

## Integrations

| ID | Name | Category | Transport | Credentials |
|----|------|----------|-----------|-------------|
| `aws` | AWS | Cloud | stdio (npx) | AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY |
| `azure-mcp` | Microsoft Azure | Cloud | stdio (npx) | AZURE_SUBSCRIPTION_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET |
| `bitbucket` | Bitbucket | VCS | stdio | BITBUCKET_USERNAME, BITBUCKET_PASSWORD |
| `brave-search` | Brave Search | Search | stdio (npx) | BRAVE_SEARCH_API_KEY |
| `discord-mcp` | Discord | Communication | stdio | DISCORD_BOT_TOKEN |
| `dropbox` | Dropbox | Storage | stdio (npx) | DROPBOX_ACCESS_TOKEN |
| `elasticsearch` | Elasticsearch | Database | HTTP | ELASTICSEARCH_URL, ELASTICSEARCH_API_KEY |
| `exa-search` | Exa Search | Search | stdio (npx) | EXA_API_KEY |
| `gcp-mcp` | Google Cloud | Cloud | stdio (npx) | GCP_PROJECT_ID, GCP_SERVICE_ACCOUNT_KEY |
| `github` | GitHub | VCS | stdio | GITHUB_TOKEN |
| `gitlab` | GitLab | VCS | stdio | GITLAB_TOKEN, GITLAB_URL |
| `gmail` | Gmail | Email | stdio | GMAIL_ACCOUNT, GMAIL_OAUTH_TOKEN |
| `google-calendar` | Google Calendar | Productivity | stdio | GOOGLE_CALENDAR_API_KEY |
| `google-drive` | Google Drive | Storage | stdio | GOOGLE_DRIVE_API_KEY, GOOGLE_DRIVE_FOLDER_ID |
| `jira` | Jira | Project Management | stdio | JIRA_URL, JIRA_USERNAME, JIRA_API_TOKEN |
| `linear` | Linear | Project Management | stdio (npx) | LINEAR_API_KEY |
| `mongodb` | MongoDB | Database | HTTP | MONGODB_URI |
| `notion` | Notion | Knowledge Base | stdio | NOTION_API_KEY, NOTION_DATABASE_ID |
| `postgresql` | PostgreSQL | Database | stdio | DATABASE_URL |
| `redis` | Redis | Cache/Database | HTTP | REDIS_URL |
| `sentry` | Sentry | Monitoring | stdio | SENTRY_AUTH_TOKEN, SENTRY_ORG_SLUG |
| `slack` | Slack | Communication | stdio (npx) | SLACK_BOT_TOKEN, SLACK_SIGNING_SECRET |
| `sqlite-mcp` | SQLite | Database | stdio (npx) | SQLITE_DATABASE_PATH |
| `teams-mcp` | Microsoft Teams | Communication | stdio | TEAMS_BOT_ID, TEAMS_BOT_PASSWORD |
| `todoist` | Todoist | Productivity | stdio | TODOIST_API_TOKEN |

## TOML Template Format

```toml
id = "integration-id"
name = "Display Name"
description = "One-line purpose"
category = "category"
icon = "emoji"
tags = ["tag1", "tag2"]

[transport]
type = "stdio" | "sse" | "http"
command = "command-to-run"
args = ["arg1", "arg2"]

[[required_env]]
name = "ENV_VAR_NAME"
label = "Human-readable name"
help = "Instructions"
is_secret = true | false
get_url = "URL to get credential"

[health]
interval = 60
command = "health-check-command"
timeout = 10

[tools]
count = 15
# or list each tool...
```

## For AI Agents

### Working In This Directory

- Integration templates are loaded at compile-time by `bundled.rs` and embedded as const TOML strings.
- Each .toml file is an `IntegrationTemplate` that defines: transport config, required credentials, health check, tools provided.
- Templates are read-only; user installations store state in `~/.openfang/integrations.toml`.
- Registry merges bundled templates with installed state to provide a unified view.
- Installer uses template + user credentials to spawn the MCP server.

### Adding a New Integration

1. Create `integrations/new-service.toml` with template structure
2. Fill in: id, name, description, transport (how to launch), required_env (what credentials), health (health check), tools (what it provides)
3. Test: verify TOML parses, required credentials are clear, health check works
4. Regenerate `src/bundled.rs` (auto-detected by build system)

### Testing Requirements

- Verify TOML parses correctly (valid TOML, required fields present)
- Verify transport config is correct (command path exists, args are valid)
- Test health check: verify the configured health command detects server up/down
- Test credential setup: verify all required env vars have clear labels and help URLs
- Test MCP server launch: verify transport command actually spawns the server
- Test tools: verify the MCP server provides the expected tools and tool count matches

### Common Patterns

- `transport.type` is usually "stdio" for npm-based or local binary servers
- `transport.type` is "http" for servers that expose an HTTP endpoint
- `required_env` entries with `is_secret=true` are stored in the vault; `is_secret=false` are plain config
- Health checks use a simple command (often a curl request or server ping)
- Tool count is optional but helpful for UI display
- Tags help with filtering and discovery (e.g., "cloud", "database", "communication")

## Dependencies

### Internal
- `openfang-extensions/src` — integration registry and installer

### External
None — templates are pure TOML data files

<!-- MANUAL: -->
