//! Hand scheduler — bridges `HandScheduleSpec` to the kernel's cron engine.

use std::collections::HashMap;
use uuid::Uuid;

/// A schedule specification parsed from a `HAND.toml` manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandScheduleSpec {
    /// A standard 5-field cron expression (minute hour dom month dow).
    Cron(String),
    /// A fixed interval in seconds.
    Interval { seconds: u64 },
    /// Run once on activation, then stop.
    Once,
}

/// Error type for scheduler operations.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Invalid cron expression '{expr}': {reason}")]
    InvalidCron { expr: String, reason: String },
    #[error("Invalid interval: {0}")]
    InvalidInterval(String),
    #[error("Job not found: {0}")]
    JobNotFound(Uuid),
}

/// A registered scheduler job.
#[derive(Debug, Clone)]
pub struct ScheduledJob {
    pub job_id: Uuid,
    pub hand_id: String,
    pub spec: HandScheduleSpec,
    pub next_run_description: String,
}

/// Bridges `HandScheduleSpec` to the kernel's cron engine.
#[derive(Debug, Default)]
pub struct HandScheduler {
    jobs: std::sync::RwLock<HashMap<Uuid, ScheduledJob>>,
}

impl HandScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn validate_spec(&self, spec: &HandScheduleSpec) -> Result<(), SchedulerError> {
        match spec {
            HandScheduleSpec::Cron(expr) => self.validate_cron(expr),
            HandScheduleSpec::Interval { seconds } => {
                if *seconds == 0 {
                    Err(SchedulerError::InvalidInterval(
                        "Interval must be greater than 0 seconds".to_string(),
                    ))
                } else {
                    Ok(())
                }
            }
            HandScheduleSpec::Once => Ok(()),
        }
    }

    pub fn register(
        &self,
        instance_id: Uuid,
        hand_id: &str,
        spec: HandScheduleSpec,
    ) -> Result<ScheduledJob, SchedulerError> {
        self.validate_spec(&spec)?;
        let next_run_description = match &spec {
            HandScheduleSpec::Cron(expr) => format!("cron: {expr}"),
            HandScheduleSpec::Interval { seconds } => format!("every {}s", seconds),
            HandScheduleSpec::Once => "once (on activation)".to_string(),
        };
        let job = ScheduledJob {
            job_id: instance_id,
            hand_id: hand_id.to_string(),
            spec,
            next_run_description,
        };
        self.jobs
            .write()
            .expect("scheduler jobs lock")
            .insert(instance_id, job.clone());
        Ok(job)
    }

    pub fn cancel(&self, instance_id: Uuid) -> Result<ScheduledJob, SchedulerError> {
        self.jobs
            .write()
            .expect("scheduler jobs lock")
            .remove(&instance_id)
            .ok_or(SchedulerError::JobNotFound(instance_id))
    }

    pub fn list_jobs(&self) -> Vec<ScheduledJob> {
        self.jobs
            .read()
            .expect("scheduler jobs lock")
            .values()
            .cloned()
            .collect()
    }

    pub fn job_count(&self) -> usize {
        self.jobs.read().expect("scheduler jobs lock").len()
    }

    fn validate_cron(&self, expr: &str) -> Result<(), SchedulerError> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(SchedulerError::InvalidCron {
                expr: expr.to_string(),
                reason: format!(
                    "expected 5 fields (minute hour dom month dow), got {}",
                    fields.len()
                ),
            });
        }
        let limits = [(0u32, 59u32), (0, 23), (1, 31), (1, 12), (0, 7)];
        let names = ["minute", "hour", "day-of-month", "month", "day-of-week"];
        for (i, (field, (min, max))) in fields.iter().zip(limits.iter()).enumerate() {
            self.validate_cron_field(field, *min, *max, names[i], expr)?;
        }
        Ok(())
    }

    fn validate_cron_field(
        &self,
        field: &str,
        min: u32,
        max: u32,
        name: &str,
        expr: &str,
    ) -> Result<(), SchedulerError> {
        if field == "*" {
            return Ok(());
        }
        if let Some(step_str) = field.strip_prefix("*/") {
            let step: u32 = step_str.parse().map_err(|_| SchedulerError::InvalidCron {
                expr: expr.to_string(),
                reason: format!("{name} step '{step_str}' is not a valid integer"),
            })?;
            if step == 0 {
                return Err(SchedulerError::InvalidCron {
                    expr: expr.to_string(),
                    reason: format!("{name} step must be > 0"),
                });
            }
            return Ok(());
        }
        if field.contains('-') {
            let parts: Vec<&str> = field.splitn(2, '-').collect();
            let a: u32 = parts[0].parse().map_err(|_| SchedulerError::InvalidCron {
                expr: expr.to_string(),
                reason: format!("{name} range start '{}' is not a valid integer", parts[0]),
            })?;
            let b: u32 = parts[1].parse().map_err(|_| SchedulerError::InvalidCron {
                expr: expr.to_string(),
                reason: format!("{name} range end '{}' is not a valid integer", parts[1]),
            })?;
            if a > b || a < min || b > max {
                return Err(SchedulerError::InvalidCron {
                    expr: expr.to_string(),
                    reason: format!("{name} range {a}-{b} is out of bounds ({min}-{max})"),
                });
            }
            return Ok(());
        }
        for part in field.split(',') {
            let v: u32 = part.parse().map_err(|_| SchedulerError::InvalidCron {
                expr: expr.to_string(),
                reason: format!("{name} value '{part}' is not a valid integer"),
            })?;
            if v < min || v > max {
                return Err(SchedulerError::InvalidCron {
                    expr: expr.to_string(),
                    reason: format!("{name} value {v} is out of bounds ({min}-{max})"),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_cron() {
        let s = HandScheduler::new();
        assert!(s.validate_spec(&HandScheduleSpec::Cron("0 */6 * * *".to_string())).is_ok());
        assert!(s.validate_spec(&HandScheduleSpec::Cron("30 9 * * 1-5".to_string())).is_ok());
        assert!(s.validate_spec(&HandScheduleSpec::Cron("*/15 * * * *".to_string())).is_ok());
    }

    #[test]
    fn test_validate_invalid_cron() {
        let s = HandScheduler::new();
        assert!(s.validate_spec(&HandScheduleSpec::Cron("not-a-cron".to_string())).is_err());
        assert!(s.validate_spec(&HandScheduleSpec::Cron("0 25 * * *".to_string())).is_err());
        assert!(s.validate_spec(&HandScheduleSpec::Cron("0 * *".to_string())).is_err());
    }

    #[test]
    fn test_validate_interval() {
        let s = HandScheduler::new();
        assert!(s.validate_spec(&HandScheduleSpec::Interval { seconds: 3600 }).is_ok());
        assert!(s.validate_spec(&HandScheduleSpec::Interval { seconds: 0 }).is_err());
    }

    #[test]
    fn test_register_and_cancel_job() {
        let s = HandScheduler::new();
        let id = Uuid::new_v4();
        let job = s
            .register(id, "researcher", HandScheduleSpec::Interval { seconds: 3600 })
            .expect("register");
        assert_eq!(job.job_id, id);
        assert_eq!(s.job_count(), 1);
        s.cancel(id).expect("cancel");
        assert_eq!(s.job_count(), 0);
    }

    #[test]
    fn test_cancel_nonexistent_job_returns_error() {
        let s = HandScheduler::new();
        let result = s.cancel(Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_list_jobs() {
        let s = HandScheduler::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        s.register(id1, "clip", HandScheduleSpec::Once).expect("register clip");
        s.register(id2, "lead", HandScheduleSpec::Cron("0 9 * * 1-5".to_string()))
            .expect("register lead");
        let jobs = s.list_jobs();
        assert_eq!(jobs.len(), 2);
    }
}
