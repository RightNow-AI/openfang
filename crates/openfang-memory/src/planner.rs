//! Planner module boundaries for the Personal Chief of Staff slice.

mod planner_agents;
mod planner_inference;
mod planner_service;
mod planner_store;
mod planner_today;

pub use planner_store::PlannerStore;
pub(crate) use planner_service::{
    clarify_inbox_item, hydrate_today_plan_recommendations, inbox_items_with_recommendations,
    list_agent_catalog, rebuild_today_plan, set_agent_catalog_enabled,
};

#[cfg(test)]
mod tests {
    use super::planner_inference::{infer_candidate, PlannerItemKind};
    use super::planner_service::{
        clarify_inbox_item, hydrate_today_plan_recommendations, list_agent_catalog,
        rebuild_today_plan, set_agent_catalog_enabled,
    };
    use super::planner_today::today_eligible;
    use super::PlannerStore;
    use crate::migration::run_migrations;
    use chrono::{DateTime, Utc};
    use openfang_types::planner::{
        PlannerAgentRecommendation, PlannerInboxStatus, PlannerRecommendationConfidence,
        PlannerTask, PlannerTaskStatus, PlannerTodayPlan, PriorityBand,
    };
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn make_store() -> PlannerStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        PlannerStore::new(Arc::new(Mutex::new(conn)))
    }

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-03-09T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn candidate_task(text: &str) -> PlannerTask {
        let candidate = infer_candidate(text, fixed_now());
        PlannerTask {
            id: "task-1".to_string(),
            title: candidate.task_title,
            status: candidate.task_status,
            priority: candidate.priority,
            effort_minutes: candidate.effort_minutes,
            energy: candidate.energy,
            project_id: None,
            due_at: candidate.due_at,
            scheduled_for: candidate.scheduled_for,
            blocked_by: candidate.blocked_by,
            next_action: candidate.next_action,
            source_inbox_item_id: None,
            agent_recommendation: None,
            created_at: fixed_now(),
            updated_at: fixed_now(),
        }
    }

    #[test]
    fn inference_handles_phrase_collisions_and_time_language() {
        struct Case {
            text: &'static str,
            kind: PlannerItemKind,
            priority: PriorityBand,
            due_date: Option<&'static str>,
            scheduled_date: Option<&'static str>,
            blocked: bool,
            eligible_today: bool,
        }

        let today = fixed_now().date_naive();
        let cases = [
            Case {
                text: "Today page this week",
                kind: PlannerItemKind::Project,
                priority: PriorityBand::High,
                due_date: Some("2026-03-13"),
                scheduled_date: Some("2026-03-09"),
                blocked: false,
                eligible_today: true,
            },
            Case {
                text: "Finish Today page today",
                kind: PlannerItemKind::Task,
                priority: PriorityBand::Urgent,
                due_date: Some("2026-03-09"),
                scheduled_date: Some("2026-03-09"),
                blocked: false,
                eligible_today: true,
            },
            Case {
                text: "Ship planner this week",
                kind: PlannerItemKind::Project,
                priority: PriorityBand::High,
                due_date: Some("2026-03-13"),
                scheduled_date: Some("2026-03-09"),
                blocked: false,
                eligible_today: true,
            },
            Case {
                text: "Review planner tomorrow",
                kind: PlannerItemKind::Task,
                priority: PriorityBand::Medium,
                due_date: Some("2026-03-10"),
                scheduled_date: Some("2026-03-10"),
                blocked: false,
                eligible_today: false,
            },
            Case {
                text: "Need to define schema",
                kind: PlannerItemKind::Task,
                priority: PriorityBand::Medium,
                due_date: None,
                scheduled_date: None,
                blocked: false,
                eligible_today: true,
            },
            Case {
                text: "Build planner system",
                kind: PlannerItemKind::Project,
                priority: PriorityBand::Medium,
                due_date: None,
                scheduled_date: None,
                blocked: false,
                eligible_today: true,
            },
            Case {
                text: "Fix broken focus timer today",
                kind: PlannerItemKind::Task,
                priority: PriorityBand::Urgent,
                due_date: Some("2026-03-09"),
                scheduled_date: Some("2026-03-09"),
                blocked: false,
                eligible_today: true,
            },
            Case {
                text: "Plan weekly review flow",
                kind: PlannerItemKind::Project,
                priority: PriorityBand::Medium,
                due_date: None,
                scheduled_date: None,
                blocked: false,
                eligible_today: true,
            },
        ];

        for case in cases {
            let candidate = infer_candidate(case.text, fixed_now());
            assert_eq!(candidate.kind, case.kind, "kind mismatch for {}", case.text);
            assert_eq!(candidate.priority, case.priority, "priority mismatch for {}", case.text);
            assert!(!candidate.next_action.trim().is_empty(), "next_action missing for {}", case.text);
            assert_eq!(
                candidate.blocked_by.is_empty(),
                !case.blocked,
                "blocker mismatch for {}",
                case.text
            );
            assert_eq!(
                candidate.due_at.map(|dt| dt.date_naive().to_string()).as_deref(),
                case.due_date,
                "due_at mismatch for {}",
                case.text
            );
            assert_eq!(
                candidate
                    .scheduled_for
                    .map(|dt| dt.date_naive().to_string())
                    .as_deref(),
                case.scheduled_date,
                "scheduled_for mismatch for {}",
                case.text
            );

            let task = candidate_task(case.text);
            assert_eq!(
                today_eligible(&task, today),
                case.eligible_today,
                "today eligibility mismatch for {}",
                case.text
            );
        }
    }

    #[test]
    fn clarify_creates_project_and_task_for_weekly_project_input() {
        let store = make_store();
        let item = store
            .create_inbox_item("Need to define planner schema and wire Today page this week.")
            .unwrap();

        let (clarified, project, task, tasks) = clarify_inbox_item(&store, &item.id).unwrap();
        assert_eq!(clarified.status, PlannerInboxStatus::Clarified);
        assert!(project.is_some());
        assert_eq!(task.next_action, "Define planner schema");
        assert_eq!(task.priority, PriorityBand::High);
        assert_eq!(tasks.len(), 1);
        assert!(task.agent_recommendation.is_none());
    }

    #[test]
    fn clarify_recommends_specialists_for_split_task_input() {
        let store = make_store();
        let item = store
            .create_inbox_item("Need a security review of auth flow and then write launch notes")
            .unwrap();

        let (_clarified, project, _task, tasks) = clarify_inbox_item(&store, &item.id).unwrap();
        assert!(project.is_some());
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].agent_recommendation.as_ref().map(|r| r.agent_id.as_str()), Some("security-auditor"));
        assert_eq!(tasks[1].agent_recommendation.as_ref().map(|r| r.agent_id.as_str()), Some("writer"));
    }

    #[test]
    fn agent_catalog_persists_enabled_state() {
        let store = make_store();
        let catalog = list_agent_catalog(&store).unwrap();
        assert!(catalog.iter().any(|entry| entry.agent_id == "writer" && entry.enabled));

        let updated = set_agent_catalog_enabled(&store, "writer", false).unwrap();
        assert!(!updated.enabled);

        let catalog = list_agent_catalog(&store).unwrap();
        assert!(catalog.iter().any(|entry| entry.agent_id == "writer" && !entry.enabled));
    }

    #[test]
    fn locked_recommendation_fixtures_hold() {
        struct Case {
            input: &'static str,
            expected: &'static [Option<&'static str>],
        }

        let cases = [
            Case {
                input: "Need a security review of auth flow and then write launch notes",
                expected: &[Some("security-auditor"), Some("writer")],
            },
            Case {
                input: "Translate onboarding email into Spanish",
                expected: &[Some("translator")],
            },
            Case {
                input: "Need to think through project scope",
                expected: &[None],
            },
        ];

        for case in cases {
            let store = make_store();
            let item = store.create_inbox_item(case.input).unwrap();
            let (_clarified, _project, _task, tasks) = clarify_inbox_item(&store, &item.id).unwrap();
            let actual = tasks
                .iter()
                .map(|task| task.agent_recommendation.as_ref().map(|rec| rec.agent_id.as_str()))
                .collect::<Vec<_>>();
            assert_eq!(actual, case.expected, "fixture failed for {}", case.input);
        }
    }

    #[test]
    fn saved_today_plan_drops_legacy_assistant_recommendations_on_rehydrate() {
        let store = make_store();
        let item = store
            .create_inbox_item("Need a security review of auth flow")
            .unwrap();
        let (_clarified, _project, task, _tasks) = clarify_inbox_item(&store, &item.id).unwrap();

        let mut persisted_task = task.clone();
        persisted_task.agent_recommendation = Some(PlannerAgentRecommendation {
            agent_id: "assistant".to_string(),
            name: "Assistant".to_string(),
            reason: "Legacy fallback".to_string(),
            confidence: PlannerRecommendationConfidence::Low,
        });

        let plan = PlannerTodayPlan {
            date: "2026-03-10".to_string(),
            daily_outcome: "Ship auth update".to_string(),
            must_do: vec![persisted_task],
            should_do: vec![],
            could_do: vec![],
            blockers: vec![],
            focus_suggestion: None,
            rebuilt_at: Some(fixed_now()),
        };

        store.upsert_today_plan(&plan).unwrap();
        let persisted = store
            .get_today_plan(chrono::NaiveDate::from_ymd_opt(2026, 3, 10).unwrap())
            .unwrap()
            .unwrap();
        let reloaded = hydrate_today_plan_recommendations(&store, persisted).unwrap();
        assert_eq!(
            reloaded.must_do[0]
                .agent_recommendation
                .as_ref()
                .map(|rec| rec.agent_id.as_str()),
            Some("security-auditor")
        );
    }

    #[test]
    fn rebuild_today_caps_must_do_and_excludes_blocked_work() {
        let store = make_store();
        for text in [
            "Need to define planner schema today",
            "Need to wire Today page today",
            "Need to build planner API today",
            "Need to review CSS today",
            "Blocked by API contract",
        ] {
            let item = store.create_inbox_item(text).unwrap();
            let _ = clarify_inbox_item(&store, &item.id).unwrap();
        }

        let plan = rebuild_today_plan(&store, Utc::now().date_naive()).unwrap();
        assert!(plan.must_do.len() <= 3);
        assert!(plan.must_do.iter().all(|task| task.status != PlannerTaskStatus::Blocked));
        assert!(!plan.blockers.is_empty());
    }
}
