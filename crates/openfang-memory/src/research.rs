//! Research store — persistence layer for experiments, validated patterns,
//! and the control plane configuration.
//!
//! All three entities are stored in SQLite tables created by the v16 migration.
//! JSON-encoded blobs are used for variable-length fields (selection_trace,
//! example_work_item_ids, validated_patterns_applied).

use chrono::{DateTime, Utc};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::research::{
    ControlPlaneConfig, ExperimentScore, ExperimentStatus, PromotionStatus, ResearchExperiment,
    SelectionTrace, ValidatedPattern,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// SQLite-backed store for research experiments, validated patterns, and
/// the active control plane configuration.
#[derive(Clone)]
pub struct ResearchStore {
    conn: Arc<Mutex<Connection>>,
}

impl ResearchStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    // -----------------------------------------------------------------------
    // Control plane config
    // -----------------------------------------------------------------------

    /// Persist (upsert) the active control plane config.
    ///
    /// Only one row is kept; id is always `"active"`.
    pub fn upsert_control_plane(&self, cfg: &ControlPlaneConfig) -> OpenFangResult<()> {
        let json = serde_json::to_string(cfg)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO research_control_plane (id, config_json, updated_at)
             VALUES ('active', ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET config_json = excluded.config_json,
                                           updated_at  = excluded.updated_at",
            rusqlite::params![json, Utc::now().to_rfc3339()],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Load the active control plane config.  Returns `Default` if none stored.
    pub fn load_control_plane(&self) -> OpenFangResult<ControlPlaneConfig> {
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let result: Option<String> = conn
            .query_row(
                "SELECT config_json FROM research_control_plane WHERE id = 'active'",
                [],
                |row| row.get(0),
            )
            .ok();
        match result {
            Some(json) => serde_json::from_str(&json).map_err(|e| OpenFangError::Internal(e.to_string())),
            None => Ok(ControlPlaneConfig::default()),
        }
    }

    // -----------------------------------------------------------------------
    // Experiments — write
    // -----------------------------------------------------------------------

    /// Persist a new experiment.
    pub fn create_experiment(&self, exp: &ResearchExperiment) -> OpenFangResult<ResearchExperiment> {
        let trace_json = serde_json::to_string(&exp.selection_trace)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let patterns_json = serde_json::to_string(&exp.validated_patterns_applied)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let score_json = exp
            .score
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO research_experiments (
                id, work_item_id, hypothesis, planner_id, executor_id, reviewer_id,
                status, score_json, promotion_status, validated_patterns_applied_json,
                result_summary, selection_trace_json,
                started_at, finished_at, created_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            rusqlite::params![
                exp.id,
                exp.work_item_id,
                exp.hypothesis,
                exp.planner_id,
                exp.executor_id,
                exp.reviewer_id,
                exp.status.as_str(),
                score_json,
                exp.promotion_status.as_ref().map(|p| p.as_str()),
                patterns_json,
                exp.result_summary,
                trace_json,
                exp.started_at.to_rfc3339(),
                exp.finished_at.map(|t| t.to_rfc3339()),
                exp.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        drop(conn);
        self.get_experiment(&exp.id)?.ok_or_else(|| OpenFangError::Internal("experiment vanished after insert".into()))
    }

    /// Transition an experiment's status.
    pub fn update_experiment_status(&self, id: &str, status: ExperimentStatus) -> OpenFangResult<()> {
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "UPDATE research_experiments SET status = ?1 WHERE id = ?2",
            rusqlite::params![status.as_str(), id],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Record the Reviewer's scorecard and promotion decision.
    pub fn record_score(
        &self,
        id: &str,
        score: &ExperimentScore,
        promotion: &PromotionStatus,
        summary: Option<&str>,
    ) -> OpenFangResult<()> {
        let score_json = serde_json::to_string(score)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "UPDATE research_experiments
             SET score_json = ?1, promotion_status = ?2, result_summary = ?3,
                 status = ?4, finished_at = ?5
             WHERE id = ?6",
            rusqlite::params![
                score_json,
                promotion.as_str(),
                summary,
                ExperimentStatus::Reviewed.as_str(),
                Utc::now().to_rfc3339(),
                id,
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Append a `SelectionTrace` entry to an existing experiment.
    pub fn append_trace(&self, id: &str, entry: &SelectionTrace) -> OpenFangResult<()> {
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let existing_json: String = conn
            .query_row(
                "SELECT selection_trace_json FROM research_experiments WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut trace: Vec<SelectionTrace> = serde_json::from_str(&existing_json)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        trace.push(entry.clone());
        let new_json = serde_json::to_string(&trace)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;

        conn.execute(
            "UPDATE research_experiments SET selection_trace_json = ?1 WHERE id = ?2",
            rusqlite::params![new_json, id],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Experiments — read
    // -----------------------------------------------------------------------

    /// Fetch an experiment by ID.
    pub fn get_experiment(&self, id: &str) -> OpenFangResult<Option<ResearchExperiment>> {
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let result = conn.query_row(
            "SELECT id, work_item_id, hypothesis, planner_id, executor_id, reviewer_id,
                    status, score_json, promotion_status, validated_patterns_applied_json,
                    result_summary, selection_trace_json, started_at, finished_at, created_at
             FROM research_experiments WHERE id = ?1",
            rusqlite::params![id],
            row_to_experiment,
        );
        match result {
            Ok(exp) => Ok(Some(exp)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    /// List experiments, optionally filtered by status (snake_case strings).
    pub fn list_experiments(&self, status_filter: Option<&str>) -> OpenFangResult<Vec<ResearchExperiment>> {
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let (sql, params_vec): (String, Vec<String>) = if let Some(s) = status_filter {
            (
                "SELECT id, work_item_id, hypothesis, planner_id, executor_id, reviewer_id,
                        status, score_json, promotion_status, validated_patterns_applied_json,
                        result_summary, selection_trace_json, started_at, finished_at, created_at
                 FROM research_experiments WHERE status = ?1 ORDER BY created_at DESC LIMIT 100"
                    .into(),
                vec![s.to_string()],
            )
        } else {
            (
                "SELECT id, work_item_id, hypothesis, planner_id, executor_id, reviewer_id,
                        status, score_json, promotion_status, validated_patterns_applied_json,
                        result_summary, selection_trace_json, started_at, finished_at, created_at
                 FROM research_experiments ORDER BY created_at DESC LIMIT 100"
                    .into(),
                vec![],
            )
        };

        let mut stmt = conn.prepare(&sql).map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let rows = if params_vec.is_empty() {
            stmt.query_map([], row_to_experiment)
        } else {
            stmt.query_map(rusqlite::params![params_vec[0]], row_to_experiment)
        }
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;

        let mut results = vec![];
        for r in rows {
            results.push(r.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(results)
    }

    // -----------------------------------------------------------------------
    // Validated patterns — write
    // -----------------------------------------------------------------------

    /// Persist a new validated pattern.
    pub fn create_pattern(&self, pat: &ValidatedPattern) -> OpenFangResult<ValidatedPattern> {
        let examples_json = serde_json::to_string(&pat.example_work_item_ids)
            .map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT INTO research_validated_patterns
                (id, description, pattern_type, example_work_item_ids_json,
                 times_applied, success_rate, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![
                pat.id,
                pat.description,
                pat.pattern_type,
                examples_json,
                pat.times_applied as i64,
                pat.success_rate as f64,
                pat.created_at.to_rfc3339(),
                pat.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        drop(conn);
        self.get_pattern(&pat.id)?.ok_or_else(|| OpenFangError::Internal("pattern vanished after insert".into()))
    }

    /// Record a usage of a pattern and update its success rate.
    pub fn record_pattern_usage(&self, id: &str, succeeded: bool) -> OpenFangResult<()> {
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        // Read current stats
        let (times, rate): (i64, f64) = conn
            .query_row(
                "SELECT times_applied, success_rate FROM research_validated_patterns WHERE id = ?1",
                rusqlite::params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let new_times = times + 1;
        let new_rate = (rate * times as f64 + if succeeded { 1.0 } else { 0.0 }) / new_times as f64;
        conn.execute(
            "UPDATE research_validated_patterns
             SET times_applied = ?1, success_rate = ?2, updated_at = ?3
             WHERE id = ?4",
            rusqlite::params![new_times, new_rate, Utc::now().to_rfc3339(), id],
        )
        .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Validated patterns — read
    // -----------------------------------------------------------------------

    /// Fetch a validated pattern by ID.
    pub fn get_pattern(&self, id: &str) -> OpenFangResult<Option<ValidatedPattern>> {
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let result = conn.query_row(
            "SELECT id, description, pattern_type, example_work_item_ids_json,
                    times_applied, success_rate, created_at, updated_at
             FROM research_validated_patterns WHERE id = ?1",
            rusqlite::params![id],
            row_to_pattern,
        );
        match result {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(OpenFangError::Memory(e.to_string())),
        }
    }

    /// List all validated patterns, most-applied first.
    pub fn list_patterns(&self) -> OpenFangResult<Vec<ValidatedPattern>> {
        let conn = self.conn.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, description, pattern_type, example_work_item_ids_json,
                        times_applied, success_rate, created_at, updated_at
                 FROM research_validated_patterns ORDER BY times_applied DESC LIMIT 200",
            )
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let rows = stmt
            .query_map([], row_to_pattern)
            .map_err(|e| OpenFangError::Memory(e.to_string()))?;
        let mut results = vec![];
        for r in rows {
            results.push(r.map_err(|e| OpenFangError::Memory(e.to_string()))?);
        }
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// Row mappers
// ---------------------------------------------------------------------------

fn parse_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.as_deref().and_then(|v| v.parse::<DateTime<Utc>>().ok())
}

fn row_to_experiment(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResearchExperiment> {
    let status_str: String = row.get(6)?;
    let score_json: Option<String> = row.get(7)?;
    let promo_str: Option<String> = row.get(8)?;
    let patterns_json: String = row.get(9)?;
    let trace_json: String = row.get(11)?;
    let started_str: String = row.get(12)?;
    let finished_str: Option<String> = row.get(13)?;
    let created_str: String = row.get(14)?;

    Ok(ResearchExperiment {
        id: row.get(0)?,
        work_item_id: row.get(1)?,
        hypothesis: row.get(2)?,
        planner_id: row.get(3)?,
        executor_id: row.get(4)?,
        reviewer_id: row.get(5)?,
        status: parse_experiment_status(&status_str),
        score: score_json
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok()),
        promotion_status: promo_str.as_deref().map(parse_promotion_status),
        validated_patterns_applied: serde_json::from_str(&patterns_json).unwrap_or_default(),
        result_summary: row.get(10)?,
        selection_trace: serde_json::from_str(&trace_json).unwrap_or_default(),
        started_at: started_str.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
        finished_at: parse_dt(finished_str),
        created_at: created_str.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
    })
}

fn row_to_pattern(row: &rusqlite::Row<'_>) -> rusqlite::Result<ValidatedPattern> {
    let examples_json: String = row.get(3)?;
    let created_str: String = row.get(6)?;
    let updated_str: String = row.get(7)?;
    Ok(ValidatedPattern {
        id: row.get(0)?,
        description: row.get(1)?,
        pattern_type: row.get(2)?,
        example_work_item_ids: serde_json::from_str(&examples_json).unwrap_or_default(),
        times_applied: row.get::<_, i64>(4)? as u32,
        success_rate: row.get::<_, f64>(5)? as f32,
        created_at: created_str.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
        updated_at: updated_str.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now()),
    })
}

fn parse_experiment_status(s: &str) -> ExperimentStatus {
    match s {
        "planned"         => ExperimentStatus::Planned,
        "running"         => ExperimentStatus::Running,
        "awaiting_review" => ExperimentStatus::AwaitingReview,
        "reviewed"        => ExperimentStatus::Reviewed,
        "aborted"         => ExperimentStatus::Aborted,
        _                 => ExperimentStatus::Planned,
    }
}

fn parse_promotion_status(s: &str) -> PromotionStatus {
    match s {
        "promoted"     => PromotionStatus::Promoted,
        "accepted"     => PromotionStatus::Accepted,
        "needs_review" => PromotionStatus::NeedsReview,
        _              => PromotionStatus::Rejected,
    }
}

// ---------------------------------------------------------------------------
// as_str helpers (avoid orphan impl by keeping them local)
// ---------------------------------------------------------------------------

trait AsStr {
    fn as_str(&self) -> &'static str;
}

impl AsStr for ExperimentStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Planned        => "planned",
            Self::Running        => "running",
            Self::AwaitingReview => "awaiting_review",
            Self::Reviewed       => "reviewed",
            Self::Aborted        => "aborted",
        }
    }
}

impl AsStr for PromotionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Promoted    => "promoted",
            Self::Accepted    => "accepted",
            Self::NeedsReview => "needs_review",
            Self::Rejected    => "rejected",
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::research::{ResearchExperiment, ValidatedPattern};
    use rusqlite::Connection;

    fn test_store() -> ResearchStore {
        let conn = Connection::open_in_memory().unwrap();
        // Create tables directly (no full migration system in tests)
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS research_control_plane (
                id TEXT PRIMARY KEY,
                config_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS research_experiments (
                id TEXT PRIMARY KEY,
                work_item_id TEXT,
                hypothesis TEXT NOT NULL,
                planner_id TEXT NOT NULL,
                executor_id TEXT,
                reviewer_id TEXT,
                status TEXT NOT NULL DEFAULT 'planned',
                score_json TEXT,
                promotion_status TEXT,
                validated_patterns_applied_json TEXT NOT NULL DEFAULT '[]',
                result_summary TEXT,
                selection_trace_json TEXT NOT NULL DEFAULT '[]',
                started_at TEXT NOT NULL,
                finished_at TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS research_validated_patterns (
                id TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                pattern_type TEXT NOT NULL,
                example_work_item_ids_json TEXT NOT NULL DEFAULT '[]',
                times_applied INTEGER NOT NULL DEFAULT 0,
                success_rate REAL NOT NULL DEFAULT 0.0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )
        .unwrap();
        ResearchStore::new(Arc::new(Mutex::new(conn)))
    }

    #[test]
    fn create_and_get_experiment() {
        let store = test_store();
        let exp = ResearchExperiment::new("e1".into(), "test hypothesis".into(), "agent-1".into());
        store.create_experiment(&exp).unwrap();
        let fetched = store.get_experiment("e1").unwrap().unwrap();
        assert_eq!(fetched.id, "e1");
        assert_eq!(fetched.hypothesis, "test hypothesis");
    }

    #[test]
    fn update_experiment_status() {
        let store = test_store();
        let exp = ResearchExperiment::new("e2".into(), "h".into(), "a".into());
        store.create_experiment(&exp).unwrap();
        store.update_experiment_status("e2", ExperimentStatus::Running).unwrap();
        let fetched = store.get_experiment("e2").unwrap().unwrap();
        assert_eq!(fetched.status, ExperimentStatus::Running);
    }

    #[test]
    fn record_score_transitions_to_reviewed() {
        use openfang_types::research::ScoreWeights;
        let store = test_store();
        let exp = ResearchExperiment::new("e3".into(), "h".into(), "a".into());
        store.create_experiment(&exp).unwrap();
        let weights = ScoreWeights::default();
        let score = ExperimentScore::compute(0.9, 0.85, 0.8, 1.0, &weights, None);
        store.record_score("e3", &score, &PromotionStatus::Promoted, Some("great")).unwrap();
        let fetched = store.get_experiment("e3").unwrap().unwrap();
        assert_eq!(fetched.status, ExperimentStatus::Reviewed);
        assert_eq!(fetched.promotion_status, Some(PromotionStatus::Promoted));
        assert!(fetched.score.is_some());
    }

    #[test]
    fn list_experiments_filter_by_status() {
        let store = test_store();
        store.create_experiment(&ResearchExperiment::new("e4".into(), "h".into(), "a".into())).unwrap();
        store.create_experiment(&ResearchExperiment::new("e5".into(), "h2".into(), "a".into())).unwrap();
        store.update_experiment_status("e4", ExperimentStatus::Running).unwrap();
        let running = store.list_experiments(Some("running")).unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, "e4");
    }

    #[test]
    fn create_and_list_patterns() {
        let store = test_store();
        let pat = ValidatedPattern::new("p1".into(), "use api adapter".into(), "adapter_selection".into());
        store.create_pattern(&pat).unwrap();
        let list = store.list_patterns().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "p1");
    }

    #[test]
    fn record_pattern_usage_updates_rate() {
        let store = test_store();
        let pat = ValidatedPattern::new("p2".into(), "desc".into(), "t".into());
        store.create_pattern(&pat).unwrap();
        store.record_pattern_usage("p2", true).unwrap();
        store.record_pattern_usage("p2", false).unwrap();
        let fetched = store.get_pattern("p2").unwrap().unwrap();
        assert_eq!(fetched.times_applied, 2);
        assert!((fetched.success_rate - 0.5).abs() < 1e-4);
    }

    #[test]
    fn control_plane_defaults_when_not_stored() {
        let store = test_store();
        let cfg = store.load_control_plane().unwrap();
        assert_eq!(cfg.label, "default");
    }

    #[test]
    fn upsert_control_plane_persists() {
        let store = test_store();
        let mut cfg = ControlPlaneConfig::default();
        cfg.label = "custom".into();
        store.upsert_control_plane(&cfg).unwrap();
        let loaded = store.load_control_plane().unwrap();
        assert_eq!(loaded.label, "custom");
    }

    #[test]
    fn upsert_control_plane_replaces_previous() {
        let store = test_store();
        let mut cfg1 = ControlPlaneConfig::default();
        cfg1.label = "v1".into();
        store.upsert_control_plane(&cfg1).unwrap();
        let mut cfg2 = ControlPlaneConfig::default();
        cfg2.label = "v2".into();
        store.upsert_control_plane(&cfg2).unwrap();
        let loaded = store.load_control_plane().unwrap();
        assert_eq!(loaded.label, "v2");
    }
}
