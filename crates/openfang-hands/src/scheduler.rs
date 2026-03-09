//! Hand scheduler integration.
//!
//! Converts a [`HandScheduleSpec`] declared in a `HAND.toml` manifest into a
//! [`CronJob`] and manages the lifecycle of those jobs independently of the
//! kernel's general-purpose `CronScheduler`.
//!
//! # Design
//!
//! The `HandScheduler` is intentionally **self-contained** — it does not hold
//! a reference to the kernel's `CronScheduler` directly.  Instead it produces
//! `CronJob` values that the kernel can insert into its own scheduler.  This
//! keeps `openfang-hands` free of a circular dependency on `openfang-kernel`.
//!
//! ```text
//! HandRegistry::activate()
//!     └─► HandScheduler::build_job()   → CronJob
//!             └─► kernel.cron.add_job(job)   (done by the kernel, not here)
//! ```

use crate::{HandDefinition, HandScheduleSpec};
use chrono::Utc;
use openfang_types::agent::AgentId;
use openfang_types::scheduler::{CronAction, CronDelivery, CronJob, CronJobId, CronSchedule};
use std::collections::HashMap;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// HandScheduler
// ---------------------------------------------------------------------------

/// Tracks which cron job IDs belong to which hand instances.
///
/// The kernel is responsible for actually inserting/removing jobs from its
/// `CronScheduler`; this struct just keeps the mapping so jobs can be
/// removed when a Hand is deactivated.
#[derive(Debug, Default)]
pub struct HandScheduler {
    /// `instance_id` → `CronJobId` for every scheduled hand instance.
    jobs: HashMap<Uuid, CronJobId>,
}

impl HandScheduler {
    /// Create a new, empty hand scheduler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a [`CronJob`] for a hand instance, if the manifest declares a
    /// default schedule.
    ///
    /// Returns `None` when the hand has no `default_schedule` field.
    ///
    /// The caller (the kernel) is responsible for inserting the returned job
    /// into its `CronScheduler`.
    pub fn build_job(
        &mut self,
        instance_id: Uuid,
        agent_id: AgentId,
        def: &HandDefinition,
    ) -> Option<CronJob> {
        let spec = def.default_schedule.as_ref()?;

        let schedule = match spec {
            HandScheduleSpec::Every { every_secs } => CronSchedule::Every {
                every_secs: *every_secs,
            },
            HandScheduleSpec::Cron { expr, tz } => CronSchedule::Cron {
                expr: expr.clone(),
                tz: tz.clone(),
            },
        };

        let job_id = CronJobId(Uuid::new_v4());
        let now = Utc::now();

        let job = CronJob {
            id: job_id.clone(),
            agent_id,
            name: format!("hand:{}", def.id),
            enabled: true,
            schedule,
            action: CronAction::AgentTurn {
                message: format!(
                    "You are the {} Hand. This is your scheduled run. \
                     Execute your primary task as defined in your system prompt.",
                    def.name
                ),
                model_override: None,
                timeout_secs: Some(300),
            },
            delivery: CronDelivery::None,
            created_at: now,
            last_run: None,
            next_run: None,
        };

        self.jobs.insert(instance_id, job_id);
        Some(job)
    }

    /// Remove the cron job mapping for a hand instance.
    ///
    /// Returns the `CronJobId` that was registered, if any, so the caller can
    /// remove it from the kernel's `CronScheduler`.
    pub fn remove_job(&mut self, instance_id: &Uuid) -> Option<CronJobId> {
        self.jobs.remove(instance_id)
    }

    /// Return the `CronJobId` for a hand instance, if one was registered.
    pub fn get_job_id(&self, instance_id: &Uuid) -> Option<&CronJobId> {
        self.jobs.get(instance_id)
    }

    /// Return all registered (instance_id, job_id) pairs.
    pub fn all_jobs(&self) -> impl Iterator<Item = (&Uuid, &CronJobId)> {
        self.jobs.iter()
    }

    /// Number of scheduled hand instances.
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// Whether there are no scheduled hand instances.
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        HandAgentConfig, HandCategory, HandDashboard, HandDefinition, HandScheduleSpec,
    };

    fn make_def(schedule: Option<HandScheduleSpec>) -> HandDefinition {
        HandDefinition {
            id: "test-hand".to_string(),
            name: "Test Hand".to_string(),
            description: "A hand for testing".to_string(),
            category: HandCategory::Productivity,
            icon: "🤖".to_string(),
            tools: vec!["shell_exec".to_string()],
            skills: vec![],
            mcp_servers: vec![],
            requires: vec![],
            settings: vec![],
            agent: HandAgentConfig {
                name: "test-agent".to_string(),
                description: "test".to_string(),
                module: "builtin:chat".to_string(),
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                api_key_env: None,
                base_url: None,
                max_tokens: 4096,
                temperature: 0.7,
                system_prompt: "You are a test hand.".to_string(),
                max_iterations: None,
            },
            dashboard: HandDashboard::default(),
            skill_content: None,
            default_schedule: schedule,
        }
    }

    fn make_agent_id() -> AgentId {
        AgentId(Uuid::new_v4())
    }

    #[test]
    fn no_schedule_returns_none() {
        let mut sched = HandScheduler::new();
        let instance_id = Uuid::new_v4();
        let def = make_def(None);
        let job = sched.build_job(instance_id, make_agent_id(), &def);
        assert!(job.is_none());
        assert!(sched.is_empty());
    }

    #[test]
    fn every_schedule_builds_job() {
        let mut sched = HandScheduler::new();
        let instance_id = Uuid::new_v4();
        let def = make_def(Some(HandScheduleSpec::Every { every_secs: 3600 }));
        let job = sched.build_job(instance_id, make_agent_id(), &def).unwrap();

        assert_eq!(job.name, "hand:test-hand");
        assert!(job.enabled);
        assert_eq!(sched.len(), 1);

        match &job.schedule {
            CronSchedule::Every { every_secs } => assert_eq!(*every_secs, 3600),
            other => panic!("Expected Every schedule, got {other:?}"),
        }
    }

    #[test]
    fn cron_schedule_builds_job() {
        let mut sched = HandScheduler::new();
        let instance_id = Uuid::new_v4();
        let def = make_def(Some(HandScheduleSpec::Cron {
            expr: "0 9 * * 1-5".to_string(),
            tz: Some("America/New_York".to_string()),
        }));
        let job = sched.build_job(instance_id, make_agent_id(), &def).unwrap();

        match &job.schedule {
            CronSchedule::Cron { expr, tz } => {
                assert_eq!(expr, "0 9 * * 1-5");
                assert_eq!(tz.as_deref(), Some("America/New_York"));
            }
            other => panic!("Expected Cron schedule, got {other:?}"),
        }
    }

    #[test]
    fn remove_job_returns_id() {
        let mut sched = HandScheduler::new();
        let instance_id = Uuid::new_v4();
        let def = make_def(Some(HandScheduleSpec::Every { every_secs: 300 }));
        let job = sched.build_job(instance_id, make_agent_id(), &def).unwrap();
        let job_id = job.id.clone();

        let removed = sched.remove_job(&instance_id).unwrap();
        assert_eq!(removed.0, job_id.0);
        assert!(sched.is_empty());
    }

    #[test]
    fn remove_nonexistent_returns_none() {
        let mut sched = HandScheduler::new();
        let result = sched.remove_job(&Uuid::new_v4());
        assert!(result.is_none());
    }

    #[test]
    fn multiple_instances_tracked_independently() {
        let mut sched = HandScheduler::new();
        let def = make_def(Some(HandScheduleSpec::Every { every_secs: 600 }));

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        sched.build_job(id1, make_agent_id(), &def);
        sched.build_job(id2, make_agent_id(), &def);

        assert_eq!(sched.len(), 2);
        assert!(sched.get_job_id(&id1).is_some());
        assert!(sched.get_job_id(&id2).is_some());

        sched.remove_job(&id1);
        assert_eq!(sched.len(), 1);
        assert!(sched.get_job_id(&id1).is_none());
        assert!(sched.get_job_id(&id2).is_some());
    }
}
