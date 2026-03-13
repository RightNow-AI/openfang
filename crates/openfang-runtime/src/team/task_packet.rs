//! Task packet — the atomic unit of work in the team coordination model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

fn default_now() -> DateTime<Utc> {
    Utc::now()
}

/// Priority band for scheduling and coordinator attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

/// What the task must produce and where it must land for the coordinator to accept it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputContract {
    /// Expected artifact format (e.g. "JavaScript ESM module", "JSON doc", "Markdown checklist").
    pub format: String,
    /// Relative paths or glob patterns the output must satisfy.
    pub location: Vec<String>,
    /// Agent ID that reviews and accepts the output before the task is marked Done.
    pub reviewer: String,
}

/// A self-contained unit of work assigned to one owner agent.
///
/// Every field is designed to be serializable so packets can be persisted,
/// loaded from JSON files on disk, or passed over the A2A wire protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPacket {
    /// Unique stable identifier for this task.
    pub task_id: String,
    /// Parent task that spawned this one, if any (subagent work).
    pub parent_task_id: Option<String>,
    /// Agent ID of the worker or coordinator that owns this task.
    pub owner_agent_id: String,
    /// One sentence describing the outcome required.
    pub goal: String,
    /// Files or subsystems this task is allowed to touch.
    pub scope: Vec<String>,
    /// Inputs this task reads (file paths, API endpoints, env vars).
    pub inputs: Vec<String>,
    /// task_ids that must be Done before this task can move to InProgress.
    pub dependencies: Vec<String>,
    /// Hard constraints that must not be violated.
    pub constraints: Vec<String>,
    /// Tool names or categories the worker may use.
    pub tools_allowed: Vec<String>,
    /// Verifiable conditions that, when all satisfied, allow marking this task Done.
    pub done_criteria: Vec<String>,
    /// Conditions under which the worker must escalate to the coordinator.
    pub escalation_rules: Vec<String>,
    /// Specifies the required output artifact.
    pub output_contract: OutputContract,
    /// Scheduling priority.
    #[serde(default)]
    pub priority: TaskPriority,
    /// Wall-clock time the packet was created.
    #[serde(default = "default_now")]
    pub created_at: DateTime<Utc>,
}

impl TaskPacket {
    /// Construct a minimal packet with the required fields only.
    pub fn new(
        task_id: impl Into<String>,
        owner_agent_id: impl Into<String>,
        goal: impl Into<String>,
        output_contract: OutputContract,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            parent_task_id: None,
            owner_agent_id: owner_agent_id.into(),
            goal: goal.into(),
            scope: Vec::new(),
            inputs: Vec::new(),
            dependencies: Vec::new(),
            constraints: Vec::new(),
            tools_allowed: Vec::new(),
            done_criteria: Vec::new(),
            escalation_rules: Vec::new(),
            output_contract,
            priority: TaskPriority::Normal,
            created_at: Utc::now(),
        }
    }
}
