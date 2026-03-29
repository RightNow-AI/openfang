<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-skills/bundled

## Purpose

60 bundled AI skill definitions for domain-specific expertise. Each skill is a SKILL.md file with a YAML frontmatter header (name, description) followed by structured guidance on principles, techniques, patterns, and pitfalls for a specialized domain.

## Structure

Each subdirectory contains one skill:
```
skill-name/
  SKILL.md    — Skill definition with frontmatter + system prompt
```

## Categories

| Category | Skills | Examples |
|----------|--------|----------|
| **Cloud & Infrastructure** | 3 | aws, azure, gcp |
| **DevOps & Containers** | 5 | docker, kubernetes, helm, terraform, ansible |
| **Databases** | 5 | mongodb, postgres-expert, redis-expert, sqlite-expert, vector-db |
| **Programming Languages** | 6 | python-expert, rust-expert, golang-expert, typescript-expert, react-expert, nextjs-expert |
| **Web & APIs** | 4 | graphql-expert, openapi-expert, web-search, nextjs-expert |
| **Data & Analytics** | 4 | data-analyst, data-pipeline, ml-engineer, sql-analyst |
| **Security & Compliance** | 3 | security-audit, oauth-expert, crypto-expert |
| **Operations & Monitoring** | 4 | prometheus, sentry, elasticsearch, nginx |
| **Communication & Content** | 3 | email-writer, technical-writer, writing-coach |
| **Specialized Domains** | 18 | code-reviewer, git-expert, linux-networking, shell-scripting, presentation, interview-prep, project-manager, regex-expert, jira, confluence, notion, slack-tools, ci-cd, compliance, api-tester, css-expert, figma-expert, wasm-expert, llm-finetuning, prompt-engineer |

## Skill Format

Each SKILL.md follows this pattern:

```yaml
---
name: skill-id
description: "One-line skill purpose"
---

# Skill Title

Introductory paragraph explaining the expertise domain.

## Key Principles

- Principle 1
- Principle 2

## Techniques

- Technique with explanation

## Common Patterns

- Pattern 1: Use case description
- Pattern 2: Use case description

## Pitfalls to Avoid

- Don't do X; it causes Y
- Don't do Z; use W instead
```

## For AI Agents

### Working In This Directory

- Skills are loaded at runtime by `openfang-skills` crate via `bundled.rs`.
- Each skill's SKILL.md is embedded as a const string and served to agents in system prompts.
- Skill ID is the directory name (e.g., `docker` → skill ID is "docker").
- Agents request a skill by ID; the runtime loads the corresponding SKILL.md content.

### Adding a New Skill

1. Create a new directory: `crates/openfang-skills/bundled/skill-name/`
2. Create `SKILL.md` with frontmatter and content
3. Regenerate `src/bundled.rs` (auto-detected by build system)
4. Add to `skills` array in registry if manual registration is required

### Testing Requirements

- Verify SKILL.md frontmatter parses correctly (valid YAML)
- Verify skill content is coherent and actionable (no contradictions)
- Test skill integration: agent receives skill prompt, follows principles correctly
- Validate that skill guidance matches actual language/tool behavior

### Common Patterns

- Skills use active voice: "Use X to do Y" rather than passive guidance
- Pitfalls are framed as "Do not X; reason" or "Do not X; use Y instead"
- Code examples are domain-specific and tested when possible
- Principles are ordered by importance (most fundamental first)

## Dependencies

### Internal
- `openfang-skills/src` — skill registry and loader

### External
None — skills are pure data (SKILL.md files)

<!-- MANUAL: -->
