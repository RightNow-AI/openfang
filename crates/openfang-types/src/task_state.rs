//! Durable task state model for long-running orchestration.
//!
//! Defines canonical task states and per-state timestamps so task progress can
//! be serialized, persisted, and recovered across sessions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Canonical state for orchestrated tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionState {
    Pending,
    InProgress,
    Failed,
    Blocked,
    Done,
    Canceled,
}

impl TaskExecutionState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Canceled)
    }
}

/// Timestamp record for each task state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskStateTimestamps {
    pub pending_at: DateTime<Utc>,
    pub in_progress_at: Option<DateTime<Utc>>,
    pub failed_at: Option<DateTime<Utc>>,
    pub blocked_at: Option<DateTime<Utc>>,
    pub done_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl TaskStateTimestamps {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            pending_at: now,
            in_progress_at: None,
            failed_at: None,
            blocked_at: None,
            done_at: None,
            canceled_at: None,
            updated_at: now,
        }
    }

    pub fn mark(&mut self, state: TaskExecutionState, at: DateTime<Utc>) {
        match state {
            TaskExecutionState::Pending => self.pending_at = at,
            TaskExecutionState::InProgress => self.in_progress_at = Some(at),
            TaskExecutionState::Failed => self.failed_at = Some(at),
            TaskExecutionState::Blocked => self.blocked_at = Some(at),
            TaskExecutionState::Done => self.done_at = Some(at),
            TaskExecutionState::Canceled => self.canceled_at = Some(at),
        }
        self.updated_at = at;
    }
}

/// Durable task state + transition metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableTaskState {
    pub state: TaskExecutionState,
    pub timestamps: TaskStateTimestamps,
}

impl DurableTaskState {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            state: TaskExecutionState::Pending,
            timestamps: TaskStateTimestamps::new(now),
        }
    }

    pub fn transition(
        &mut self,
        next: TaskExecutionState,
        at: DateTime<Utc>,
    ) -> Result<(), String> {
        if self.state == next {
            self.timestamps.updated_at = at;
            return Ok(());
        }

        if self.state.is_terminal() {
            return Err(format!(
                "cannot transition from terminal state {:?} to {:?}",
                self.state, next
            ));
        }

        if !is_valid_transition(self.state, next) {
            return Err(format!(
                "invalid state transition: {:?} -> {:?}",
                self.state, next
            ));
        }

        self.state = next;
        self.timestamps.mark(next, at);
        Ok(())
    }
}

fn is_valid_transition(current: TaskExecutionState, next: TaskExecutionState) -> bool {
    use TaskExecutionState as S;
    matches!(
        (current, next),
        (
            S::Pending,
            S::InProgress | S::Failed | S::Blocked | S::Canceled
        ) | (
            S::InProgress,
            S::Failed | S::Blocked | S::Done | S::Canceled
        ) | (S::Failed, S::InProgress | S::Blocked | S::Canceled)
            | (S::Blocked, S::InProgress | S::Failed | S::Canceled)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn new_state_starts_pending_with_timestamps() {
        let now = Utc.with_ymd_and_hms(2026, 3, 5, 12, 0, 0).unwrap();
        let state = DurableTaskState::new(now);
        assert_eq!(state.state, TaskExecutionState::Pending);
        assert_eq!(state.timestamps.pending_at, now);
        assert_eq!(state.timestamps.updated_at, now);
        assert!(state.timestamps.in_progress_at.is_none());
        assert!(state.timestamps.done_at.is_none());
    }

    #[test]
    fn transition_records_state_timestamps() {
        let t0 = Utc.with_ymd_and_hms(2026, 3, 5, 12, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 3, 5, 12, 1, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 3, 5, 12, 2, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 3, 5, 12, 3, 0).unwrap();

        let mut state = DurableTaskState::new(t0);
        state
            .transition(TaskExecutionState::InProgress, t1)
            .unwrap();
        state.transition(TaskExecutionState::Blocked, t2).unwrap();
        state.transition(TaskExecutionState::Canceled, t3).unwrap();

        assert_eq!(state.state, TaskExecutionState::Canceled);
        assert_eq!(state.timestamps.in_progress_at, Some(t1));
        assert_eq!(state.timestamps.blocked_at, Some(t2));
        assert_eq!(state.timestamps.canceled_at, Some(t3));
        assert_eq!(state.timestamps.updated_at, t3);
    }

    #[test]
    fn terminal_state_rejects_further_transition() {
        let t0 = Utc.with_ymd_and_hms(2026, 3, 5, 12, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 3, 5, 12, 1, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 3, 5, 12, 2, 0).unwrap();

        let mut state = DurableTaskState::new(t0);
        state
            .transition(TaskExecutionState::InProgress, t1)
            .unwrap();
        state.transition(TaskExecutionState::Done, t2).unwrap();

        let err = state
            .transition(
                TaskExecutionState::Failed,
                Utc.with_ymd_and_hms(2026, 3, 5, 12, 3, 0).unwrap(),
            )
            .unwrap_err();
        assert!(err.contains("terminal state"));
    }
}
