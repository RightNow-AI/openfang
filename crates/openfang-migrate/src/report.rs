//! Migration report generation.

use std::fmt;

/// Summary of a migration run.
#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
    /// Source framework name.
    pub source: String,
    /// Items that were successfully imported.
    pub imported: Vec<MigrateItem>,
    /// Items that were skipped (with reason).
    pub skipped: Vec<SkippedItem>,
    /// Warnings generated during migration.
    pub warnings: Vec<String>,
    /// Whether this was a dry run.
    pub dry_run: bool,
}

/// A successfully imported item.
#[derive(Debug, Clone)]
pub struct MigrateItem {
    /// What type of item (agent, config, memory, session, skill, channel).
    pub kind: ItemKind,
    /// Name or identifier.
    pub name: String,
    /// Destination path.
    pub destination: String,
}

/// An item that was skipped.
#[derive(Debug, Clone)]
pub struct SkippedItem {
    /// What type of item.
    pub kind: ItemKind,
    /// Name or identifier.
    pub name: String,
    /// Why it was skipped.
    pub reason: String,
}

/// The type of migrated item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Config,
    Agent,
    Memory,
    Session,
    Skill,
    Channel,
    Secret,
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config => write!(f, "Config"),
            Self::Agent => write!(f, "Agent"),
            Self::Memory => write!(f, "Memory"),
            Self::Session => write!(f, "Session"),
            Self::Skill => write!(f, "Skill"),
            Self::Channel => write!(f, "Channel"),
            Self::Secret => write!(f, "Secret"),
        }
    }
}

impl MigrationReport {
    /// Generate a human-readable Markdown summary.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        let mode = if self.dry_run { " (Dry Run)" } else { "" };

        out.push_str(&format!(
            "# Migration Report: {} -> OMTAE{}\n\n",
            self.source, mode
        ));

        // Summary
        out.push_str("## Summary\n\n");
        out.push_str(&format!("- Imported: {} items\n", self.imported.len()));
        out.push_str(&format!("- Skipped: {} items\n", self.skipped.len()));
        out.push_str(&format!("- Warnings: {}\n\n", self.warnings.len()));

        // Imported
        if !self.imported.is_empty() {
            out.push_str("## Imported\n\n");
            out.push_str("| Type | Name | Destination |\n");
            out.push_str("|------|------|-------------|\n");
            for item in &self.imported {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    item.kind, item.name, item.destination
                ));
            }
            out.push('\n');
        }

        // Skipped
        if !self.skipped.is_empty() {
            out.push_str("## Skipped\n\n");
            out.push_str("| Type | Name | Reason |\n");
            out.push_str("|------|------|--------|\n");
            for item in &self.skipped {
                out.push_str(&format!(
                    "| {} | {} | {} |\n",
                    item.kind, item.name, item.reason
                ));
            }
            out.push('\n');
        }

        // Warnings
        if !self.warnings.is_empty() {
            out.push_str("## Warnings\n\n");
            for w in &self.warnings {
                out.push_str(&format!("- {w}\n"));
            }
            out.push('\n');
        }

        // Next steps
        out.push_str("## Next Steps\n\n");
        out.push_str("1. Review imported agent manifests in `~/.omtae/agents/`\n");
        out.push_str(
            "2. Review `~/.omtae/secrets.env` — verify tokens were migrated correctly\n",
        );
        out.push_str("3. Set any remaining API keys referenced in `~/.omtae/config.toml`\n");
        out.push_str("4. Start the daemon: `omtae start`\n");
        out.push_str("5. Test your agents: `omtae agent list`\n");

        out
    }

    /// Print the report to stdout in a friendly format.
    pub fn print_summary(&self) {
        let mode = if self.dry_run { " (dry run)" } else { "" };
        println!("\n  Migration complete!{mode}\n");
        println!("  Imported: {} items", self.imported.len());
        println!("  Skipped:  {} items", self.skipped.len());
        println!("  Warnings: {}", self.warnings.len());

        if !self.imported.is_empty() {
            println!("\n  Imported:");
            for item in &self.imported {
                println!("    [{}] {} -> {}", item.kind, item.name, item.destination);
            }
        }

        if !self.skipped.is_empty() {
            println!("\n  Skipped:");
            for item in &self.skipped {
                println!("    [{}] {} — {}", item.kind, item.name, item.reason);
            }
        }

        if !self.warnings.is_empty() {
            println!("\n  Warnings:");
            for w in &self.warnings {
                println!("    - {w}");
            }
        }

        if !self.dry_run {
            println!("\n  Next steps:");
            println!("    omtae start");
            println!("    omtae agent list");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_report() {
        let report = MigrationReport {
            source: "OpenClaw".to_string(),
            dry_run: false,
            ..Default::default()
        };
        let md = report.to_markdown();
        assert!(md.contains("Migration Report: OpenClaw"));
        assert!(md.contains("Imported: 0 items"));
    }

    #[test]
    fn test_report_with_items() {
        let report = MigrationReport {
            source: "OpenClaw".to_string(),
            imported: vec![MigrateItem {
                kind: ItemKind::Agent,
                name: "coder".to_string(),
                destination: "~/.omtae/agents/coder/agent.toml".to_string(),
            }],
            skipped: vec![SkippedItem {
                kind: ItemKind::Skill,
                name: "custom-skill".to_string(),
                reason: "Unsupported format".to_string(),
            }],
            warnings: vec!["API key not found".to_string()],
            dry_run: true,
        };
        let md = report.to_markdown();
        assert!(md.contains("(Dry Run)"));
        assert!(md.contains("coder"));
        assert!(md.contains("Unsupported format"));
        assert!(md.contains("API key not found"));
    }
}
