//! # maestro-pai
//!
//! PAI (Personal AI Infrastructure) self-evolution framework, implementing
//! the core concepts from Daniel Miessler's PAI v4.0.3.
//!
//! ## What PAI Actually Is (Honest Assessment)
//!
//! PAI is a **prompt architecture**, not a software framework. It consists of:
//! - A 7-Phase Algorithm (383-line markdown file for Claude Code)
//! - Skills (markdown workflow documents)
//! - Learning hooks (structured JSONL feedback logs)
//! - Self-evolution tools (mine logs → propose algorithm changes)
//! - TELOS context (10 markdown files capturing user identity)
//!
//! ## What This Crate Does
//!
//! Translates PAI's prompt-level concepts into structured Rust types
//! and persistence layers that integrate with OpenFang's skill system
//! and memory system. Specifically:
//!
//! 1. **Learning Hooks** — Capture structured feedback from every task
//! 2. **Pattern Synthesis** — Mine accumulated learnings for patterns
//! 3. **Wisdom Frames** — Crystallize domain knowledge into searchable units
//! 4. **Algorithm Upgrade** — Propose changes to the 7-phase algorithm
//!    based on accumulated patterns
//!
//! ## What Maestro Got Wrong
//!
//! Maestro's `pai_core` (1,650 LOC) and `fabric_core` (850 LOC) tried to
//! compile PAI's prompt patterns into Rust code. The Stitch module defined
//! `Pattern`, `Weave`, `Loom` types that had no actual implementation.
//! The Fabric integration was a stub that returned placeholder strings.
//!
//! ## What This Crate Gets Right (Hopefully)
//!
//! It does NOT try to replicate PAI's prompt architecture in code.
//! Instead, it provides the **data layer** that PAI's prompts need:
//! - Structured storage for learnings (SQLite, not JSONL)
//! - Query interface for pattern mining
//! - Versioned algorithm storage with diff tracking
//! - TELOS context management
//!
//! ## HONEST GAPS
//!
//! - Pattern synthesis still requires an LLM call (this crate stores data,
//!   the LLM does the actual synthesis)
//! - Algorithm upgrade proposals are suggestions, not auto-applied changes
//! - No evaluation of whether upgrades actually improve performance
//! - TELOS context is user-provided, not automatically inferred
//! - The "self-evolving" claim is aspirational — it's really "self-logging
//!   with periodic human-reviewed upgrades"

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod hooks;
pub mod patterns;
pub mod telos;
pub mod wisdom;

/// A structured learning captured from a task execution.
///
/// This is the atomic unit of PAI's self-evolution system.
/// Learnings accumulate over time and are periodically mined
/// for patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    /// The task that produced this learning.
    pub task_id: String,
    /// Category: ALGORITHM, SYSTEM, FAILURE, SYNTHESIS, REFLECTION
    pub category: String,
    /// The actual insight.
    pub insight: String,
    /// Context in which the insight was discovered.
    pub context: String,
    /// Whether this insight is actionable.
    pub actionable: bool,
    /// User rating of the task outcome (1-5, None if not rated).
    pub user_rating: Option<u8>,
    /// Tags for searchability.
    pub tags: Vec<String>,
}

/// A synthesized pattern — extracted from multiple learnings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    /// The learning IDs that contributed to this pattern.
    pub source_learning_ids: Vec<Uuid>,
    /// How many times this pattern has been observed.
    pub occurrence_count: u32,
    /// Confidence that this pattern is real (0.0 - 1.0).
    pub confidence: f64,
    /// Suggested action based on this pattern.
    pub suggested_action: String,
    pub discovered_at: DateTime<Utc>,
}

/// A wisdom frame — crystallized domain knowledge.
///
/// From PAI: "A wisdom frame is a self-contained unit of knowledge
/// that can be retrieved and applied to future tasks."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WisdomFrame {
    pub id: Uuid,
    pub domain: String,
    pub title: String,
    pub content: String,
    /// Source patterns that led to this wisdom.
    pub source_pattern_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub last_applied_at: Option<DateTime<Utc>>,
    /// How many times this wisdom has been applied.
    pub application_count: u32,
}

/// An algorithm version — tracks changes to the 7-phase algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgorithmVersion {
    pub version: String,
    pub description: String,
    /// The full algorithm text (markdown).
    pub algorithm_text: String,
    /// Diff from previous version.
    pub diff_from_previous: Option<String>,
    /// Patterns that motivated this change.
    pub motivating_pattern_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    /// Performance metrics after this version was deployed.
    pub performance_metrics: Option<serde_json::Value>,
}

/// TELOS context — deep user identity for personalization.
///
/// From PAI v4.0.3: 10 markdown files capturing:
/// MISSION, GOALS, PROJECTS, BELIEFS, PREFERENCES,
/// HABITS, RELATIONSHIPS, HEALTH, FINANCES, CALENDAR
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelosContext {
    pub entries: std::collections::HashMap<String, String>,
    pub last_updated: DateTime<Utc>,
}
