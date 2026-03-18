//! Centralized system prompt builder.
//!
//! Assembles a structured, multi-section system prompt from agent context.
//! Replaces the scattered `push_str` prompt injection throughout the codebase
//! with a single, testable, ordered prompt builder.

use openfang_types::memory::{MemoryFragment, MemorySource};
use tracing;

// ---------------------------------------------------------------------------
// H6: Prompt injection detection
// ---------------------------------------------------------------------------

/// Check if `content` contains common prompt injection patterns.
///
/// This is an in-process defense-in-depth measure, not a complete solution.
/// It catches obvious injection attempts in user-sourced data (memories,
/// USER.md, canonical context) before they reach the assembled system prompt.
///
/// Patterns are intentionally kept simple to avoid false positives on
/// legitimate user content. Callers should log when this returns `true`.
fn contains_injection_pattern(content: &str) -> bool {
    let lower = content.to_lowercase();
    // Common prompt injection phrases. Add new ones conservatively \u2014
    // false positives corrupt legitimate context.
    const PATTERNS: &[&str] = &[
        "ignore previous instructions",
        "ignore all previous",
        "disregard previous",
        "disregard all previous",
        "forget your instructions",
        "new instructions:",
        "system prompt override",
        "ignore the above",
        "override system",
        "you are now",
        "act as if",
    ];
    PATTERNS.iter().any(|p| lower.contains(p))
}

/// All the context needed to build a system prompt for an agent.
#[derive(Debug, Clone, Default)]
pub struct PromptContext {
    /// Agent name (from manifest).
    pub agent_name: String,
    /// Agent description (from manifest).
    pub agent_description: String,
    /// Base system prompt authored in the agent manifest.
    pub base_system_prompt: String,
    /// Tool names this agent has access to.
    pub granted_tools: Vec<String>,
    /// Recalled memories.
    pub recalled_memories: Vec<MemoryFragment>,
    /// Skill summary text (from kernel.build_skill_summary()).
    pub skill_summary: String,
    /// Prompt context from prompt-only skills.
    pub skill_prompt_context: String,
    /// MCP server/tool summary text.
    pub mcp_summary: String,
    /// Agent workspace path.
    pub workspace_path: Option<String>,
    /// SOUL.md content (persona).
    pub soul_md: Option<String>,
    /// USER.md content.
    pub user_md: Option<String>,
    /// MEMORY.md content.
    pub memory_md: Option<String>,
    /// Cross-channel canonical context summary.
    pub canonical_context: Option<String>,
    /// Known user name (from shared memory).
    pub user_name: Option<String>,
    /// Channel type (telegram, discord, web, etc.).
    pub channel_type: Option<String>,
    /// Whether this agent was spawned as a subagent.
    pub is_subagent: bool,
    /// Whether this agent has autonomous config.
    pub is_autonomous: bool,
    /// AGENTS.md content (behavioral guidance).
    pub agents_md: Option<String>,
    /// BOOTSTRAP.md content (first-run ritual).
    pub bootstrap_md: Option<String>,
    /// Workspace context section (project type, context files).
    pub workspace_context: Option<String>,
    /// IDENTITY.md content (visual identity + personality frontmatter).
    pub identity_md: Option<String>,
    /// HEARTBEAT.md content (autonomous agent checklist).
    pub heartbeat_md: Option<String>,
    /// Peer agents visible to this agent: (name, state, model).
    pub peer_agents: Vec<(String, String, String)>,
    /// Current date/time string for temporal awareness.
    pub current_date: Option<String>,
    /// Sender identity (e.g. WhatsApp phone number, Telegram user ID).
    pub sender_id: Option<String>,
    /// Sender display name.
    pub sender_name: Option<String>,
}

/// Build the complete system prompt from a `PromptContext`.
///
/// Produces an ordered, multi-section prompt. Sections with no content are
/// omitted entirely (no empty headers). Subagent mode skips sections that
/// add unnecessary context overhead.
pub fn build_system_prompt(ctx: &PromptContext) -> String {
    collect_prompt_sections(ctx)
        .into_iter()
        .map(|section| section.content)
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn build_prompt_telemetry(ctx: &PromptContext) -> PromptTelemetry {
    let sections = collect_prompt_sections(ctx);
    let prompt = sections
        .iter()
        .map(|section| section.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let telemetry_sections = sections
        .into_iter()
        .map(|section| PromptSectionTelemetry {
            name: section.name,
            chars: section.content.chars().count(),
            estimated_tokens: estimate_token_count(&section.content),
        })
        .collect();

    PromptTelemetry {
        total_chars: prompt.chars().count(),
        estimated_tokens: estimate_token_count(&prompt),
        sections: telemetry_sections,
    }
}

pub fn estimate_token_count(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

fn collect_prompt_sections(ctx: &PromptContext) -> Vec<PromptSection> {
    let mut sections: Vec<PromptSection> = Vec::with_capacity(16);

    // Section 1 — Agent Identity (always present)
    push_section(&mut sections, "Agent Identity", build_identity_section(ctx));

    // Section 1.5 — Current Date/Time (always present when set)
    if let Some(ref date) = ctx.current_date {
        push_section(
            &mut sections,
            "Current Date",
            format!("## Current Date\nToday is {date}."),
        );
    }

    // Section 2 — Tool Call Behavior (skip for subagents)
    if !ctx.is_subagent {
        push_section(
            &mut sections,
            "Tool Call Behavior",
            TOOL_CALL_BEHAVIOR.to_string(),
        );
    }

    // Section 2.5 — Agent Behavioral Guidelines (skip for subagents)
    if !ctx.is_subagent {
        if let Some(ref agents) = ctx.agents_md {
            if !agents.trim().is_empty() {
                push_section(&mut sections, "Agent Behavioral Guidelines", cap_str(agents, 2000));
            }
        }
    }

    // Section 3 — Available Tools (always present if tools exist)
    let tools_section = build_tools_section(&ctx.granted_tools);
    push_section(&mut sections, "Available Tools", tools_section);

    // Section 4 — Memory Protocol (always present)
    let mem_section = build_memory_section(ctx);
    push_section(&mut sections, "Memory Protocol", mem_section);

    // Section 5 — Skills (only if skills available)
    if !ctx.skill_summary.is_empty() || !ctx.skill_prompt_context.is_empty() {
        push_section(
            &mut sections,
            "Skills",
            build_skills_section(&ctx.skill_summary, &ctx.skill_prompt_context),
        );
    }

    // Section 6 — MCP Servers (only if summary present)
    if !ctx.mcp_summary.is_empty() {
        push_section(&mut sections, "MCP Servers", build_mcp_section(&ctx.mcp_summary));
    }

    // Section 7 — Persona / Identity files (skip for subagents)
    if !ctx.is_subagent {
        let persona = build_persona_section(
            ctx.identity_md.as_deref(),
            ctx.soul_md.as_deref(),
            ctx.user_md.as_deref(),
            ctx.memory_md.as_deref(),
            ctx.workspace_path.as_deref(),
        );
        push_section(&mut sections, "Persona", persona);
    }

    // Section 7.5 — Heartbeat checklist (only for autonomous agents)
    if !ctx.is_subagent && ctx.is_autonomous {
        if let Some(ref heartbeat) = ctx.heartbeat_md {
            if !heartbeat.trim().is_empty() {
                push_section(
                    &mut sections,
                    "Heartbeat Checklist",
                    format!("## Heartbeat Checklist\n{}", cap_str(heartbeat, 1000)),
                );
            }
        }
    }

    // Section 8 — User Personalization (skip for subagents)
    if !ctx.is_subagent {
        push_section(
            &mut sections,
            "User Personalization",
            build_user_section(ctx.user_name.as_deref()),
        );
    }

    // Section 9 — Channel Awareness (skip for subagents)
    if !ctx.is_subagent {
        if let Some(ref channel) = ctx.channel_type {
            push_section(&mut sections, "Channel Awareness", build_channel_section(channel));
        }
    }

    // Section 9.1 — Sender Identity (skip for subagents)
    if !ctx.is_subagent {
        if let Some(sender_line) =
            build_sender_section(ctx.sender_name.as_deref(), ctx.sender_id.as_deref())
        {
            sections.push(sender_line);
        }
    }

    // Section 9.5 — Peer Agent Awareness (skip for subagents)
    if !ctx.is_subagent && !ctx.peer_agents.is_empty() {
        push_section(
            &mut sections,
            "Peer Agent Awareness",
            build_peer_agents_section(
                &ctx.agent_name,
                &ctx.peer_agents,
                ctx.effective_peer_list_limit(),
            ),
        );
    }

    // Section 10 — Safety & Oversight (skip for subagents)
    if !ctx.is_subagent {
        push_section(&mut sections, "Safety & Oversight", SAFETY_SECTION.to_string());
    }

    // Section 11 — Operational Guidelines (always present)
    push_section(
        &mut sections,
        "Operational Guidelines",
        OPERATIONAL_GUIDELINES.to_string(),
    );

    // Section 11.5 — Production hardening and security enforcement
    if ctx.include_hardening_section() {
        push_section(
            &mut sections,
            "Production Hardening",
            build_hardening_section(ctx),
        );
    }

    // Section 12 — Canonical Context moved to build_canonical_context_message()
    // to keep the system prompt stable across turns for provider prompt caching.

    // Section 13 — Bootstrap Protocol (only on first-run, skip for subagents)
    if !ctx.is_subagent {
        if let Some(ref bootstrap) = ctx.bootstrap_md {
            if !bootstrap.trim().is_empty() {
                // Only inject if no user_name memory exists (first-run heuristic)
                let has_user_name = ctx.recalled_memories.iter().any(|m| m.content.contains("user_name:") || m.scope == "user_name");
                if !has_user_name && ctx.user_name.is_none() {
                    push_section(
                        &mut sections,
                        "Bootstrap Protocol",
                        format!("## First-Run Protocol\n{}", cap_str(bootstrap, 1500)),
                    );
                }
            }
        }
    }

    // Section 14 — Workspace Context (skip for subagents)
    if !ctx.is_subagent {
        if let Some(ref ws_ctx) = ctx.workspace_context {
            if !ws_ctx.trim().is_empty() {
                push_section(&mut sections, "Workspace Context", cap_str(ws_ctx, 1000));
            }
        }
    }

    sections
}

fn push_section(sections: &mut Vec<PromptSection>, name: &'static str, content: String) {
    if !content.trim().is_empty() {
        sections.push(PromptSection { name, content });
    }
}

// ---------------------------------------------------------------------------
// Section builders
// ---------------------------------------------------------------------------

fn build_identity_section(ctx: &PromptContext) -> String {
    if ctx.base_system_prompt.is_empty() {
        format!(
            "You are {}, an AI agent running inside the OpenFang Agent OS.\n{}",
            ctx.agent_name, ctx.agent_description
        )
    } else {
        ctx.base_system_prompt.clone()
    }
}

/// Static tool-call behavior directives.
const TOOL_CALL_BEHAVIOR: &str = "\
## Tool Call Behavior
- When you need to use a tool, call it immediately. Do not narrate or explain routine tool calls.
- Only explain tool calls when the action is destructive, unusual, or the user explicitly asked for an explanation.
- Prefer action over narration.
- Use tools when they improve accuracy, freshness, or task completion.
- When executing multiple sequential tool calls, batch them — don't output reasoning between each call.
- If a tool returns useful results, present the KEY information, not the raw output.
- When web_fetch or web_search returns content, you MUST include the relevant data in your response. \
Quote specific facts, numbers, or passages from the fetched content. Never say you fetched something \
without sharing what you found.
- Start with the answer, not meta-commentary about how you'll help.
- IMPORTANT: If your instructions or persona mention a shell command, script path, or code snippet, \
execute it via the appropriate tool call (shell_exec, file_write, etc.). Never output commands as \
code blocks — always call the tool instead.";

/// Build the grouped tools section (Section 3).
pub fn build_tools_section(granted_tools: &[String]) -> String {
    if granted_tools.is_empty() {
        return String::new();
    }

    // Group tools by category
    let mut groups: std::collections::BTreeMap<&str, Vec<(&str, &str)>> =
        std::collections::BTreeMap::new();
    for name in granted_tools {
        let cat = tool_category(name);
        let hint = tool_hint(name);
        groups.entry(cat).or_default().push((name.as_str(), hint));
    }

    let mut out = String::from("## Your Tools\nYou have access to these capabilities:\n");
    for (category, tools) in &groups {
        out.push_str(&format!("\n**{}**: ", capitalize(category)));
        let descs: Vec<String> = tools
            .iter()
            .map(|(name, hint)| {
                if hint.is_empty() {
                    (*name).to_string()
                } else {
                    format!("{name} ({hint})")
                }
            })
            .collect();
        out.push_str(&descs.join(", "));
    }
    out
}

/// Build canonical context as a standalone user message (instead of system prompt).
///
/// This keeps the system prompt stable across turns, enabling provider prompt caching
/// (Anthropic cache_control, etc.). The canonical context changes every turn, so
/// injecting it in the system prompt caused 82%+ cache misses.
pub fn build_canonical_context_message(ctx: &PromptContext) -> Option<String> {
    if ctx.is_subagent {
        return None;
    }
    ctx.canonical_context
        .as_ref()
        .filter(|c| !c.is_empty())
        .map(|c| {
            let capped = cap_str(c, ctx.effective_canonical_context_limit());
            // H6: Scan canonical context for injection patterns.
            if contains_injection_pattern(&capped) {
                tracing::warn!(
                    "Possible prompt injection detected in canonical context — content redacted"
                );
                return "[Previous conversation context]\n[Context redacted by safety filter]"
                    .to_string();
            }
            format!("[Previous conversation context]\n{}", capped)
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryRank {
    pinned_rank: u8,
    kind_rank: u8,
    relevance_rank: i32,
    updated_at_rank: i64,
    stable_id: openfang_types::memory::MemoryId,
}

impl MemoryRank {
    fn from_memory(memory: &MemoryFragment) -> Self {
        let is_pinned = memory.metadata.get("is_pinned")
            .and_then(|v| v.as_bool()).unwrap_or(false);

        Self {
            pinned_rank: u8::from(is_pinned),
            kind_rank: memory_kind_rank(memory),
            relevance_rank: relevance_rank(memory.confidence),
            updated_at_rank: memory.accessed_at.timestamp().max(memory.created_at.timestamp()),
            stable_id: memory.id,
        }
    }
}

fn memory_source_rank(source: &MemorySource) -> u8 {
    match source {
        MemorySource::UserProvided => 50,
        MemorySource::Observation => 30,
        MemorySource::Document => 20,
        MemorySource::System => 10,
        MemorySource::Conversation => 0,
        MemorySource::Inference => 5,
    }
}

fn memory_kind_rank(memory: &MemoryFragment) -> u8 {
    let normalized_kind = memory
        .metadata
        .get("kind")
        .or_else(|| memory.metadata.get("memory_kind"))
        .or_else(|| memory.metadata.get("memory_type"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_ascii_lowercase());

    match normalized_kind.as_deref() {
        Some("user_identity") | Some("useridentity") => 50,
        Some("user_preference") | Some("userpreference") => 40,
        Some("long_term_constraint") | Some("longtermconstraint") => 35,
        Some("canonical_context") | Some("canonicalcontext") => 30,
        Some("task_context") | Some("taskcontext") => 20,
        Some("note") => 0,
        Some(_) => 10,
        None => infer_memory_kind_rank(memory),
    }
}

fn infer_memory_kind_rank(memory: &MemoryFragment) -> u8 {
    let scope = memory.scope.to_ascii_lowercase();
    let content = memory.content.to_ascii_lowercase();

    if scope == "user_name"
        || scope.contains("identity")
        || content.starts_with("user_name:")
        || content.starts_with("name:")
    {
        50
    } else if scope.contains("preference") || content.contains("prefers") {
        40
    } else if scope.contains("constraint")
        || content.contains("must always")
        || content.contains("never ")
    {
        35
    } else if scope.contains("canonical") {
        30
    } else if scope.contains("task") {
        20
    } else {
        memory_source_rank(&memory.source)
    }
}

fn relevance_rank(score: f32) -> i32 {
    let clamped = score.clamp(0.0, 1.0);
    (clamped * 1_000.0).round() as i32
}

fn compare_recalled_memories(a: &MemoryFragment, b: &MemoryFragment) -> std::cmp::Ordering {
    let a_rank = MemoryRank::from_memory(a);
    let b_rank = MemoryRank::from_memory(b);

    b_rank
        .pinned_rank
        .cmp(&a_rank.pinned_rank)
        .then_with(|| b_rank.kind_rank.cmp(&a_rank.kind_rank))
        .then_with(|| b_rank.relevance_rank.cmp(&a_rank.relevance_rank))
        .then_with(|| b_rank.updated_at_rank.cmp(&a_rank.updated_at_rank))
        .then_with(|| {
            let a_str = a_rank.stable_id.0.to_string();
            let b_str = b_rank.stable_id.0.to_string();
            a_str.cmp(&b_str)
        })
}

fn rank_recalled_memories(memories: &mut [MemoryFragment]) {
    memories.sort_by(compare_recalled_memories);
}

fn omitted_memories_text(count: usize) -> String {
    match count {
        1 => "_And 1 more memory omitted._".to_string(),
        n => format!("_And {} more memories omitted._", n),
    }
}

/// Build the memory section (Section 4).
///
/// Also used by `agent_loop.rs` to append recalled memories after DB lookup.
pub fn build_memory_section(memories: &[(String, String)]) -> String {
    let mut out = String::from("## Memory\n");
    if memories.is_empty() {
        out.push_str(
            "- When the user asks about something from a previous conversation, use memory_recall first.\n\
             - Store important preferences, decisions, and context with memory_store for future use.",
        );
    } else {
        out.push_str(
            "- Use the recalled memories below to inform your responses.\n\
             - Only call memory_recall if you need information not already shown here.\n\
             - Store important preferences, decisions, and context with memory_store for future use.",
        );
        out.push_str("\n\nRecalled memories:\n");
        for (key, content) in memories.iter().take(5) {
            let capped = cap_str(content, 500);
            if key.is_empty() {
                out.push_str(&format!("- {capped}\n"));
            } else {
                lines.push(format!("- {}", content));
            }
        }
    }

    if lines.is_empty() {
        return section;
    }

    section.push_str("\n\nRecalled memories:\n");
    section.push_str(&lines.join("\n"));

    let omitted_count = total_count.saturating_sub(lines.len());
    if omitted_count > 0 {
        section.push_str(&format!("\n\n{}", omitted_memories_text(omitted_count)));
    }

    section
}

fn build_skills_section(skill_summary: &str, prompt_context: &str) -> String {
    let mut out = String::from("## Skills\n");
    if !skill_summary.is_empty() {
        out.push_str(
            "You have installed skills. If a request matches a skill, use its tools directly.\n",
        );
        out.push_str(skill_summary.trim());
    }
    if !prompt_context.is_empty() {
        out.push('\n');
        out.push_str(&cap_str(prompt_context, 2000));
    }
    out
}

fn build_mcp_section(mcp_summary: &str) -> String {
    format!("## Connected Tool Servers (MCP)\n{}", mcp_summary.trim())
}

fn build_persona_section(
    identity_md: Option<&str>,
    soul_md: Option<&str>,
    user_md: Option<&str>,
    memory_md: Option<&str>,
    workspace_path: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(ws) = workspace_path {
        parts.push(format!("## Workspace\nWorkspace: {ws}"));
    }

    // Identity file (IDENTITY.md) — personality at a glance, before SOUL.md
    if let Some(identity) = identity_md {
        if !identity.trim().is_empty() {
            parts.push(format!("## Identity\n{}", cap_str(identity, 500)));
        }
    }

    if let Some(soul) = soul_md {
        if !soul.trim().is_empty() {
            let sanitized = strip_code_blocks(soul);
            parts.push(format!(
                "## Persona\nEmbody this identity in your tone and communication style. Be natural, not stiff or generic.\n{}",
                cap_str(&sanitized, 1000)
            ));
        }
    }

    if let Some(user) = user_md {
        if !user.trim().is_empty() {
            // H6: Scan USER.md content for injection patterns.
            let capped = cap_str(user, 500);
            if contains_injection_pattern(&capped) {
                tracing::warn!("Possible prompt injection detected in USER.md content — using placeholder");
                parts.push("## User Context\n[User context flagged by safety filter]".to_string());
            } else {
                parts.push(format!("## User Context\n{}", capped));
            }
        }
    }

    if let Some(memory) = memory_md {
        if !memory.trim().is_empty() {
            parts.push(format!("## Long-Term Memory\n{}", cap_str(memory, 500)));
        }
    }

    parts.join("\n\n")
}

fn build_user_section(user_name: Option<&str>) -> String {
    match user_name {
        Some(name) => {
            format!(
                "## User Profile\n\
                 The user's name is \"{name}\". Address them by name naturally \
                 when appropriate (greetings, farewells, etc.), but don't overuse it."
            )
        }
        None => "## User Profile\n\
             If you don't know the user's name, ask for it in your first reply and store it immediately with `memory_store` using key \"user_name\"."
            .to_string(),
    }
}

fn build_channel_section(channel: &str) -> String {
    let (limit, hints) = match channel {
        "telegram" => (
            "4096",
            "Use Telegram-compatible formatting (bold with *, code with `backticks`).",
        ),
        "discord" => (
            "2000",
            "Use Discord markdown. Split long responses across multiple messages if needed.",
        ),
        "slack" => (
            "4000",
            "Use Slack mrkdwn formatting (*bold*, _italic_, `code`).",
        ),
        "whatsapp" => (
            "4096",
            "Keep messages concise. WhatsApp has limited formatting.",
        ),
        "irc" => (
            "512",
            "Keep messages very short. No markdown — plain text only.",
        ),
        "matrix" => (
            "65535",
            "Matrix supports rich formatting. Use markdown freely.",
        ),
        "teams" => ("28000", "Use Teams-compatible markdown."),
        _ => ("4096", "Use markdown formatting where supported."),
    };
    format!(
        "## Channel\n\
         You are responding via {channel}. Keep messages under {limit} chars.\n\
         {hints}"
    )
}

fn build_sender_section(sender_name: Option<&str>, sender_id: Option<&str>) -> Option<String> {
    match (sender_name, sender_id) {
        (Some(name), Some(id)) => Some(format!("## Sender\nMessage from: {name} ({id})")),
        (Some(name), None) => Some(format!("## Sender\nMessage from: {name}")),
        (None, Some(id)) => Some(format!("## Sender\nMessage from: {id}")),
        (None, None) => None,
    }
}

fn build_peer_agents_section(self_name: &str, peers: &[(String, String, String)]) -> String {
    let mut out = String::from(
        "## Peer Agents\n\
         You are part of a multi-agent system. These active agents are running alongside you:\n",
    );
    for (name, _state, model) in active_peers.iter().take(visible_limit) {
        out.push_str(&format!("- **{}** — model: {}\n", name, model));
    }
    let overflow = active_peers.len().saturating_sub(visible_limit);
    if overflow > 0 {
        out.push_str(&format!("- …and {overflow} more active peers\n"));
    }
    out.push_str(
        "\nYou can communicate with them using `agent_send` (by name) and see all agents with `agent_list`. \
         Delegate tasks to specialized agents when appropriate.",
    );
    out
}

/// Static safety section.
const SAFETY_SECTION: &str = "\
## Safety
- Prioritize safety and human oversight over task completion.
- NEVER auto-execute purchases, payments, account deletions, or irreversible actions without explicit user confirmation.
- If a tool could cause data loss, explain what it will do and confirm first.
- If you cannot accomplish a task safely, explain the limitation.
- When in doubt, ask the user.";

/// Static operational guidelines (replaces STABILITY_GUIDELINES).
const OPERATIONAL_GUIDELINES: &str = "\
## Operational Guidelines
- Do NOT retry a tool call with identical parameters if it failed. Try a different approach.
- If a tool returns an error, analyze the error before calling it again.
- Prefer targeted, specific tool calls over broad ones.
- Plan your approach before executing multiple tool calls.
- If you cannot accomplish a task after a few attempts, explain what went wrong instead of looping.
- Never call the same tool more than 3 times with the same parameters.
- If a message requires no response (simple acknowledgments, reactions, messages not directed at you), respond with exactly NO_REPLY.";

const HARDENING_INTRO: &str = "\
## AI Production Hardening and Security Enforcement
Apply this section only for developer tasks, explicit hardening passes, or security review flows.

You are acting as a senior software architect and security engineer. Your job is to eliminate technical debt, remove dead code, enforce strict security, improve architecture, and validate stability before deployment.";

const DEVELOPER_PROMPT_ADDON: &str = "\
### Non-Negotiable Rules — Code Hygiene
- Tree shake unused imports and exports.
- Detect and remove orphaned files, components, utilities, and endpoints.
- Eliminate duplicate logic and commented-out code.
- Avoid circular dependencies.
- Enforce strict typing and eliminate unchecked type gaps.
- Run lint checks and fix violations.
- Validate API contracts against schemas or typed interfaces.
- Run spell check on identifiers, comments, README content, and UI strings.
- Ensure naming conventions remain consistent.

### Error Handling
- Replace weak try/catch patterns with structured error handling.
- Refactor fragile if/else chains into guard clauses, early returns, and explicit error types.
- Do not allow silent failures.
- Every error should use the project's standard structured format where supported, including a stable error code, a human-readable message, optional details, and server-side logging.

### Dependencies
- Remove unused dependencies.
- Prefer actively maintained packages.
- Avoid abandoned libraries.
- Run vulnerability audits after dependency changes.
- Lock versions explicitly according to project policy.

### API Smoke Testing
- Generate or maintain smoke tests for every affected endpoint.
- Cover health checks, auth validation, invalid payloads, unauthorized access, and response-shape validation.
- Ensure CI fails if an endpoint contract breaks.

### Documentation
- Regenerate or rewrite README and setup documentation to reflect the real architecture.
- Document project overview, architecture summary, local development, security practices, CI/CD, deployment, known limitations, model loading, and smoke testing.";

const SECURITY_PROMPT_ADDON: &str = "\
### Security
#### Authentication
- Never invent custom authentication when a vetted provider is required.
- Prefer established auth systems and conservative session handling.
- Require secure token lifecycle management when refresh tokens exist.

#### API Protection
- Protect every endpoint with the appropriate authentication and authorization middleware.
- Add rate limiting where exposure warrants it.
- Validate all inputs with strict schemas.
- Use parameterized queries only.
- Block mass assignment.
- Validate redirect URLs with an allow-list.
- Restrict CORS to approved production origins.

#### Database
- Enforce row-level or ownership checks server-side where applicable.
- Never trust client-supplied user identifiers.

#### File Handling
- Validate MIME type and file signature.
- Enforce file size limits.
- Store uploads outside the public root unless intentionally public.
- Use signed URLs or equivalent controlled access when needed.

#### Webhooks and Payments
- Verify webhook signatures.
- Log all financial actions.
- Reject unsigned or replayed requests.

#### Infrastructure
- Separate test and production environments.
- Ensure test integrations never hit live systems.
- Remove console-log style debugging from production paths.
- Add audit logs for deletions, role changes, payments, and data exports.
- Prefer edge protections such as DDoS mitigation where relevant.";

const VALIDATION_PROMPT_ADDON: &str = "\
### Validation Pipeline
Before approval, require clean lint, type, schema, and spell-check results; no unused files or imports; protected endpoints; standardized errors; and passing smoke tests.

### Final Validation Hard Stop
- Do not report success if only module-level tests pass.
- Do not report success if workspace-wide build, test, or lint fails.
- Treat pre-existing errors as blocking production readiness.
- After every code change, run workspace build, workspace tests, and workspace clippy with warnings denied.
- If any command fails, identify whether the failure is new or pre-existing, show the exact file and symbol causing it, and do not mark the task complete.
- Mark remediation complete only when workspace build passes, workspace tests pass, workspace clippy passes, prompt-builder tests pass, and no critical security or schema issues remain.

### Problem Backlog Enforcement
- Do not ignore large issue counts.
- Summarize current problems by category and count.
- Separate pre-existing issues, issues introduced by the current change, auto-fixable issues, and manual-review items.
- Resolve issues in batches with validation after each batch.
- Do not mix security fixes with broad refactors in one commit unless required.

### Rust Module and Visibility Enforcement
- Do not publicly re-export crate-private symbols.
- Match re-export visibility to source item visibility.
- Prefer narrower visibility by default.
- Use `pub(crate)` for internal helpers and `pub` only for deliberate external APIs.
- When fixing visibility errors, decide whether the symbol is an internal helper or a true public API before changing visibility.
- Do not widen visibility unless cross-crate use requires it.
- After visibility changes, rerun workspace build, workspace tests, and workspace clippy with warnings denied.";

const MODEL_LOADING_PROMPT_ADDON: &str = "\
### Architecture Refactor
- Rebuild onboarding and other complex flows as modular steps with isolated state and explicit validation.
- Break monolithic upload or ingestion paths into maintainable units.
- Do not bundle heavyweight model artifacts when local loading is the better architecture.
- Support local Ollama detection, model selection, model presence validation, and graceful fallback messaging.

### Onboarding and Model Loading Enforcement
- Do not ship large bundled local model assets inside the main app by default.
- Move local model setup into onboarding.
- Detect Ollama availability during onboarding.
- Let the client choose the model provider and model name.
- Validate local model presence before first use.
- Show actionable setup guidance when a model is missing.
- Keep fallback behavior explicit and logged.
- Document all model setup steps in README.";

const HARDENING_GENERAL_PRINCIPLE: &str = "\
### General Principle
Assume malicious input.
Assume future scale.
Assume another developer will maintain this.
Optimize for long-term stability over short-term speed.
If uncertain, choose the more restrictive option.

### Execution Plan
- Run dependency analysis and remove dead code.
- Standardize error formats across the backend.
- Enforce auth and rate limiting globally where applicable.
- Build smoke tests before adding features.
- Rebuild onboarding modularly.
- Replace bundled model strategies with client-side local model loading when appropriate.
- Regenerate README from the actual architecture.
- Clean first, secure second, scale third.";

fn build_hardening_section(ctx: &PromptContext) -> String {
    let mut modules = vec![HARDENING_INTRO.to_string()];

    if ctx.is_developer_task || ctx.requires_hardening {
        modules.push(DEVELOPER_PROMPT_ADDON.to_string());
    }

    if ctx.requires_security_review || ctx.requires_hardening {
        modules.push(SECURITY_PROMPT_ADDON.to_string());
    }

    modules.push(VALIDATION_PROMPT_ADDON.to_string());
    modules.push(MODEL_LOADING_PROMPT_ADDON.to_string());
    modules.push(HARDENING_GENERAL_PRINCIPLE.to_string());

    modules.join("\n\n")
}

// ---------------------------------------------------------------------------
// Tool metadata helpers
// ---------------------------------------------------------------------------

/// Map a tool name to its category for grouping.
pub fn tool_category(name: &str) -> &'static str {
    match name {
        "file_read" | "file_write" | "file_list" | "file_delete" | "file_move" | "file_copy"
        | "file_search" => "Files",

        "web_search" | "web_fetch" => "Web",

        "browser_navigate" | "browser_click" | "browser_type" | "browser_screenshot"
        | "browser_read_page" | "browser_close" | "browser_scroll" | "browser_wait"
        | "browser_evaluate" | "browser_select" | "browser_back" => "Browser",

        "shell_exec" | "shell_background" => "Shell",

        "memory_store" | "memory_recall" | "memory_delete" | "memory_list" => "Memory",

        "agent_send" | "agent_spawn" | "agent_list" | "agent_kill" => "Agents",

        "image_describe" | "image_generate" | "audio_transcribe" | "tts_speak" => "Media",

        "docker_exec" | "docker_build" | "docker_run" => "Docker",

        "cron_create" | "cron_list" | "cron_delete" => "Scheduling",

        "process_start" | "process_poll" | "process_write" | "process_kill" | "process_list" => {
            "Processes"
        }

        _ if name.starts_with("mcp_") => "MCP",
        _ if name.starts_with("skill_") => "Skills",
        _ => "Other",
    }
}

/// Map a tool name to a one-line description hint.
pub fn tool_hint(name: &str) -> &'static str {
    match name {
        // Files
        "file_read" => "read file contents",
        "file_write" => "create or overwrite a file",
        "file_list" => "list directory contents",
        "file_delete" => "delete a file",
        "file_move" => "move or rename a file",
        "file_copy" => "copy a file",
        "file_search" => "search files by name pattern",

        // Web
        "web_search" => "search the web for information",
        "web_fetch" => "fetch a URL and get its content as markdown",

        // Browser
        "browser_navigate" => "open a URL in the browser",
        "browser_click" => "click an element on the page",
        "browser_type" => "type text into an input field",
        "browser_screenshot" => "capture a screenshot",
        "browser_read_page" => "extract page content as text",
        "browser_close" => "close the browser session",
        "browser_scroll" => "scroll the page",
        "browser_wait" => "wait for an element or condition",
        "browser_evaluate" => "run JavaScript on the page",
        "browser_select" => "select a dropdown option",
        "browser_back" => "go back to the previous page",

        // Shell
        "shell_exec" => "execute a shell command",
        "shell_background" => "run a command in the background",

        // Memory
        "memory_store" => "save a key-value pair to memory",
        "memory_recall" => "search memory for relevant context",
        "memory_delete" => "delete a memory entry",
        "memory_list" => "list stored memory keys",

        // Agents
        "agent_send" => "send a message to another agent",
        "agent_spawn" => "create a new agent",
        "agent_list" => "list running agents",
        "agent_kill" => "terminate an agent",

        // Media
        "image_describe" => "describe an image",
        "image_generate" => "generate an image from a prompt",
        "audio_transcribe" => "transcribe audio to text",
        "tts_speak" => "convert text to speech",

        // Docker
        "docker_exec" => "run a command in a container",
        "docker_build" => "build a Docker image",
        "docker_run" => "start a Docker container",

        // Scheduling
        "cron_create" => "schedule a recurring task",
        "cron_list" => "list scheduled tasks",
        "cron_delete" => "remove a scheduled task",

        // Processes
        "process_start" => "start a long-running process (REPL, server)",
        "process_poll" => "read stdout/stderr from a running process",
        "process_write" => "write to a process's stdin",
        "process_kill" => "terminate a running process",
        "process_list" => "list active processes",

        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Cap a string to `max_chars`, appending "..." if truncated.
/// Strip markdown triple-backtick code blocks from content.
///
/// Prevents LLMs from copying code blocks as text output instead of making
/// tool calls when SOUL.md contains command examples.
fn strip_code_blocks(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut in_block = false;
    for line in content.lines() {
        if line.trim_start().starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if !in_block {
            result.push_str(line);
            result.push('\n');
        }
    }
    // Collapse multiple blank lines left by stripped blocks
    while result.contains("\n\n\n") {
        result = result.replace("\n\n\n", "\n\n");
    }
    result.trim().to_string()
}

fn cap_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max_chars)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

/// Capitalize the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use openfang_types::agent::AgentId;
    use openfang_types::memory::{MemoryId, MemorySource};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn basic_ctx() -> PromptContext {
        PromptContext {
            agent_name: "researcher".to_string(),
            agent_description: "Research agent".to_string(),
            base_system_prompt: "You are Researcher, a research agent.".to_string(),
            granted_tools: vec![
                "web_search".to_string(),
                "web_fetch".to_string(),
                "file_read".to_string(),
                "file_write".to_string(),
                "memory_store".to_string(),
                "memory_recall".to_string(),
            ],
            ..Default::default()
        }
    }

    fn mk_test_memory(
        id_seed: u128,
        content: &str,
        scope: &str,
        source: MemorySource,
        confidence: f32,
        accessed_at_days_ago: i64,
        metadata: &[(&str, serde_json::Value)],
    ) -> MemoryFragment {
        let created_at = Utc::now() - Duration::days(accessed_at_days_ago + 1);
        let accessed_at = Utc::now() - Duration::days(accessed_at_days_ago);
        let mut memory_metadata = HashMap::new();
        for (key, value) in metadata {
            memory_metadata.insert((*key).to_string(), value.clone());
        }

        MemoryFragment {
            id: MemoryId(Uuid::from_u128(id_seed)),
            agent_id: AgentId(Uuid::from_u128(9_000 + id_seed)),
            content: content.to_string(),
            embedding: None,
            metadata: memory_metadata,
            source,
            confidence,
            created_at,
            accessed_at,
            access_count: 0,
            scope: scope.to_string(),
        }
    }

    fn pos(haystack: &str, needle: &str) -> usize {
        haystack
            .find(needle)
            .unwrap_or_else(|| panic!("missing expected substring: {needle}"))
    }

    #[test]
    fn test_hardening_omitted_by_default() {
        let prompt = build_system_prompt(&basic_ctx());
        assert!(!prompt.contains("## AI Production Hardening and Security Enforcement"));
    }

    #[test]
    fn test_full_prompt_has_all_sections() {
        let mut ctx = basic_ctx();
        ctx.is_developer_task = true;
        ctx.requires_security_review = true;
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.contains("You are Researcher"));
        assert!(prompt.contains("## Tool Call Behavior"));
        assert!(prompt.contains("## Your Tools"));
        assert!(prompt.contains("## Memory"));
        assert!(prompt.contains("## User Profile"));
        assert!(prompt.contains("## Safety"));
        assert!(prompt.contains("## Operational Guidelines"));
        assert!(prompt.contains("## AI Production Hardening and Security Enforcement"));
        assert!(prompt.contains("### Final Validation Hard Stop"));
        assert!(prompt.contains("### Onboarding and Model Loading Enforcement"));
        assert!(prompt.contains("### Problem Backlog Enforcement"));
        assert!(prompt.contains("### Rust Module and Visibility Enforcement"));
    }

    #[test]
    fn test_section_ordering() {
        let mut ctx = basic_ctx();
        ctx.requires_hardening = true;
        let prompt = build_system_prompt(&ctx);
        let tool_behavior_pos = prompt.find("## Tool Call Behavior").unwrap();
        let tools_pos = prompt.find("## Your Tools").unwrap();
        let memory_pos = prompt.find("## Memory").unwrap();
        let safety_pos = prompt.find("## Safety").unwrap();
        let guidelines_pos = prompt.find("## Operational Guidelines").unwrap();
        let remediation_pos = prompt
            .find("## AI Production Hardening and Security Enforcement")
            .unwrap();

        assert!(tool_behavior_pos < tools_pos);
        assert!(tools_pos < memory_pos);
        assert!(memory_pos < safety_pos);
        assert!(safety_pos < guidelines_pos);
        assert!(guidelines_pos < remediation_pos);
    }

    #[test]
    fn test_subagent_omits_sections() {
        let mut ctx = basic_ctx();
        ctx.is_subagent = true;
        let prompt = build_system_prompt(&ctx);

        assert!(!prompt.contains("## Tool Call Behavior"));
        assert!(!prompt.contains("## User Profile"));
        assert!(!prompt.contains("## Channel"));
        assert!(!prompt.contains("## Safety"));
        assert!(!prompt.contains("## AI Production Hardening and Security Enforcement"));
        // Subagents still get tools and guidelines
        assert!(prompt.contains("## Your Tools"));
        assert!(prompt.contains("## Operational Guidelines"));
        assert!(prompt.contains("## Memory"));
    }

    #[test]
    fn test_subagent_hardening_opt_in() {
        let mut ctx = basic_ctx();
        ctx.is_subagent = true;
        ctx.requires_security_review = true;
        let prompt = build_system_prompt(&ctx);

        assert!(prompt.contains("## AI Production Hardening and Security Enforcement"));
        assert!(prompt.contains("### Security"));
    }

    #[test]
    fn test_empty_tools_no_section() {
        let ctx = PromptContext {
            agent_name: "test".to_string(),
            ..Default::default()
        };
        let prompt = build_system_prompt(&ctx);
        assert!(!prompt.contains("## Your Tools"));
    }

    #[test]
    fn test_tool_grouping() {
        let tools = vec![
            "web_search".to_string(),
            "web_fetch".to_string(),
            "file_read".to_string(),
            "browser_navigate".to_string(),
        ];
        let section = build_tools_section(&tools);
        assert!(section.contains("**Browser**"));
        assert!(section.contains("**Files**"));
        assert!(section.contains("**Web**"));
    }

    #[test]
    fn test_tool_categories() {
        assert_eq!(tool_category("file_read"), "Files");
        assert_eq!(tool_category("web_search"), "Web");
        assert_eq!(tool_category("browser_navigate"), "Browser");
        assert_eq!(tool_category("shell_exec"), "Shell");
        assert_eq!(tool_category("memory_store"), "Memory");
        assert_eq!(tool_category("agent_send"), "Agents");
        assert_eq!(tool_category("mcp_github_search"), "MCP");
        assert_eq!(tool_category("unknown_tool"), "Other");
    }

    #[test]
    fn test_tool_hints() {
        assert!(!tool_hint("web_search").is_empty());
        assert!(!tool_hint("file_read").is_empty());
        assert!(!tool_hint("browser_navigate").is_empty());
        assert!(tool_hint("some_unknown_tool").is_empty());
    }

    #[test]
    fn test_tool_behavior_uses_softened_wording() {
        assert!(TOOL_CALL_BEHAVIOR.contains(
            "Use tools when they improve accuracy, freshness, or task completion."
        ));
        assert!(!TOOL_CALL_BEHAVIOR.contains(
            "If you can answer by using a tool, do it."
        ));
    }

    #[test]
    fn no_reply_instruction_uses_exact_token() {
        let prompt = build_system_prompt(&basic_ctx());
        assert!(prompt.contains("NO_REPLY"));
        assert!(
            prompt.contains("output only the token") || prompt.contains("exactly NO_REPLY")
        );
    }

    #[test]
    fn test_memory_section_empty() {
        let section = build_memory_section(&basic_ctx());
        assert!(section.contains("## Memory"));
        assert!(section.contains("use memory_recall first"));
        assert!(!section.contains("Recalled memories"));
    }

    #[test]
    fn test_memory_section_with_items() {
        let mut ctx = basic_ctx();
        ctx.recalled_memories = vec![
            mk_test_memory(
                1,
                "User likes dark mode",
                "preference",
                MemorySource::UserProvided,
                0.8,
                2,
                &[("kind", serde_json::json!("user_preference"))],
            ),
            mk_test_memory(
                2,
                "Working on Rust project",
                "task_context",
                MemorySource::Conversation,
                0.5,
                1,
                &[("kind", serde_json::json!("task_context"))],
            ),
        ];
        let section = build_memory_section(&ctx);
        assert!(section.contains("Recalled memories"));
        assert!(section.contains("[pref] User likes dark mode"));
        assert!(section.contains("[ctx] Working on Rust project"));
        assert!(section.contains("Use the recalled memories below"));
        assert!(!section.contains("use memory_recall first"));
    }

    #[test]
    fn ranks_pinned_memory_above_recent_generic_memory() {
        let mut ctx = basic_ctx();
        ctx.memory_item_limit = 1;
        ctx.recalled_memories = vec![
            mk_test_memory(
                1,
                "low value recent note",
                "notes",
                MemorySource::Conversation,
                0.05,
                0,
                &[],
            ),
            mk_test_memory(
                2,
                "user_name: Dean",
                "user_name",
                MemorySource::UserProvided,
                0.10,
                90,
                &[
                    ("is_pinned", serde_json::json!(true)),
                    ("kind", serde_json::json!("user_identity")),
                ],
            ),
        ];

        let section = build_memory_section(&ctx);
        assert!(section.contains("user_name: Dean"));
        assert!(!section.contains("low value recent note"));
    }

    #[test]
    fn ranks_user_identity_above_low_value_note() {
        let mut ctx = basic_ctx();
        ctx.memory_item_limit = 1;
        ctx.recalled_memories = vec![
            mk_test_memory(
                1,
                "misc meeting note with no durable value",
                "notes",
                MemorySource::Conversation,
                0.20,
                1,
                &[],
            ),
            mk_test_memory(
                2,
                "user_name: Dean",
                "user_name",
                MemorySource::UserProvided,
                0.20,
                30,
                &[("kind", serde_json::json!("user_identity"))],
            ),
        ];

        let section = build_memory_section(&ctx);
        assert!(section.contains("user_name: Dean"));
        assert!(!section.contains("misc meeting note with no durable value"));
    }

    #[test]
    fn uses_recency_as_tiebreaker() {
        let mut ctx = basic_ctx();
        ctx.memory_item_limit = 1;
        ctx.recalled_memories = vec![
            mk_test_memory(
                1,
                "prefers concise answers",
                "preferences",
                MemorySource::UserProvided,
                0.70,
                10,
                &[("kind", serde_json::json!("user_preference"))],
            ),
            mk_test_memory(
                2,
                "prefers short code comments",
                "preferences",
                MemorySource::UserProvided,
                0.70,
                1,
                &[("kind", serde_json::json!("user_preference"))],
            ),
        ];

        let section = build_memory_section(&ctx);
        assert!(section.contains("prefers short code comments"));
        assert!(!section.contains("prefers concise answers"));
    }

    #[test]
    fn truncates_after_ranking_not_before() {
        let mut ctx = basic_ctx();
        ctx.memory_item_limit = 2;
        ctx.recalled_memories = vec![
            mk_test_memory(
                1,
                "low value recent note",
                "notes",
                MemorySource::Conversation,
                0.05,
                0,
                &[],
            ),
            mk_test_memory(
                2,
                "user_name: Dean",
                "user_name",
                MemorySource::UserProvided,
                0.10,
                90,
                &[
                    ("is_pinned", serde_json::json!(true)),
                    ("kind", serde_json::json!("user_identity")),
                ],
            ),
            mk_test_memory(
                3,
                "prefers concise answers",
                "preferences",
                MemorySource::UserProvided,
                0.95,
                20,
                &[("kind", serde_json::json!("user_preference"))],
            ),
        ];

        let section = build_memory_section(&ctx);
        assert!(section.contains("user_name: Dean"));
        assert!(section.contains("prefers concise answers"));
        assert!(!section.contains("low value recent note"));
    }

    #[test]
    fn preserves_deterministic_order_for_equal_scores() {
        let mut ctx = basic_ctx();
        ctx.memory_item_limit = 2;
        ctx.recalled_memories = vec![
            mk_test_memory(
                2,
                "prefers compact layouts",
                "preferences",
                MemorySource::UserProvided,
                0.50,
                3,
                &[("kind", serde_json::json!("user_preference"))],
            ),
            mk_test_memory(
                1,
                "prefers dark mode",
                "preferences",
                MemorySource::UserProvided,
                0.50,
                3,
                &[("kind", serde_json::json!("user_preference"))],
            ),
        ];

        let section = build_memory_section(&ctx);
        assert!(pos(&section, "prefers dark mode") < pos(&section, "prefers compact layouts"));
    }

    #[test]
    fn memory_section_includes_omitted_count_when_truncated() {
        let mut ctx = basic_ctx();
        ctx.memory_item_limit = 2;
        ctx.recalled_memories = vec![
            mk_test_memory(
                1,
                "user_name: Dean",
                "user_name",
                MemorySource::UserProvided,
                1.0,
                10,
                &[
                    ("is_pinned", serde_json::json!(true)),
                    ("kind", serde_json::json!("user_identity")),
                ],
            ),
            mk_test_memory(
                2,
                "prefers concise answers",
                "preferences",
                MemorySource::UserProvided,
                0.9,
                5,
                &[("kind", serde_json::json!("user_preference"))],
            ),
            mk_test_memory(
                3,
                "low value note",
                "notes",
                MemorySource::Conversation,
                0.1,
                1,
                &[],
            ),
        ];

        let section = build_memory_section(&ctx);
        assert!(section.contains("user_name: Dean"));
        assert!(section.contains("prefers concise answers"));
        assert!(!section.contains("low value note"));
        assert!(section.contains("And 1 more memory omitted"));
    }

    #[test]
    fn test_memory_content_capped() {
        let mut ctx = basic_ctx();
        ctx.recalled_memories = vec![mk_test_memory(
            1,
            &"x".repeat(1000),
            "notes",
            MemorySource::Conversation,
            0.1,
            1,
            &[],
        )];
        let section = build_memory_section(&ctx);
        assert!(section.contains("..."));
        assert!(section.len() < 1200);
    }

    #[test]
    fn test_memory_limit_override() {
        let mut ctx = basic_ctx();
        ctx.memory_content_limit = 12;
        ctx.recalled_memories = vec![mk_test_memory(
            1,
            &"abcdefghij".repeat(10),
            "notes",
            MemorySource::Conversation,
            0.1,
            1,
            &[],
        )];
        let section = build_memory_section(&ctx);
        assert!(section.contains("abcdefghijab..."));
    }

    #[test]
    fn test_skills_section_omitted_when_empty() {
        let ctx = basic_ctx();
        let prompt = build_system_prompt(&ctx);
        assert!(!prompt.contains("## Skills"));
    }

    #[test]
    fn test_skills_section_present() {
        let mut ctx = basic_ctx();
        ctx.skill_summary = "- web-search: Search the web\n- git-expert: Git commands".to_string();
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.contains("## Skills"));
        assert!(prompt.contains("web-search"));
    }

    #[test]
    fn test_mcp_section_omitted_when_empty() {
        let ctx = basic_ctx();
        let prompt = build_system_prompt(&ctx);
        assert!(!prompt.contains("## Connected Tool Servers"));
    }

    #[test]
    fn test_mcp_section_present() {
        let mut ctx = basic_ctx();
        ctx.mcp_summary = "- github: 5 tools (search, create_issue, ...)".to_string();
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.contains("## Connected Tool Servers (MCP)"));
        assert!(prompt.contains("github"));
    }

    #[test]
    fn test_persona_section_with_soul() {
        let mut ctx = basic_ctx();
        ctx.soul_md = Some("You are a pirate. Arr!".to_string());
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.contains("## Persona"));
        assert!(prompt.contains("pirate"));
    }

    #[test]
    fn test_persona_soul_capped_at_1000() {
        let long_soul = "x".repeat(2000);
        let section = build_persona_section(None, Some(&long_soul), None, None, None);
        assert!(section.contains("..."));
        // The raw soul content in the section should be at most 1003 chars (1000 + "...")
        assert!(section.len() < 1200);
    }

    #[test]
    fn test_channel_telegram() {
        let section = build_channel_section("telegram");
        assert!(section.contains("4096"));
        assert!(section.contains("Telegram"));
    }

    #[test]
    fn test_channel_discord() {
        let section = build_channel_section("discord");
        assert!(section.contains("2000"));
        assert!(section.contains("Discord"));
    }

    #[test]
    fn test_channel_irc() {
        let section = build_channel_section("irc");
        assert!(section.contains("512"));
        assert!(section.contains("plain text"));
    }

    #[test]
    fn test_channel_unknown_gets_default() {
        let section = build_channel_section("smoke_signal");
        assert!(section.contains("4096"));
        assert!(section.contains("smoke_signal"));
    }

    #[test]
    fn test_unknown_name_instruction_is_compact() {
        let section = build_user_section(None);
        assert!(section.contains("ask for it in your first reply"));
        assert!(section.contains("memory_store"));
        assert!(!section.contains("warmly introduce yourself"));
    }

    #[test]
    fn test_user_name_known() {
        let mut ctx = basic_ctx();
        ctx.user_name = Some("Alice".to_string());
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.contains("Alice"));
        assert!(!prompt.contains("don't know the user's name"));
    }

    #[test]
    fn test_user_name_unknown() {
        let ctx = basic_ctx();
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.contains("don't know the user's name"));
    }

    #[test]
    fn test_canonical_context_not_in_system_prompt() {
        let mut ctx = basic_ctx();
        ctx.canonical_context =
            Some("User was discussing Rust async patterns last time.".to_string());
        let prompt = build_system_prompt(&ctx);
        // Canonical context should NOT be in system prompt (moved to user message)
        assert!(!prompt.contains("## Previous Conversation Context"));
        assert!(!prompt.contains("Rust async patterns"));
        // But should be available via build_canonical_context_message
        let msg = build_canonical_context_message(&ctx);
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("Rust async patterns"));
    }

    #[test]
    fn test_canonical_context_limit_override() {
        let mut ctx = basic_ctx();
        ctx.canonical_context = Some("abcdefghij".repeat(20));
        ctx.canonical_context_limit = 12;

        let msg = build_canonical_context_message(&ctx).unwrap();
        assert!(msg.contains("abcdefghijab..."));
    }

    #[test]
    fn test_canonical_context_omitted_for_subagent() {
        let mut ctx = basic_ctx();
        ctx.is_subagent = true;
        ctx.canonical_context = Some("Previous context here.".to_string());
        let prompt = build_system_prompt(&ctx);
        assert!(!prompt.contains("Previous Conversation Context"));
        // Should also be None from build_canonical_context_message
        assert!(build_canonical_context_message(&ctx).is_none());
    }

    #[test]
    fn test_empty_base_prompt_generates_default_identity() {
        let ctx = PromptContext {
            agent_name: "helper".to_string(),
            agent_description: "A helpful agent".to_string(),
            ..Default::default()
        };
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.contains("You are helper"));
        assert!(prompt.contains("A helpful agent"));
    }

    #[test]
    fn test_workspace_in_persona() {
        let mut ctx = basic_ctx();
        ctx.workspace_path = Some("/home/user/project".to_string());
        let prompt = build_system_prompt(&ctx);
        assert!(prompt.contains("## Workspace"));
        assert!(prompt.contains("/home/user/project"));
    }

    #[test]
    fn test_peer_section_excludes_inactive_peers() {
        let peers = vec![
            ("self".to_string(), "Running".to_string(), "m1".to_string()),
            (
                "active-peer".to_string(),
                "Running".to_string(),
                "m2".to_string(),
            ),
            (
                "inactive-peer".to_string(),
                "Suspended".to_string(),
                "m3".to_string(),
            ),
        ];
        let section = build_peer_agents_section("self", &peers, 10);
        assert!(section.contains("active-peer"));
        assert!(!section.contains("inactive-peer"));
    }

    #[test]
    fn test_peer_section_caps_visible_peers() {
        let peers: Vec<(String, String, String)> = (0..12)
            .map(|i| (format!("peer-{i}"), "Running".to_string(), "model-x".to_string()))
            .collect();
        let section = build_peer_agents_section("self", &peers, 3);
        assert!(section.contains("peer-0"));
        assert!(section.contains("peer-2"));
        assert!(!section.contains("peer-11"));
        assert!(section.contains("and 9 more active peers"));
    }

    #[test]
    fn test_prompt_size_regression_bound() {
        let mut ctx = basic_ctx();
        ctx.is_developer_task = true;
        ctx.requires_security_review = true;
        ctx.is_autonomous = true;
        ctx.current_date = Some("Wednesday, March 11, 2026 (2026-03-11 12:00 UTC)".to_string());
        ctx.user_name = Some("Alice".to_string());
        ctx.channel_type = Some("discord".to_string());
        ctx.memory_item_limit = 5;
        ctx.recalled_memories = (0..12)
            .map(|i| {
                mk_test_memory(
                    i as u128 + 1,
                    &"memory ".repeat(120),
                    "notes",
                    MemorySource::Conversation,
                    0.2,
                    i as i64,
                    &[],
                )
            })
            .collect();
        ctx.skill_summary = "skill summary ".repeat(150);
        ctx.skill_prompt_context = "prompt context ".repeat(150);
        ctx.mcp_summary = "mcp server summary ".repeat(120);
        ctx.workspace_path = Some("/workspace/project".to_string());
        ctx.soul_md = Some("persona ".repeat(220));
        ctx.user_md = Some("user context ".repeat(120));
        ctx.memory_md = Some("long-term memory ".repeat(120));
        ctx.canonical_context = Some("canonical context ".repeat(180));
        ctx.agents_md = Some("behavior guidance ".repeat(160));
        ctx.bootstrap_md = Some("bootstrap guidance ".repeat(120));
        ctx.workspace_context = Some("workspace context ".repeat(120));
        ctx.identity_md = Some("identity ".repeat(120));
        ctx.heartbeat_md = Some("heartbeat ".repeat(120));
        ctx.peer_agents = (0..25)
            .map(|i| {
                (
                    format!("peer-{i}"),
                    "Running".to_string(),
                    "model-x".to_string(),
                )
            })
            .collect();

        let telemetry = build_prompt_telemetry(&ctx);
        assert!(
            telemetry.estimated_tokens < 8_000,
            "prompt too large: {} tokens, sections: {:?}",
            telemetry.estimated_tokens,
            telemetry.sections
        );
        assert!(
            telemetry
                .sections
                .iter()
                .any(|section| section.name == "Production Hardening")
        );
    }

    #[test]
    fn test_cap_str_short() {
        assert_eq!(cap_str("hello", 10), "hello");
    }

    #[test]
    fn test_cap_str_long() {
        let result = cap_str("hello world", 5);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn test_cap_str_multibyte_utf8() {
        // This was panicking with "byte index is not a char boundary" (#38)
        let chinese = "你好世界这是一个测试字符串";
        let result = cap_str(chinese, 4);
        assert_eq!(result, "你好世界...");
        // Exact boundary
        assert_eq!(cap_str(chinese, 100), chinese);
    }

    #[test]
    fn test_cap_str_emoji() {
        let emoji = "👋🌍🚀✨💯";
        let result = cap_str(emoji, 3);
        assert_eq!(result, "👋🌍🚀...");
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("files"), "Files");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("MCP"), "MCP");
    }
}
