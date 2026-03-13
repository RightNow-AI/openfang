//! Planner domain types for the Personal Chief of Staff product slice.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlannerInboxStatus {
    #[default]
    Captured,
    Clarified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlannerTaskStatus {
    #[default]
    Todo,
    InProgress,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PriorityBand {
    Low,
    #[default]
    Medium,
    High,
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnergyLevel {
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlannerProjectStatus {
    #[default]
    Active,
    OnHold,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlannerReviewScope {
    #[default]
    Shutdown,
    Weekly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlannerRecommendationConfidence {
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannerAgentRecommendation {
    pub agent_id: String,
    pub name: String,
    pub reason: String,
    pub confidence: PlannerRecommendationConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannerAgentCatalogEntry {
    pub agent_id: String,
    pub name: String,
    pub description: String,
    pub purpose: String,
    pub best_for: String,
    pub avoid_for: String,
    pub example: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannerInboxItem {
    pub id: String,
    pub text: String,
    pub status: PlannerInboxStatus,
    pub created_at: DateTime<Utc>,
    pub clarified_at: Option<DateTime<Utc>>,
    pub task_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannerTask {
    pub id: String,
    pub title: String,
    pub status: PlannerTaskStatus,
    pub priority: PriorityBand,
    pub effort_minutes: Option<u32>,
    pub energy: EnergyLevel,
    pub project_id: Option<String>,
    pub due_at: Option<DateTime<Utc>>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub blocked_by: Vec<String>,
    pub next_action: String,
    pub source_inbox_item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_recommendation: Option<PlannerAgentRecommendation>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannerProject {
    pub id: String,
    pub title: String,
    pub outcome: String,
    pub status: PlannerProjectStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannerRoutine {
    pub id: String,
    pub name: String,
    pub trigger: String,
    pub thread_label: String,
    pub active: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlannerReview {
    pub id: String,
    pub scope: PlannerReviewScope,
    pub created_at: DateTime<Utc>,
    pub summary: String,
    pub wins: Vec<String>,
    pub misses: Vec<String>,
    pub adjustments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PlannerTodayPlan {
    pub date: String,
    pub daily_outcome: String,
    pub must_do: Vec<PlannerTask>,
    pub should_do: Vec<PlannerTask>,
    pub could_do: Vec<PlannerTask>,
    pub blockers: Vec<String>,
    pub focus_suggestion: Option<PlannerTask>,
    pub rebuilt_at: Option<DateTime<Utc>>,
}
