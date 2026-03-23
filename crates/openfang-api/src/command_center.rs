//! Command Center — in-memory store and HTTP handlers.
//!
//! Routes:
//!   POST   /clients
//!   GET    /clients/:id
//!   PUT    /clients/:id
//!   POST   /wizard/generate-plan
//!   GET    /tasks            ?client_id=…
//!   PATCH  /tasks/:id
//!   POST   /tasks/:id/approve
//!   POST   /tasks/:id/reject
//!   POST   /tasks/:id/request-changes
//!   POST   /tasks/:id/run
//!   GET    /approvals        ?client_id=…
//!   GET    /results          ?client_id=…

use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

// ────────────────────────────────────────────────────────────────────────────
// State
// ────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct CommandCenterStore {
    pub clients:   HashMap<String, ClientProfile>,
    pub tasks:     HashMap<String, PlannedTask>,
    pub approvals: HashMap<String, ApprovalItem>,
    pub results:   HashMap<String, RunResult>,
}

pub type CommandCenterState = Arc<RwLock<CommandCenterStore>>;

// ────────────────────────────────────────────────────────────────────────────
// Router
// ────────────────────────────────────────────────────────────────────────────

pub fn router(state: CommandCenterState) -> Router {
    Router::new()
        .route("/clients",                   post(create_client))
        .route("/clients/{id}",               get(get_client).put(update_client))
        .route("/wizard/generate-plan",      post(generate_plan))
        .route("/tasks",                     get(list_tasks))
        .route("/tasks/{id}",                 patch(update_task))
        .route("/tasks/{id}/approve",         post(approve_task))
        .route("/tasks/{id}/reject",          post(reject_task))
        .route("/tasks/{id}/request-changes", post(request_changes_task))
        .route("/tasks/{id}/run",             post(run_task))
        .route("/approvals",                 get(list_approvals))
        .route("/results",                   get(list_results))
        .with_state(state)
}

// ────────────────────────────────────────────────────────────────────────────
// Data models
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    None,
    #[default]
    Required,
    Conditional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approver {
    pub name:  String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientProfile {
    pub id:                           String,
    pub client_name:                  String,
    pub business_name:                String,
    pub industry:                     String,
    pub main_goal:                    String,
    pub website_url:                  String,
    pub offer:                        String,
    pub customer:                     String,
    pub notes:                        String,
    pub approval_mode:                ApprovalMode,
    pub approvers:                    Vec<Approver>,
    pub require_approval_for_email:   bool,
    pub require_approval_for_tool_use: bool,
    pub require_approval_for_assignment: bool,
    pub created_at:                   String,
    pub updated_at:                   String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateClientRequest {
    pub client_name:                  Option<String>,
    pub business_name:                Option<String>,
    pub industry:                     Option<String>,
    pub main_goal:                    Option<String>,
    pub website_url:                  Option<String>,
    pub offer:                        Option<String>,
    pub customer:                     Option<String>,
    pub notes:                        Option<String>,
    pub approval_mode:                Option<ApprovalMode>,
    pub approvers:                    Option<Vec<Approver>>,
    pub require_approval_for_email:   Option<bool>,
    pub require_approval_for_tool_use: Option<bool>,
    pub require_approval_for_assignment: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateClientRequest {
    pub patch: CreateClientRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    SummarizeBusiness,
    ResearchCompetitors,
    DraftOutreachEmails,
    AssignFollowupChores,
    PrepareWeeklyTaskPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedTask {
    pub id:               String,
    pub client_id:        String,
    pub title:            String,
    pub r#type:           TaskType,
    pub status:           String,
    pub board_column:     String,
    pub priority:         String,
    pub assigned_agent:   String,
    pub required_tools:   Vec<String>,
    pub approval_required: bool,
    pub approval_status:  String,
    pub input_snapshot:   serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalItem {
    pub id:              String,
    pub task_id:         String,
    pub client_id:       String,
    pub requested_by:    String,
    pub status:          String,
    pub preview_summary: String,
    pub tool_actions:    Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub id:               String,
    pub task_id:          String,
    pub client_id:        String,
    pub status:           String,
    pub output_type:      String,
    pub title:            String,
    pub content_markdown: String,
    pub started_at:       String,
    pub completed_at:     String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratePlanRequest {
    pub client_id:            String,
    pub selected_task_types:  Vec<TaskType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub status:         Option<String>,
    pub board_column:   Option<String>,
    pub assigned_agent: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClientIdQuery {
    pub client_id: String,
}

// ────────────────────────────────────────────────────────────────────────────
// Handlers
// ────────────────────────────────────────────────────────────────────────────

type JsonResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalDecision {
    Approve,
    Reject,
    RequestChanges,
}

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

fn sync_approval_items(store: &mut CommandCenterStore, task: &PlannedTask) {
    for approval in store.approvals.values_mut() {
        if approval.task_id == task.id {
            approval.status = task.approval_status.clone();
        }
    }
}

fn apply_assignment(task: &mut PlannedTask, agent: String) {
    task.assigned_agent = agent;
}

fn apply_board_move(task: &mut PlannedTask, column: &str) -> Result<(), String> {
    match column {
        "backlog" => {
            task.board_column = "backlog".into();
            task.status = if task.approval_required && task.approval_status == "approved" {
                "approved".into()
            } else {
                "draft".into()
            };
        }
        "this_week" => {
            task.board_column = "this_week".into();
            task.status = if task.approval_required && task.approval_status == "approved" {
                "approved".into()
            } else {
                "draft".into()
            };
        }
        "waiting" => {
            if !task.approval_required {
                return Err("Only approval-gated tasks can move to waiting".into());
            }
            task.board_column = "waiting".into();
            task.status = "pending_approval".into();
            task.approval_status = "pending".into();
        }
        "today" => {
            if task.approval_required && task.approval_status != "approved" {
                return Err("Task needs approval before moving to today".into());
            }
            task.board_column = "today".into();
            task.status = if task.status == "running" {
                "running".into()
            } else {
                "approved".into()
            };
        }
        "done" => {
            if task.approval_required && task.approval_status != "approved" {
                return Err("Task needs approval before moving to done".into());
            }
            task.board_column = "done".into();
            task.status = "completed".into();
        }
        _ => return Err("Unknown board column".into()),
    }

    Ok(())
}

fn apply_approval_decision(task: &mut PlannedTask, decision: ApprovalDecision) -> Result<(), String> {
    if !task.approval_required {
        return Err("Task does not require approval".into());
    }

    match decision {
        ApprovalDecision::Approve => {
            task.approval_status = "approved".into();
            task.board_column = "today".into();
            task.status = "approved".into();
        }
        ApprovalDecision::Reject => {
            task.approval_status = "rejected".into();
            task.board_column = "backlog".into();
            task.status = "draft".into();
        }
        ApprovalDecision::RequestChanges => {
            task.approval_status = "changes_requested".into();
            task.board_column = "backlog".into();
            task.status = "draft".into();
        }
    }

    Ok(())
}

fn apply_run_started(task: &mut PlannedTask) -> Result<(), String> {
    if task.approval_required && task.approval_status != "approved" {
        return Err("Task needs approval before run".into());
    }

    if task.status == "completed" {
        return Err("Completed tasks cannot be run again".into());
    }

    if task.status != "approved" && task.status != "running" {
        return Err("Only approved tasks can be run".into());
    }

    task.status = "running".into();
    task.board_column = "today".into();

    Ok(())
}

fn apply_run_completed(task: &mut PlannedTask) {
    task.status = "completed".into();
    task.board_column = "done".into();
}

async fn create_client(
    State(state): State<CommandCenterState>,
    Json(req): Json<CreateClientRequest>,
) -> JsonResult {
    let now = Utc::now().to_rfc3339();
    let id  = format!("cl_{}", Uuid::new_v4().simple());

    let client = ClientProfile {
        id:                           id.clone(),
        client_name:                  req.client_name.unwrap_or_default(),
        business_name:                req.business_name.unwrap_or_default(),
        industry:                     req.industry.unwrap_or_default(),
        main_goal:                    req.main_goal.unwrap_or_default(),
        website_url:                  req.website_url.unwrap_or_default(),
        offer:                        req.offer.unwrap_or_default(),
        customer:                     req.customer.unwrap_or_default(),
        notes:                        req.notes.unwrap_or_default(),
        approval_mode:                req.approval_mode.unwrap_or_default(),
        approvers:                    req.approvers.unwrap_or_default(),
        require_approval_for_email:   req.require_approval_for_email.unwrap_or(true),
        require_approval_for_tool_use: req.require_approval_for_tool_use.unwrap_or(true),
        require_approval_for_assignment: req.require_approval_for_assignment.unwrap_or(true),
        created_at:                   now.clone(),
        updated_at:                   now,
    };

    state.write().await.clients.insert(id, client.clone());
    Ok(Json(serde_json::json!({ "client": client })))
}

async fn get_client(
    State(state): State<CommandCenterState>,
    Path(id): Path<String>,
) -> JsonResult {
    let store = state.read().await;
    let client = store.clients.get(&id).cloned()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Client not found"))?;
    Ok(Json(serde_json::json!({ "client": client })))
}

async fn update_client(
    State(state): State<CommandCenterState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateClientRequest>,
) -> JsonResult {
    let mut store = state.write().await;
    let client = store.clients.get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Client not found"))?;

    let p = req.patch;
    if let Some(v) = p.client_name                  { client.client_name = v; }
    if let Some(v) = p.business_name                { client.business_name = v; }
    if let Some(v) = p.industry                     { client.industry = v; }
    if let Some(v) = p.main_goal                    { client.main_goal = v; }
    if let Some(v) = p.website_url                  { client.website_url = v; }
    if let Some(v) = p.offer                        { client.offer = v; }
    if let Some(v) = p.customer                     { client.customer = v; }
    if let Some(v) = p.notes                        { client.notes = v; }
    if let Some(v) = p.approval_mode                { client.approval_mode = v; }
    if let Some(v) = p.approvers                    { client.approvers = v; }
    if let Some(v) = p.require_approval_for_email   { client.require_approval_for_email = v; }
    if let Some(v) = p.require_approval_for_tool_use { client.require_approval_for_tool_use = v; }
    if let Some(v) = p.require_approval_for_assignment { client.require_approval_for_assignment = v; }

    client.updated_at = Utc::now().to_rfc3339();
    Ok(Json(serde_json::json!({ "client": client.clone() })))
}

async fn generate_plan(
    State(state): State<CommandCenterState>,
    Json(req): Json<GeneratePlanRequest>,
) -> JsonResult {
    let mut store = state.write().await;
    let client = store.clients.get(&req.client_id).cloned()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Client not found"))?;

    let mut created = Vec::new();

    for task_type in req.selected_task_types {
        let id = format!("tsk_{}", Uuid::new_v4().simple());

        let (title, agent, tools, default_needs_approval): (&str, &str, Vec<String>, bool) =
            match task_type {
                TaskType::SummarizeBusiness => (
                    "Summarize business",
                    "Business Context Agent",
                    vec!["website_summarizer".into()],
                    false,
                ),
                TaskType::ResearchCompetitors => (
                    "Research competitors",
                    "Research Agent",
                    vec!["web_search".into(), "scraper".into()],
                    false,
                ),
                TaskType::DraftOutreachEmails => (
                    "Draft outreach emails",
                    "Email Agent",
                    vec!["mcp_email".into(), "copy_generator".into()],
                    true,
                ),
                TaskType::AssignFollowupChores => (
                    "Assign follow-up chores",
                    "Ops Agent",
                    vec!["task_logger".into()],
                    true,
                ),
                TaskType::PrepareWeeklyTaskPlan => (
                    "Prepare weekly task plan",
                    "Task Planner Agent",
                    vec!["task_planner".into()],
                    false,
                ),
            };

        let approval_required = match client.approval_mode {
            ApprovalMode::None        => false,
            ApprovalMode::Required    => true,
            ApprovalMode::Conditional => default_needs_approval,
        };

        let task = PlannedTask {
            id: id.clone(),
            client_id: client.id.clone(),
            title: title.to_string(),
            r#type: task_type,
            status: if approval_required { "pending_approval".into() } else { "draft".into() },
            board_column: if approval_required { "waiting".into() } else { "backlog".into() },
            priority: "high".into(),
            assigned_agent: agent.into(),
            required_tools: tools.clone(),
            approval_required,
            approval_status: if approval_required { "pending".into() } else { "none".into() },
            input_snapshot: serde_json::json!({
                "business_name": client.business_name,
                "offer":         client.offer,
                "customer":      client.customer,
            }),
        };

        if approval_required {
            let approval = ApprovalItem {
                id:              format!("apr_{}", Uuid::new_v4().simple()),
                task_id:         id.clone(),
                client_id:       client.id.clone(),
                requested_by:    "Task Planner Agent".into(),
                status:          "pending".into(),
                preview_summary: format!("{title} is ready for approval"),
                tool_actions:    tools,
            };
            store.approvals.insert(approval.id.clone(), approval);
        }

        store.tasks.insert(id.clone(), task.clone());
        created.push(task);
    }

    Ok(Json(serde_json::json!({ "tasks": created })))
}

async fn list_tasks(
    State(state): State<CommandCenterState>,
    Query(q): Query<ClientIdQuery>,
) -> Json<serde_json::Value> {
    let store = state.read().await;
    let tasks: Vec<&PlannedTask> = store.tasks.values()
        .filter(|t| t.client_id == q.client_id)
        .collect();
    Json(serde_json::json!({ "tasks": tasks }))
}

async fn approve_task(
    State(state): State<CommandCenterState>,
    Path(id): Path<String>,
) -> JsonResult {
    let mut store = state.write().await;
    let task = store.tasks.get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Task not found"))?;

    apply_approval_decision(task, ApprovalDecision::Approve)
        .map_err(|msg| err(StatusCode::CONFLICT, &msg))?;
    let task = task.clone();
    sync_approval_items(&mut store, &task);

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn reject_task(
    State(state): State<CommandCenterState>,
    Path(id): Path<String>,
) -> JsonResult {
    let mut store = state.write().await;
    let task = store.tasks.get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Task not found"))?;

    apply_approval_decision(task, ApprovalDecision::Reject)
        .map_err(|msg| err(StatusCode::CONFLICT, &msg))?;
    let task = task.clone();
    sync_approval_items(&mut store, &task);

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn request_changes_task(
    State(state): State<CommandCenterState>,
    Path(id): Path<String>,
) -> JsonResult {
    let mut store = state.write().await;
    let task = store.tasks.get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Task not found"))?;

    apply_approval_decision(task, ApprovalDecision::RequestChanges)
        .map_err(|msg| err(StatusCode::CONFLICT, &msg))?;
    let task = task.clone();
    sync_approval_items(&mut store, &task);

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn update_task(
    State(state): State<CommandCenterState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateTaskRequest>,
) -> JsonResult {
    let mut store = state.write().await;
    let task = store.tasks.get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Task not found"))?;

    if let Some(agent) = req.assigned_agent {
        apply_assignment(task, agent);
    }

    if let Some(column) = req.board_column {
        apply_board_move(task, &column)
            .map_err(|msg| err(StatusCode::CONFLICT, &msg))?;
    }

    if let Some(status) = req.status {
        return Err(err(
            StatusCode::BAD_REQUEST,
            &format!("Direct status updates are not allowed: {status}"),
        ));
    }

    let task = task.clone();
    sync_approval_items(&mut store, &task);

    Ok(Json(serde_json::json!({ "task": task })))
}

async fn run_task(
    State(state): State<CommandCenterState>,
    Path(id): Path<String>,
) -> JsonResult {
    let mut store = state.write().await;

    let task = store.tasks.get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Task not found"))?;
    apply_run_started(task)
        .map_err(|msg| err(StatusCode::CONFLICT, &msg))?;

    let (task_title, task_agent, task_client, task_id) = (
        task.title.clone(),
        task.assigned_agent.clone(),
        task.client_id.clone(),
        task.id.clone(),
    );

    let now = Utc::now().to_rfc3339();
    let result = RunResult {
        id:               format!("run_{}", Uuid::new_v4().simple()),
        task_id,
        client_id:        task_client,
        status:           "completed".into(),
        output_type:      "markdown".into(),
        title:            format!("Result for {task_title}"),
        content_markdown: format!(
            "# {task_title}\n\nGenerated by {task_agent}\n\nThis is stub output. Wire up a real agent here."
        ),
        started_at:       now.clone(),
        completed_at:     now,
    };

    let task = store.tasks.get_mut(&id).unwrap();
    apply_run_completed(task);
    store.results.insert(result.id.clone(), result.clone());

    Ok(Json(serde_json::json!({ "result": result })))
}

async fn list_approvals(
    State(state): State<CommandCenterState>,
    Query(q): Query<ClientIdQuery>,
) -> Json<serde_json::Value> {
    let store = state.read().await;
    let approvals: Vec<&ApprovalItem> = store.approvals.values()
        .filter(|a| a.client_id == q.client_id)
        .collect();
    Json(serde_json::json!({ "approvals": approvals }))
}

async fn list_results(
    State(state): State<CommandCenterState>,
    Query(q): Query<ClientIdQuery>,
) -> Json<serde_json::Value> {
    let store = state.read().await;
    let results: Vec<&RunResult> = store.results.values()
        .filter(|r| r.client_id == q.client_id)
        .collect();
    Json(serde_json::json!({ "results": results }))
}
