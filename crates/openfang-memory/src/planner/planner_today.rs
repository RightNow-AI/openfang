use chrono::{NaiveDate, Utc};
use openfang_types::planner::{PlannerTask, PlannerTaskStatus, PlannerTodayPlan, PriorityBand};

pub(crate) fn build_today_plan(date: NaiveDate, open_tasks: Vec<PlannerTask>) -> PlannerTodayPlan {
    let mut blocked = Vec::new();
    let mut eligible = Vec::new();

    for task in open_tasks {
        if task.status == PlannerTaskStatus::Blocked || !task.blocked_by.is_empty() {
            blocked.push(task);
        } else if today_eligible(&task, date) {
            eligible.push(task);
        }
    }

    eligible.sort_by(compare_tasks_for_today);

    let must_do: Vec<PlannerTask> = eligible
        .iter()
        .filter(|task| matches!(task.priority, PriorityBand::Urgent | PriorityBand::High))
        .take(3)
        .cloned()
        .collect();

    let mut remaining: Vec<PlannerTask> = eligible
        .into_iter()
        .filter(|task| !must_do.iter().any(|picked| picked.id == task.id))
        .collect();

    if must_do.is_empty() && !remaining.is_empty() {
        let first = remaining.remove(0);
        let mut must = vec![first];
        let should_do: Vec<PlannerTask> = remaining.iter().take(3).cloned().collect();
        let could_do: Vec<PlannerTask> = remaining.iter().skip(3).take(3).cloned().collect();
        let blockers = blocked.iter().map(task_blocker_summary).collect();

        return PlannerTodayPlan {
            date: date.format("%Y-%m-%d").to_string(),
            daily_outcome: format!("Move {} forward.", must[0].title),
            focus_suggestion: Some(must[0].clone()),
            rebuilt_at: Some(Utc::now()),
            must_do: std::mem::take(&mut must),
            should_do,
            could_do,
            blockers,
        };
    }

    let should_do: Vec<PlannerTask> = remaining.iter().take(3).cloned().collect();
    let could_do: Vec<PlannerTask> = remaining.iter().skip(3).take(3).cloned().collect();
    let blockers = blocked.iter().map(task_blocker_summary).collect();
    let focus_suggestion = must_do.first().cloned().or_else(|| should_do.first().cloned());
    let daily_outcome = focus_suggestion
        .as_ref()
        .map(|task| format!("Move {} forward.", task.title))
        .unwrap_or_else(|| "Clarify the inbox and protect focus.".to_string());

    PlannerTodayPlan {
        date: date.format("%Y-%m-%d").to_string(),
        daily_outcome,
        must_do,
        should_do,
        could_do,
        blockers,
        focus_suggestion,
        rebuilt_at: Some(Utc::now()),
    }
}

pub(crate) fn today_eligible(task: &PlannerTask, date: NaiveDate) -> bool {
    task.scheduled_for
        .map(|scheduled| scheduled.date_naive() <= date)
        .unwrap_or(true)
}

fn compare_tasks_for_today(a: &PlannerTask, b: &PlannerTask) -> std::cmp::Ordering {
    priority_rank(&b.priority)
        .cmp(&priority_rank(&a.priority))
        .then_with(|| a.due_at.cmp(&b.due_at))
        .then_with(|| a.scheduled_for.cmp(&b.scheduled_for))
        .then_with(|| a.created_at.cmp(&b.created_at))
}

fn priority_rank(priority: &PriorityBand) -> u8 {
    match priority {
        PriorityBand::Urgent => 4,
        PriorityBand::High => 3,
        PriorityBand::Medium => 2,
        PriorityBand::Low => 1,
    }
}

fn task_blocker_summary(task: &PlannerTask) -> String {
    if task.blocked_by.is_empty() {
        format!("{} is blocked", task.title)
    } else {
        format!("{} — {}", task.title, task.blocked_by.join(", "))
    }
}
