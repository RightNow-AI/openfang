use chrono::{DateTime, Datelike, Days, Duration, Utc, Weekday};
use openfang_types::planner::{EnergyLevel, PlannerTaskStatus, PriorityBand};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlannerItemKind {
	Task,
	Project,
}

impl PlannerItemKind {
	pub(crate) fn is_project(self) -> bool {
		matches!(self, Self::Project)
	}
}

#[derive(Debug, Clone)]
pub(crate) struct PlannerCandidate {
	pub cleaned_text: String,
	pub kind: PlannerItemKind,
	pub project_title: Option<String>,
	pub task_title: String,
	pub task_status: PlannerTaskStatus,
	pub priority: PriorityBand,
	pub effort_minutes: Option<u32>,
	pub energy: EnergyLevel,
	pub due_at: Option<DateTime<Utc>>,
	pub scheduled_for: Option<DateTime<Utc>>,
	pub blocked_by: Vec<String>,
	pub next_action: String,
}

pub(crate) fn infer_candidates(text: &str, now: DateTime<Utc>) -> Vec<PlannerCandidate> {
	let cleaned = normalize_capture_text(text);
	let segments = split_capture_segments(&cleaned);
	if segments.is_empty() {
		return vec![infer_candidate(text, now)];
	}

	segments
		.into_iter()
		.map(|segment| infer_candidate(&segment, now))
		.collect()
}

pub(crate) fn infer_candidate(text: &str, now: DateTime<Utc>) -> PlannerCandidate {
	let cleaned_text = normalize_capture_text(text);
	let kind = if looks_like_project(&cleaned_text) {
		PlannerItemKind::Project
	} else {
		PlannerItemKind::Task
	};

	let task_title = task_title_from(&cleaned_text);
	PlannerCandidate {
		project_title: kind
			.is_project()
			.then(|| project_title_from(&cleaned_text)),
		task_status: task_status_from(&cleaned_text),
		priority: priority_from(&cleaned_text),
		effort_minutes: Some(estimate_effort_minutes(&cleaned_text)),
		energy: energy_from(&cleaned_text),
		due_at: infer_due_at(&cleaned_text, now),
		scheduled_for: infer_scheduled_for(&cleaned_text, now),
		blocked_by: infer_blockers(&cleaned_text),
		next_action: next_action_from(&cleaned_text, &task_title),
		cleaned_text,
		kind,
		task_title,
	}
}

pub(crate) fn normalize_capture_text(input: &str) -> String {
	let trimmed = input.trim().trim_end_matches('.').trim();
	let lowered = trimmed.to_lowercase();
	let stripped = if lowered.starts_with("need to ") {
		&trimmed[8..]
	} else if lowered.starts_with("i need to ") {
		&trimmed[10..]
	} else if lowered.starts_with("todo: ") {
		&trimmed[6..]
	} else {
		trimmed
	};
	sentence_case(stripped.trim())
}

fn split_capture_segments(text: &str) -> Vec<String> {
	let mut normalized = text.replace(" and then ", "|then|");
	normalized = normalized.replace(" then ", "|then|");
	normalized = normalized.replace("; then ", "|then|");
	normalized = normalized.replace("\nthen ", "|then|");
	normalized
		.split("|then|")
		.map(str::trim)
		.filter(|segment| !segment.is_empty())
		.map(sentence_case)
		.collect()
}

pub(crate) fn looks_like_project(text: &str) -> bool {
	let lower = text.to_lowercase();
	lower.contains(" and ")
		|| lower.contains("this week")
		|| lower.contains("project")
		|| lower.contains("ship ")
		|| lower.contains("launch ")
		|| lower.ends_with(" system")
		|| lower.contains(" review flow")
}

fn project_title_from(text: &str) -> String {
	let cleaned = text
		.replace(" this week", "")
		.replace(" this month", "")
		.trim()
		.to_string();
	sentence_case(&cleaned)
}

fn task_title_from(text: &str) -> String {
	if let Some((first, _)) = text.split_once(" and ") {
		sentence_case(first)
	} else {
		sentence_case(text)
	}
}

fn next_action_from(text: &str, fallback_title: &str) -> String {
	if let Some((first, _)) = text.split_once(" and ") {
		sentence_case(first)
	} else if text.is_empty() {
		fallback_title.to_string()
	} else {
		sentence_case(text)
	}
}

fn estimate_effort_minutes(text: &str) -> u32 {
	let lower = text.to_lowercase();
	if lower.contains("quick") || lower.contains("small") {
		15
	} else if lower.contains("this week")
		|| lower.contains("refactor")
		|| lower.contains("design")
		|| lower.ends_with(" system")
	{
		90
	} else if lower.contains("define") || lower.contains("wire") || lower.contains("build") {
		60
	} else {
		30
	}
}

fn energy_from(text: &str) -> EnergyLevel {
	let lower = text.to_lowercase();
	if lower.contains("review") || lower.contains("reply") || lower.contains("cleanup") {
		EnergyLevel::Low
	} else if lower.contains("define")
		|| lower.contains("design")
		|| lower.contains("build")
		|| lower.contains("wire")
		|| lower.contains("refactor")
	{
		EnergyLevel::High
	} else {
		EnergyLevel::Medium
	}
}

pub(crate) fn priority_from(text: &str) -> PriorityBand {
	let lower = text.to_lowercase();
	if lower.contains("this week") || lower.contains("deadline") {
		PriorityBand::High
	} else if contains_token(&lower, "urgent")
		|| contains_token(&lower, "asap")
		|| contains_token(&lower, "today")
	{
		PriorityBand::Urgent
	} else if lower.contains("someday") || lower.contains("maybe") {
		PriorityBand::Low
	} else {
		PriorityBand::Medium
	}
}

fn task_status_from(text: &str) -> PlannerTaskStatus {
	let lower = text.to_lowercase();
	if lower.contains("blocked") || lower.contains("waiting on") {
		PlannerTaskStatus::Blocked
	} else {
		PlannerTaskStatus::Todo
	}
}

fn infer_blockers(text: &str) -> Vec<String> {
	let lower = text.to_lowercase();
	if let Some(idx) = lower.find("blocked by ") {
		return vec![sentence_case(&text[idx + 11..])];
	}
	if lower.contains("waiting on") {
		return vec!["Waiting on an external dependency".to_string()];
	}
	Vec::new()
}

pub(crate) fn infer_due_at(text: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
	let lower = text.to_lowercase();
	if lower.contains("this week") {
		let days_until_friday = days_until(now.weekday(), Weekday::Fri);
		day_at_hour(now, days_until_friday, 17)
	} else if contains_token(&lower, "tomorrow") {
		day_at_hour(now, 1, 17)
	} else if contains_token(&lower, "today") {
		day_at_hour(now, 0, 17)
	} else {
		None
	}
}

pub(crate) fn infer_scheduled_for(text: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
	let lower = text.to_lowercase();
	if lower.contains("this week") || contains_token(&lower, "today") {
		Some(now)
	} else if contains_token(&lower, "tomorrow") {
		Some(now + Duration::days(1))
	} else {
		None
	}
}

fn contains_token(text: &str, token: &str) -> bool {
	text.split(|c: char| !c.is_ascii_alphanumeric())
		.any(|part| part == token)
}

fn days_until(current: Weekday, target: Weekday) -> u32 {
	let current = current.num_days_from_monday();
	let target = target.num_days_from_monday();
	if target >= current {
		target - current
	} else {
		7 - (current - target)
	}
}

fn day_at_hour(now: DateTime<Utc>, day_offset: u32, hour: u32) -> Option<DateTime<Utc>> {
	now.date_naive()
		.checked_add_days(Days::new(day_offset.into()))
		.and_then(|date| date.and_hms_opt(hour, 0, 0))
		.map(|dt| dt.and_utc())
}

fn sentence_case(input: &str) -> String {
	let trimmed = input.trim();
	if trimmed.is_empty() {
		return String::new();
	}
	let mut chars = trimmed.chars();
	let first = chars.next().unwrap().to_uppercase().collect::<String>();
	format!("{}{}", first, chars.as_str())
}
