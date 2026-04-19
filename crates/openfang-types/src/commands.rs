//! Unified slash command registry.
//!
//! This module is the single source of truth for every slash command that can be
//! dispatched across CLI, channel adapters (Telegram/Slack/etc.), and the web
//! chat (WebSocket).
//!
//! Each dispatch site (there are three: `openfang-cli/src/tui/mod.rs`,
//! `openfang-channels/src/bridge.rs`, `openfang-api/src/ws.rs`) retains its own
//! handler logic. The registry is added as a front-door so command names and
//! aliases can be canonicalised once and help / autocomplete is generated from
//! a single list.
//!
//! # Example
//!
//! ```
//! use openfang_types::commands::{self, Surfaces};
//!
//! let def = commands::resolve("NEW").expect("new is registered");
//! assert_eq!(def.name, "new");
//! assert!(def.surfaces.contains(Surfaces::CHANNEL));
//!
//! // Autocomplete
//! let matches = commands::autocomplete("ne", Surfaces::CHANNEL);
//! assert!(matches.iter().any(|m| *m == "new"));
//!
//! // Unknown commands surface the help text so dispatch sites can return a
//! // helpful error message.
//! assert!(commands::resolve("not-a-real-command").is_none());
//! let help = commands::render_help(Surfaces::CLI);
//! assert!(help.contains("/help"));
//! ```

use bitflags::bitflags;
use serde::Serialize;

/// Command category used for help grouping.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub enum CommandCategory {
    /// Greet/welcome/help commands.
    General,
    /// Session-level commands (reset history, compact, switch model, cancel run, etc.).
    Session,
    /// Model selection & provider info.
    Model,
    /// Memory / storage commands.
    Memory,
    /// Control-plane commands (kill agent, spawn, stop).
    Control,
    /// Read-only information (status, list agents, list skills, etc.).
    Info,
    /// Automation: workflows, triggers, schedules, approvals.
    Automation,
    /// Monitoring: budget, peers, external agents.
    Monitoring,
}

impl CommandCategory {
    /// Stable human-readable label (used for help section headers).
    pub fn label(self) -> &'static str {
        match self {
            CommandCategory::General => "General",
            CommandCategory::Session => "Session",
            CommandCategory::Model => "Model",
            CommandCategory::Memory => "Memory",
            CommandCategory::Control => "Control",
            CommandCategory::Info => "Info",
            CommandCategory::Automation => "Automation",
            CommandCategory::Monitoring => "Monitoring",
        }
    }
}

bitflags! {
    /// Which surfaces a command is visible / dispatchable on.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Surfaces: u8 {
        const CLI     = 0b001;
        const CHANNEL = 0b010;
        const WEB     = 0b100;
        const ALL     = Self::CLI.bits() | Self::CHANNEL.bits() | Self::WEB.bits();
    }
}

/// A single command definition.
#[derive(Clone, Copy, Debug)]
pub struct CommandDef {
    /// Canonical name (no leading slash).
    pub name: &'static str,
    /// Zero or more aliases resolving to the same command.
    pub aliases: &'static [&'static str],
    /// One-line description used for help rendering.
    pub description: &'static str,
    /// Grouping category.
    pub category: CommandCategory,
    /// Surfaces where the command is visible.
    pub surfaces: Surfaces,
    /// Whether the command requires an active agent in the current session.
    pub requires_agent: bool,
}

/// Every slash command registered in OpenFang.
///
/// Keep this list in sync with the three dispatch sites:
///   - `openfang-cli/src/tui/mod.rs::handle_slash_command`
///   - `openfang-channels/src/bridge.rs::handle_command`
///   - `openfang-api/src/ws.rs::handle_command`
pub const COMMAND_REGISTRY: &[CommandDef] = &[
    // ── General ────────────────────────────────────────────────────────────
    CommandDef {
        name: "help",
        aliases: &[],
        description: "Show available commands",
        category: CommandCategory::General,
        surfaces: Surfaces::ALL,
        requires_agent: false,
    },
    CommandDef {
        name: "start",
        aliases: &[],
        description: "Show welcome message",
        category: CommandCategory::General,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "exit",
        aliases: &["quit"],
        description: "End chat session / disconnect from agent",
        category: CommandCategory::General,
        surfaces: Surfaces::CLI,
        requires_agent: false,
    },
    // ── Session ────────────────────────────────────────────────────────────
    CommandDef {
        name: "new",
        aliases: &["reset"],
        description: "Reset session (clear history)",
        category: CommandCategory::Session,
        surfaces: Surfaces::CHANNEL.union(Surfaces::WEB),
        requires_agent: true,
    },
    CommandDef {
        name: "clear",
        aliases: &[],
        description: "Clear chat display",
        category: CommandCategory::Session,
        surfaces: Surfaces::CLI,
        requires_agent: false,
    },
    CommandDef {
        name: "compact",
        aliases: &[],
        description: "Trigger LLM session compaction",
        category: CommandCategory::Session,
        surfaces: Surfaces::CHANNEL.union(Surfaces::WEB),
        requires_agent: true,
    },
    CommandDef {
        name: "stop",
        aliases: &[],
        description: "Cancel current agent run",
        category: CommandCategory::Session,
        surfaces: Surfaces::CHANNEL.union(Surfaces::WEB),
        requires_agent: true,
    },
    CommandDef {
        name: "usage",
        aliases: &[],
        description: "Show session token usage and cost",
        category: CommandCategory::Session,
        surfaces: Surfaces::CHANNEL.union(Surfaces::WEB),
        requires_agent: true,
    },
    CommandDef {
        name: "think",
        aliases: &[],
        description: "Toggle extended thinking",
        category: CommandCategory::Session,
        surfaces: Surfaces::CHANNEL,
        requires_agent: true,
    },
    CommandDef {
        name: "context",
        aliases: &[],
        description: "Show context window usage & pressure",
        category: CommandCategory::Session,
        surfaces: Surfaces::WEB,
        requires_agent: true,
    },
    CommandDef {
        name: "verbose",
        aliases: &[],
        description: "Cycle tool detail level (off|on|full)",
        category: CommandCategory::Session,
        surfaces: Surfaces::WEB,
        requires_agent: false,
    },
    CommandDef {
        name: "queue",
        aliases: &[],
        description: "Check if agent is processing",
        category: CommandCategory::Session,
        surfaces: Surfaces::WEB,
        requires_agent: true,
    },
    // ── Model ──────────────────────────────────────────────────────────────
    CommandDef {
        name: "model",
        aliases: &[],
        description: "Show or switch model",
        category: CommandCategory::Model,
        surfaces: Surfaces::ALL,
        requires_agent: true,
    },
    CommandDef {
        name: "models",
        aliases: &[],
        description: "List available AI models",
        category: CommandCategory::Model,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "providers",
        aliases: &[],
        description: "Show configured providers",
        category: CommandCategory::Model,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    // ── Control ────────────────────────────────────────────────────────────
    CommandDef {
        name: "kill",
        aliases: &[],
        description: "Kill the current agent",
        category: CommandCategory::Control,
        surfaces: Surfaces::CLI,
        requires_agent: true,
    },
    // ── Info ───────────────────────────────────────────────────────────────
    CommandDef {
        name: "status",
        aliases: &[],
        description: "Show system/connection status",
        category: CommandCategory::Info,
        surfaces: Surfaces::CLI.union(Surfaces::CHANNEL),
        requires_agent: false,
    },
    CommandDef {
        name: "agents",
        aliases: &[],
        description: "List running agents",
        category: CommandCategory::Info,
        surfaces: Surfaces::CLI.union(Surfaces::CHANNEL),
        requires_agent: false,
    },
    CommandDef {
        name: "agent",
        aliases: &[],
        description: "Select agent (/agent <name>)",
        category: CommandCategory::Info,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "skills",
        aliases: &[],
        description: "List installed skills",
        category: CommandCategory::Info,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "hands",
        aliases: &[],
        description: "List available and active hands",
        category: CommandCategory::Info,
        surfaces: Surfaces::CLI.union(Surfaces::CHANNEL),
        requires_agent: false,
    },
    // ── Automation ─────────────────────────────────────────────────────────
    CommandDef {
        name: "workflows",
        aliases: &[],
        description: "List workflows",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "workflow",
        aliases: &[],
        description: "Run workflow (/workflow run <name> [input])",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "triggers",
        aliases: &[],
        description: "List event triggers",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "trigger",
        aliases: &[],
        description: "Manage triggers (/trigger add|del ...)",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "schedules",
        aliases: &[],
        description: "List cron jobs",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "schedule",
        aliases: &[],
        description: "Manage schedules (/schedule add|del|run ...)",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "approvals",
        aliases: &[],
        description: "List pending approvals",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "approve",
        aliases: &[],
        description: "Approve request (/approve <id>)",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    CommandDef {
        name: "reject",
        aliases: &[],
        description: "Reject request (/reject <id>)",
        category: CommandCategory::Automation,
        surfaces: Surfaces::CHANNEL,
        requires_agent: false,
    },
    // ── Monitoring ─────────────────────────────────────────────────────────
    CommandDef {
        name: "budget",
        aliases: &[],
        description: "Show spending limits and costs",
        category: CommandCategory::Monitoring,
        surfaces: Surfaces::CHANNEL.union(Surfaces::WEB),
        requires_agent: false,
    },
    CommandDef {
        name: "peers",
        aliases: &[],
        description: "Show OFP peer network status",
        category: CommandCategory::Monitoring,
        surfaces: Surfaces::CHANNEL.union(Surfaces::WEB),
        requires_agent: false,
    },
    CommandDef {
        name: "a2a",
        aliases: &[],
        description: "List discovered external A2A agents",
        category: CommandCategory::Monitoring,
        surfaces: Surfaces::CHANNEL.union(Surfaces::WEB),
        requires_agent: false,
    },
];

/// Strip a single leading `/` if present.
#[inline]
fn strip_leading_slash(s: &str) -> &str {
    s.strip_prefix('/').unwrap_or(s)
}

/// Look up a command by its canonical name or any alias (case-insensitive).
///
/// A leading `/` on the input is tolerated so callers do not have to strip it.
///
/// Returns `None` if the name doesn't match any registered command.
pub fn resolve(name: &str) -> Option<&'static CommandDef> {
    let needle = strip_leading_slash(name).trim();
    if needle.is_empty() {
        return None;
    }
    COMMAND_REGISTRY.iter().find(|def| {
        def.name.eq_ignore_ascii_case(needle)
            || def
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(needle))
    })
}

/// Iterator over every command visible on the given surface(s).
pub fn list_for_surface(s: Surfaces) -> impl Iterator<Item = &'static CommandDef> {
    COMMAND_REGISTRY
        .iter()
        .filter(move |def| def.surfaces.intersects(s))
}

/// Render a formatted help string grouped by [`CommandCategory`].
///
/// The output lists every command visible on `s` alongside its description and
/// aliases. Designed to be printed verbatim into chat.
pub fn render_help(s: Surfaces) -> String {
    // Stable category order for consistent help output.
    const CATEGORIES: &[CommandCategory] = &[
        CommandCategory::General,
        CommandCategory::Session,
        CommandCategory::Model,
        CommandCategory::Control,
        CommandCategory::Memory,
        CommandCategory::Info,
        CommandCategory::Automation,
        CommandCategory::Monitoring,
    ];

    let mut out = String::from("Available commands:");
    for category in CATEGORIES {
        let cmds: Vec<&CommandDef> = list_for_surface(s)
            .filter(|def| def.category == *category)
            .collect();
        if cmds.is_empty() {
            continue;
        }
        out.push_str("\n\n");
        out.push_str(category.label());
        out.push_str(":\n");
        for def in cmds {
            out.push_str("  /");
            out.push_str(def.name);
            if !def.aliases.is_empty() {
                out.push_str(" (aliases: ");
                for (i, a) in def.aliases.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push('/');
                    out.push_str(a);
                }
                out.push(')');
            }
            out.push_str(" — ");
            out.push_str(def.description);
            out.push('\n');
        }
        // Trim final newline from this category block.
        out.pop();
    }
    out
}

/// Return every command name or alias on `s` that starts with `prefix`
/// (case-insensitive, no leading slash).
pub fn autocomplete(prefix: &str, s: Surfaces) -> Vec<&'static str> {
    let needle = strip_leading_slash(prefix).to_ascii_lowercase();
    let mut out: Vec<&'static str> = Vec::new();
    for def in list_for_surface(s) {
        if def.name.to_ascii_lowercase().starts_with(&needle) {
            out.push(def.name);
        }
        for alias in def.aliases {
            if alias.to_ascii_lowercase().starts_with(&needle) {
                out.push(alias);
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// No command name or alias may appear twice across the registry.
    #[test]
    fn no_duplicate_names_or_aliases() {
        let mut seen: HashSet<&'static str> = HashSet::new();
        for def in COMMAND_REGISTRY {
            assert!(
                seen.insert(def.name),
                "duplicate command name in registry: /{}",
                def.name
            );
            for alias in def.aliases {
                assert!(
                    seen.insert(alias),
                    "duplicate command alias in registry: /{} (for /{})",
                    alias,
                    def.name
                );
            }
        }
    }

    #[test]
    fn resolve_canonical_name() {
        let def = resolve("new").expect("`new` must resolve");
        assert_eq!(def.name, "new");
    }

    #[test]
    fn resolve_is_case_insensitive() {
        let a = resolve("new").unwrap();
        let b = resolve("NEW").unwrap();
        let c = resolve("New").unwrap();
        assert_eq!(a.name, b.name);
        assert_eq!(a.name, c.name);
    }

    #[test]
    fn resolve_tolerates_leading_slash() {
        let a = resolve("/new").unwrap();
        let b = resolve("new").unwrap();
        assert_eq!(a.name, b.name);
    }

    #[test]
    fn resolve_alias_hits_same_command() {
        // `reset` is registered as an alias of `new`.
        let via_name = resolve("new").unwrap();
        let via_alias = resolve("reset").unwrap();
        assert_eq!(via_name.name, via_alias.name);
        // And `quit` is an alias of `exit`.
        let via_alias = resolve("quit").unwrap();
        assert_eq!(via_alias.name, "exit");
    }

    #[test]
    fn resolve_unknown_returns_none() {
        assert!(resolve("definitely-not-a-command").is_none());
        assert!(resolve("").is_none());
        assert!(resolve("/").is_none());
    }

    #[test]
    fn list_for_surface_channel_only_includes_channel_commands() {
        for def in list_for_surface(Surfaces::CHANNEL) {
            assert!(
                def.surfaces.contains(Surfaces::CHANNEL),
                "/{} was returned for CHANNEL but doesn't declare CHANNEL surface",
                def.name
            );
        }
        // Sanity: `start` is channel-only, `kill` is CLI-only.
        let channel_names: Vec<&'static str> = list_for_surface(Surfaces::CHANNEL)
            .map(|d| d.name)
            .collect();
        assert!(channel_names.contains(&"start"));
        assert!(!channel_names.contains(&"kill"));
    }

    #[test]
    fn list_for_surface_cli_includes_cli_only_commands() {
        let cli: Vec<&'static str> = list_for_surface(Surfaces::CLI).map(|d| d.name).collect();
        assert!(cli.contains(&"kill"));
        assert!(cli.contains(&"clear"));
        assert!(cli.contains(&"exit"));
        // `start` is channel-only, should not appear on CLI.
        assert!(!cli.contains(&"start"));
    }

    #[test]
    fn list_for_surface_web_includes_web_only_commands() {
        let web: Vec<&'static str> = list_for_surface(Surfaces::WEB).map(|d| d.name).collect();
        assert!(web.contains(&"context"));
        assert!(web.contains(&"verbose"));
        assert!(web.contains(&"queue"));
    }

    #[test]
    fn render_help_mentions_every_command_on_surface() {
        for surface in [Surfaces::CLI, Surfaces::CHANNEL, Surfaces::WEB] {
            let help = render_help(surface);
            for def in list_for_surface(surface) {
                let needle = format!("/{}", def.name);
                assert!(
                    help.contains(&needle),
                    "help for {:?} is missing `{}`",
                    surface,
                    needle
                );
            }
        }
    }

    #[test]
    fn render_help_groups_by_category() {
        let help = render_help(Surfaces::CHANNEL);
        // At minimum we expect the general + session + info sections.
        assert!(help.contains("General:"));
        assert!(help.contains("Session:"));
        assert!(help.contains("Info:"));
    }

    #[test]
    fn autocomplete_prefix_matches_canonical_name() {
        let matches = autocomplete("ne", Surfaces::CHANNEL);
        assert!(
            matches.contains(&"new"),
            "autocomplete(`ne`) must include `new`, got {matches:?}"
        );
    }

    #[test]
    fn autocomplete_prefix_matches_alias() {
        let matches = autocomplete("res", Surfaces::CHANNEL);
        assert!(
            matches.contains(&"reset"),
            "autocomplete(`res`) must include `reset` (alias of new), got {matches:?}"
        );
    }

    #[test]
    fn autocomplete_empty_prefix_returns_all_for_surface() {
        let matches = autocomplete("", Surfaces::CLI);
        let cli_total = list_for_surface(Surfaces::CLI).count()
            + list_for_surface(Surfaces::CLI)
                .map(|d| d.aliases.len())
                .sum::<usize>();
        assert_eq!(matches.len(), cli_total);
    }

    #[test]
    fn autocomplete_respects_surface_filter() {
        // `kill` is CLI-only; it must not appear in CHANNEL autocomplete.
        let channel_matches = autocomplete("ki", Surfaces::CHANNEL);
        assert!(!channel_matches.contains(&"kill"));
        let cli_matches = autocomplete("ki", Surfaces::CLI);
        assert!(cli_matches.contains(&"kill"));
    }

    #[test]
    fn autocomplete_tolerates_leading_slash() {
        let a = autocomplete("/new", Surfaces::CHANNEL);
        let b = autocomplete("new", Surfaces::CHANNEL);
        assert_eq!(a, b);
    }

    /// Every command must declare at least one surface.
    #[test]
    fn every_command_has_at_least_one_surface() {
        for def in COMMAND_REGISTRY {
            assert!(
                !def.surfaces.is_empty(),
                "/{} declares no surfaces",
                def.name
            );
        }
    }

    /// Unknown-command error path rendering — doc-example-style integration check.
    #[test]
    fn unknown_command_help_rendering_is_useful() {
        // Simulated "dispatch unknown command" error path.
        let name = "zzz-not-a-command";
        let def = resolve(name);
        assert!(def.is_none());
        let help = render_help(Surfaces::CHANNEL);
        let err = format!("Unknown command: /{name}\n\n{help}");
        assert!(err.contains("Unknown command:"));
        assert!(err.contains("/help"));
    }
}
