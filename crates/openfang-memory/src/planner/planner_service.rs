use chrono::{NaiveDate, Utc};
use openfang_types::error::{OpenFangError, OpenFangResult};
use openfang_types::planner::{
    PlannerAgentCatalogEntry, PlannerInboxItem, PlannerInboxStatus, PlannerProject,
    PlannerProjectStatus, PlannerTask, PlannerTodayPlan,
};

use super::planner_agents::{catalog as agent_catalog, recommend_for_task};
use super::planner_inference::{infer_candidate, infer_candidates};
use super::planner_store::PlannerStore;
use super::planner_today::build_today_plan;

pub(crate) fn clarify_inbox_item(
    store: &PlannerStore,
    inbox_item_id: &str,
) -> OpenFangResult<(PlannerInboxItem, Option<PlannerProject>, PlannerTask, Vec<PlannerTask>)> {
    let existing = store.get_inbox_item(inbox_item_id)?.ok_or_else(|| {
        OpenFangError::InvalidInput(format!("Planner inbox item not found: {inbox_item_id}"))
    })?;

    if existing.status == PlannerInboxStatus::Clarified {
        let mut tasks = store.list_tasks_for_inbox_item(&existing.id)?;
        if tasks.is_empty() {
            let fallback_task = existing
                .task_id
                .as_deref()
                .and_then(|id| store.get_task(id).ok().flatten())
                .ok_or_else(|| {
                    OpenFangError::Internal(
                        "Inbox item marked clarified but planner task is missing".to_string(),
                    )
                })?;
            tasks.push(fallback_task);
        }
        let tasks = enrich_tasks_with_recommendations(store, tasks)?;
        let task = tasks.first().cloned().ok_or_else(|| {
            OpenFangError::Internal("Planner clarify did not produce a task".to_string())
        })?;
        let project = match &existing.project_id {
            Some(id) => store.get_project(id)?,
            None => None,
        };
        return Ok((existing, project, task, tasks));
    }

    let now = Utc::now();
    let candidates = infer_candidates(&existing.text, now);
    let create_project = candidates.len() > 1 || candidates.iter().any(|candidate| candidate.kind.is_project());
    let root_candidate = infer_candidate(&existing.text, now);

    let project = if create_project {
        let project_outcome = candidates
            .iter()
            .map(|candidate| candidate.cleaned_text.as_str())
            .collect::<Vec<_>>()
            .join(" → ");
        let project = PlannerProject {
            id: uuid::Uuid::new_v4().to_string(),
            title: root_candidate
                .project_title
                .clone()
                .unwrap_or_else(|| root_candidate.task_title.clone()),
            outcome: project_outcome,
            status: PlannerProjectStatus::Active,
            created_at: now,
            updated_at: now,
        };
        store.save_project(&project)?;
        Some(project)
    } else {
        None
    };

    let mut tasks = Vec::new();
    for candidate in candidates {
        let task = PlannerTask {
            id: uuid::Uuid::new_v4().to_string(),
            title: candidate.task_title.clone(),
            status: candidate.task_status,
            priority: candidate.priority,
            effort_minutes: candidate.effort_minutes,
            energy: candidate.energy,
            project_id: project.as_ref().map(|p| p.id.clone()),
            due_at: candidate.due_at,
            scheduled_for: candidate.scheduled_for,
            blocked_by: candidate.blocked_by,
            next_action: candidate.next_action,
            source_inbox_item_id: Some(existing.id.clone()),
            agent_recommendation: None,
            created_at: now,
            updated_at: now,
        };
        store.create_task(&task)?;
        tasks.push(task);
    }

    let tasks = enrich_tasks_with_recommendations(store, tasks)?;
    let task = tasks.first().cloned().ok_or_else(|| {
        OpenFangError::Internal("Planner clarify did not produce any tasks".to_string())
    })?;

    let clarified = PlannerInboxItem {
        status: PlannerInboxStatus::Clarified,
        clarified_at: Some(now),
        task_id: Some(task.id.clone()),
        project_id: project.as_ref().map(|p| p.id.clone()),
        ..existing
    };
    store.update_inbox_item(&clarified)?;

    Ok((clarified, project, task, tasks))
}

pub(crate) fn rebuild_today_plan(
    store: &PlannerStore,
    date: NaiveDate,
) -> OpenFangResult<PlannerTodayPlan> {
    let open_tasks = enrich_tasks_with_recommendations(store, store.list_open_tasks()?)?;
    let plan = build_today_plan(date, open_tasks);
    store.upsert_today_plan(&plan)?;
    Ok(plan)
}

pub(crate) fn hydrate_today_plan_recommendations(
    store: &PlannerStore,
    mut plan: PlannerTodayPlan,
) -> OpenFangResult<PlannerTodayPlan> {
    plan.must_do = enrich_tasks_with_recommendations(store, plan.must_do)?;
    plan.should_do = enrich_tasks_with_recommendations(store, plan.should_do)?;
    plan.could_do = enrich_tasks_with_recommendations(store, plan.could_do)?;
    plan.focus_suggestion = match plan.focus_suggestion {
        Some(task) => enrich_tasks_with_recommendations(store, vec![task])?.into_iter().next(),
        None => None,
    };
    Ok(plan)
}

pub(crate) fn inbox_items_with_recommendations(
    store: &PlannerStore,
) -> OpenFangResult<Vec<(PlannerInboxItem, Vec<PlannerTask>)>> {
    let inbox = store.list_inbox()?;
    let mut result = Vec::with_capacity(inbox.len());
    for item in inbox {
        let tasks = if item.status == PlannerInboxStatus::Clarified {
            enrich_tasks_with_recommendations(store, store.list_tasks_for_inbox_item(&item.id)?)?
        } else {
            Vec::new()
        };
        result.push((item, tasks));
    }
    Ok(result)
}

pub(crate) fn list_agent_catalog(store: &PlannerStore) -> OpenFangResult<Vec<PlannerAgentCatalogEntry>> {
    let preferences = store.list_agent_preferences()?;
    Ok(agent_catalog(&preferences))
}

pub(crate) fn set_agent_catalog_enabled(
    store: &PlannerStore,
    agent_id: &str,
    enabled: bool,
) -> OpenFangResult<PlannerAgentCatalogEntry> {
    store.set_agent_enabled(agent_id, enabled)?;
    list_agent_catalog(store)?
        .into_iter()
        .find(|entry| entry.agent_id == agent_id)
        .ok_or_else(|| OpenFangError::InvalidInput(format!("Planner agent not found: {agent_id}")))
}

fn enrich_tasks_with_recommendations(
    store: &PlannerStore,
    tasks: Vec<PlannerTask>,
) -> OpenFangResult<Vec<PlannerTask>> {
    let preferences = store.list_agent_preferences()?;
    Ok(tasks
        .into_iter()
        .map(|mut task| {
            task.agent_recommendation = recommend_for_task(&task, &preferences);
            task
        })
        .collect())
}
