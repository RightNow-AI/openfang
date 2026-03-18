//! Business Modes — Agency, Growth, School
//!
//! Shared additive architecture on top of the Command Center engine.
//! One store powers three modes. Entry pages, task catalogs, and approval
//! presets differ per mode. The engine does not change.
//!
//! Routes (all under /modes/:mode/…)
//!   POST   /modes/:mode/records
//!   GET    /modes/:mode/records
//!   GET    /modes/:mode/records/:id
//!   PUT    /modes/:mode/records/:id
//!   POST   /modes/:mode/generate-plan
//!   GET    /modes/:mode/tasks            ?record_id=…
//!   POST   /modes/:mode/tasks/:id/approve
//!   POST   /modes/:mode/tasks/:id/run
//!   GET    /modes/:mode/approvals        ?record_id=…
//!   GET    /modes/:mode/results          ?record_id=…

use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
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
pub struct ModesStore {
    pub records:   HashMap<String, ModeRecord>,
    pub tasks:     HashMap<String, ModeTask>,
    pub approvals: HashMap<String, ModeApproval>,
    pub results:   HashMap<String, ModeResult>,
}

pub type ModesState = Arc<RwLock<ModesStore>>;

// ────────────────────────────────────────────────────────────────────────────
// Router
// ────────────────────────────────────────────────────────────────────────────

pub fn router(state: ModesState) -> Router {
    Router::new()
        .route("/modes/{mode}/records",                   post(create_record).get(list_records))
        .route("/modes/{mode}/records/{id}",               get(get_record).put(update_record))
        .route("/modes/{mode}/generate-plan",             post(generate_plan))
        .route("/modes/{mode}/tasks",                     get(list_tasks))
        .route("/modes/{mode}/tasks/{id}/approve",         post(approve_task))
        .route("/modes/{mode}/tasks/{id}/run",             post(run_task))
        .route("/modes/{mode}/approvals",                 get(list_approvals))
        .route("/modes/{mode}/results",                   get(list_results))
        .with_state(state)
}

// ────────────────────────────────────────────────────────────────────────────
// Data models
// ────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BusinessMode {
    Agency,
    Growth,
    School,
}

impl BusinessMode {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "agency" => Some(Self::Agency),
            "growth" => Some(Self::Growth),
            "school" => Some(Self::School),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeRecord {
    pub id:         String,
    pub mode:       BusinessMode,
    pub title:      String,
    pub subtitle:   String,
    pub goal:       String,
    pub status:     String,   // "active" | "draft" | "archived"
    pub meta:       serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRecordRequest {
    pub title:    Option<String>,
    pub subtitle: Option<String>,
    pub goal:     Option<String>,
    pub status:   Option<String>,
    pub meta:     Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRecordRequest {
    pub patch: CreateRecordRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeTask {
    pub id:               String,
    pub record_id:        String,
    pub mode:             BusinessMode,
    pub catalog_id:       String,
    pub title:            String,
    pub assigned_agent:   String,
    pub required_tools:   Vec<String>,
    pub approval_required: bool,
    pub approval_type:    Option<String>,
    pub status:           String,
    pub approval_status:  String,
    pub priority:         String,
    pub depends_on:       Vec<String>,
    pub output_summary:   String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeApproval {
    pub id:              String,
    pub task_id:         String,
    pub record_id:       String,
    pub mode:            BusinessMode,
    pub approval_type:   String,
    pub requested_by:    String,
    pub status:          String,
    pub preview_summary: String,
    pub tool_actions:    Vec<String>,
    pub created_at:      String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeResult {
    pub id:               String,
    pub task_id:          String,
    pub record_id:        String,
    pub mode:             BusinessMode,
    pub title:            String,
    pub output_type:      String,
    pub content_markdown: String,
    pub what_worked:      String,
    pub what_failed:      String,
    pub next_action:      String,
    pub owner:            String,
    pub status:           String,
    pub started_at:       String,
    pub completed_at:     String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneratePlanRequest {
    pub record_id:          String,
    pub selected_task_ids:  Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct RecordIdQuery {
    pub record_id: String,
}

// ────────────────────────────────────────────────────────────────────────────
// Mode-specific task catalogs
// ────────────────────────────────────────────────────────────────────────────

struct CatalogEntry {
    id:              &'static str,
    title:           &'static str,
    agent:           &'static str,
    tools:           Vec<&'static str>,
    approval_type:   Option<&'static str>,
    needs_approval:  bool,
}

fn catalog_for(mode: &BusinessMode) -> Vec<CatalogEntry> {
    match mode {
        BusinessMode::Agency => vec![
            CatalogEntry { id: "intake_client_brief",   title: "Create Client Brief",         agent: "Intake Agent",           tools: vec![],                                    approval_type: None,                              needs_approval: false },
            CatalogEntry { id: "scope_service",          title: "Scope Service Request",        agent: "Scope Agent",             tools: vec![],                                    approval_type: None,                              needs_approval: false },
            CatalogEntry { id: "summarize_business",     title: "Summarize Business Context",   agent: "Business Context Agent",  tools: vec!["website_summarizer"],                approval_type: None,                              needs_approval: false },
            CatalogEntry { id: "research_competitors",   title: "Research Competitors",         agent: "Research Agent",          tools: vec!["web_search", "scraper"],             approval_type: None,                              needs_approval: false },
            CatalogEntry { id: "build_brand_voice",      title: "Build Brand Voice Guide",      agent: "Brand Voice Agent",       tools: vec![],                                    approval_type: None,                              needs_approval: false },
            CatalogEntry { id: "build_delivery_plan",    title: "Build Delivery Plan",          agent: "Task Planner Agent",      tools: vec!["task_planner"],                      approval_type: None,                              needs_approval: false },
            CatalogEntry { id: "assign_tasks",           title: "Assign Tasks to Agents",       agent: "Assignment Agent",        tools: vec![],                                    approval_type: None,                              needs_approval: false },
            CatalogEntry { id: "draft_client_copy",      title: "Draft Client-Facing Content",  agent: "Writer Agent",            tools: vec!["copy_generator"],                    approval_type: Some("draft_approval"),            needs_approval: true  },
            CatalogEntry { id: "send_client_email",      title: "Send Client Email",            agent: "Email Agent",             tools: vec!["mcp_email"],                         approval_type: Some("send_approval"),             needs_approval: true  },
            CatalogEntry { id: "package_delivery",       title: "Package Delivery",             agent: "Approval Agent",          tools: vec![],                                    approval_type: Some("client_delivery_approval"),  needs_approval: true  },
            CatalogEntry { id: "capture_followups",      title: "Capture Follow-up Tasks",      agent: "Account Manager Agent",   tools: vec!["task_logger"],                       approval_type: None,                              needs_approval: false },
            CatalogEntry { id: "identify_upsells",       title: "Identify Upsell Opportunities",agent: "Upsell Agent",            tools: vec![],                                    approval_type: None,                              needs_approval: false },
        ],
        BusinessMode::Growth => vec![
            CatalogEntry { id: "write_campaign_brief",         title: "Write Campaign Brief",           agent: "Growth Strategist",         tools: vec![],                                    approval_type: None,                      needs_approval: false },
            CatalogEntry { id: "sharpen_offer",                title: "Sharpen Offer",                  agent: "Offer Agent",                tools: vec![],                                    approval_type: None,                      needs_approval: false },
            CatalogEntry { id: "research_competitor_ads",      title: "Research Competitor Ads",        agent: "Competitor Research Agent",  tools: vec!["web_search", "ad_library"],          approval_type: None,                      needs_approval: false },
            CatalogEntry { id: "build_creative_intelligence",  title: "Build Creative Intelligence",    agent: "Ad Intelligence Agent",      tools: vec!["web_search"],                        approval_type: None,                      needs_approval: false },
            CatalogEntry { id: "generate_hooks",               title: "Generate Hook List",             agent: "Hook Writer Agent",          tools: vec!["copy_generator"],                    approval_type: Some("draft_approval"),    needs_approval: true  },
            CatalogEntry { id: "develop_angles",               title: "Develop Angles",                 agent: "Angle Analyst Agent",        tools: vec![],                                    approval_type: Some("draft_approval"),    needs_approval: true  },
            CatalogEntry { id: "write_scripts",                title: "Write Ad Scripts",               agent: "Script Writer Agent",        tools: vec!["copy_generator"],                    approval_type: Some("draft_approval"),    needs_approval: true  },
            CatalogEntry { id: "write_email_variants",         title: "Write Email Variants",           agent: "Copy Agent",                 tools: vec!["mcp_email"],                         approval_type: Some("draft_approval"),    needs_approval: true  },
            CatalogEntry { id: "video_studio_flow",            title: "Video Ad Studio",                agent: "Video Production Agent",     tools: vec!["video_toolkit", "asset_store"],      approval_type: Some("publish_approval"),  needs_approval: true  },
            CatalogEntry { id: "design_statics",               title: "Design Static Assets",           agent: "Design Agent",               tools: vec!["design_toolkit"],                    approval_type: Some("publish_approval"),  needs_approval: true  },
            CatalogEntry { id: "creative_qa",                  title: "Creative QA",                    agent: "Creative QA Agent",          tools: vec![],                                    approval_type: Some("draft_approval"),    needs_approval: true  },
            CatalogEntry { id: "publish_assets",               title: "Publish Approved Assets",        agent: "Publishing Agent",           tools: vec!["channel_publisher"],                 approval_type: Some("publish_approval"),  needs_approval: true  },
            CatalogEntry { id: "read_performance",             title: "Read Performance Data",          agent: "Performance Analyst",        tools: vec!["analytics_api"],                     approval_type: None,                      needs_approval: false },
            CatalogEntry { id: "build_optimization_plan",      title: "Build Optimization Plan",        agent: "Optimization Agent",         tools: vec![],                                    approval_type: None,                      needs_approval: false },
            CatalogEntry { id: "plan_next_experiments",        title: "Plan Next Experiments",          agent: "Experiment Planner",         tools: vec![],                                    approval_type: Some("spend_approval"),    needs_approval: true  },
        ],
        BusinessMode::School => vec![
            CatalogEntry { id: "define_program_brief",         title: "Define Program Brief",           agent: "Program Architect",          tools: vec![],                                    approval_type: None,                                          needs_approval: false },
            CatalogEntry { id: "sharpen_enrollment_offer",     title: "Sharpen Enrollment Offer",       agent: "Offer Agent",                tools: vec![],                                    approval_type: None,                                          needs_approval: false },
            CatalogEntry { id: "map_student_needs",            title: "Map Student Needs",              agent: "Student Research Agent",     tools: vec!["survey_tool", "web_search"],         approval_type: None,                                          needs_approval: false },
            CatalogEntry { id: "build_curriculum_outline",     title: "Build Curriculum Outline",       agent: "Curriculum Architect",       tools: vec![],                                    approval_type: Some("student_facing_content_approval"),       needs_approval: true  },
            CatalogEntry { id: "write_lessons",                title: "Write Lessons",                  agent: "Lesson Builder Agent",       tools: vec!["copy_generator"],                    approval_type: Some("student_facing_content_approval"),       needs_approval: true  },
            CatalogEntry { id: "design_assignments",           title: "Design Assignments",             agent: "Assignment Designer Agent",  tools: vec![],                                    approval_type: Some("student_facing_content_approval"),       needs_approval: true  },
            CatalogEntry { id: "build_resources",              title: "Build Course Resources",         agent: "Resource Builder Agent",     tools: vec![],                                    approval_type: Some("student_facing_content_approval"),       needs_approval: true  },
            CatalogEntry { id: "run_cohort_onboarding",        title: "Run Cohort Onboarding",          agent: "Cohort Ops Agent",           tools: vec!["task_logger", "mcp_email"],          approval_type: Some("student_facing_content_approval"),       needs_approval: true  },
            CatalogEntry { id: "send_email_reminders",         title: "Send Reminders and Nudges",      agent: "Email Agent",                tools: vec!["mcp_email"],                         approval_type: Some("send_approval"),                         needs_approval: true  },
            CatalogEntry { id: "review_assignments",           title: "Review Assignments",             agent: "Approval Agent",             tools: vec![],                                    approval_type: Some("student_facing_content_approval"),       needs_approval: true  },
            CatalogEntry { id: "track_student_health",         title: "Track Student Health",           agent: "Student Success Agent",      tools: vec!["analytics_api"],                     approval_type: None,                                          needs_approval: false },
            CatalogEntry { id: "capture_testimonials",         title: "Capture Testimonials",           agent: "Coach Support Agent",        tools: vec!["survey_tool", "mcp_email"],          approval_type: None,                                          needs_approval: false },
            CatalogEntry { id: "find_upsells",                 title: "Find Upsell Opportunities",      agent: "Upsell Agent",               tools: vec![],                                    approval_type: None,                                          needs_approval: false },
        ],
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────────

type JsonResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

fn parse_mode(raw: &str) -> Result<BusinessMode, (StatusCode, Json<serde_json::Value>)> {
    BusinessMode::from_str(raw).ok_or_else(|| err(StatusCode::BAD_REQUEST, "Invalid mode — use agency, growth, or school"))
}

// ────────────────────────────────────────────────────────────────────────────
// Record handlers
// ────────────────────────────────────────────────────────────────────────────

async fn create_record(
    State(state): State<ModesState>,
    Path(mode_str): Path<String>,
    Json(req): Json<CreateRecordRequest>,
) -> JsonResult {
    let mode = parse_mode(&mode_str)?;
    let now = Utc::now().to_rfc3339();
    let id  = format!("rec_{}", Uuid::new_v4().simple());

    let record = ModeRecord {
        id:         id.clone(),
        mode,
        title:      req.title.unwrap_or_default(),
        subtitle:   req.subtitle.unwrap_or_default(),
        goal:       req.goal.unwrap_or_default(),
        status:     req.status.unwrap_or_else(|| "draft".into()),
        meta:       req.meta.unwrap_or(serde_json::Value::Object(Default::default())),
        created_at: now.clone(),
        updated_at: now,
    };

    state.write().await.records.insert(id, record.clone());
    Ok(Json(serde_json::json!({ "record": record })))
}

async fn list_records(
    State(state): State<ModesState>,
    Path(mode_str): Path<String>,
) -> JsonResult {
    let mode = parse_mode(&mode_str)?;
    let store = state.read().await;
    let records: Vec<_> = store.records.values()
        .filter(|r| r.mode == mode)
        .cloned()
        .collect();
    Ok(Json(serde_json::json!({ "records": records })))
}

async fn get_record(
    State(state): State<ModesState>,
    Path((mode_str, id)): Path<(String, String)>,
) -> JsonResult {
    let _mode = parse_mode(&mode_str)?;
    let store = state.read().await;
    let record = store.records.get(&id).cloned()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Record not found"))?;
    Ok(Json(serde_json::json!({ "record": record })))
}

async fn update_record(
    State(state): State<ModesState>,
    Path((_mode_str, id)): Path<(String, String)>,
    Json(req): Json<UpdateRecordRequest>,
) -> JsonResult {
    let mut store = state.write().await;
    let record = store.records.get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Record not found"))?;

    let p = req.patch;
    if let Some(v) = p.title    { record.title = v; }
    if let Some(v) = p.subtitle { record.subtitle = v; }
    if let Some(v) = p.goal     { record.goal = v; }
    if let Some(v) = p.status   { record.status = v; }
    if let Some(v) = p.meta     { record.meta = v; }
    record.updated_at = Utc::now().to_rfc3339();

    Ok(Json(serde_json::json!({ "record": record.clone() })))
}

// ────────────────────────────────────────────────────────────────────────────
// Plan generation
// ────────────────────────────────────────────────────────────────────────────

async fn generate_plan(
    State(state): State<ModesState>,
    Path(mode_str): Path<String>,
    Json(req): Json<GeneratePlanRequest>,
) -> JsonResult {
    let mode = parse_mode(&mode_str)?;
    let catalog = catalog_for(&mode);

    // Validate record exists
    {
        let store = state.read().await;
        store.records.get(&req.record_id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Record not found"))?;
    }

    let mut created_tasks = Vec::new();

    for cat_id in &req.selected_task_ids {
        let entry = catalog.iter().find(|e| e.id == cat_id);
        let Some(entry) = entry else { continue };

        let id = format!("tsk_{}", Uuid::new_v4().simple());
        let task = ModeTask {
            id:               id.clone(),
            record_id:        req.record_id.clone(),
            mode:             mode.clone(),
            catalog_id:       entry.id.to_string(),
            title:            entry.title.to_string(),
            assigned_agent:   entry.agent.to_string(),
            required_tools:   entry.tools.iter().map(|s| s.to_string()).collect(),
            approval_required: entry.needs_approval,
            approval_type:    entry.approval_type.map(|s| s.to_string()),
            status:           if entry.needs_approval { "pending_approval".into() } else { "draft".into() },
            approval_status:  if entry.needs_approval { "pending".into() } else { "none".into() },
            priority:         "high".into(),
            depends_on:       vec![],
            output_summary:   String::new(),
        };

        // Create approval item for tasks that need it
        if entry.needs_approval {
            let appr = ModeApproval {
                id:              format!("appr_{}", Uuid::new_v4().simple()),
                task_id:         id.clone(),
                record_id:       req.record_id.clone(),
                mode:            mode.clone(),
                approval_type:   entry.approval_type.unwrap_or("draft_approval").to_string(),
                requested_by:    entry.agent.to_string(),
                status:          "pending".into(),
                preview_summary: format!("Approval required for: {}", entry.title),
                tool_actions:    entry.tools.iter().map(|s| s.to_string()).collect(),
                created_at:      Utc::now().to_rfc3339(),
            };
            state.write().await.approvals.insert(appr.id.clone(), appr);
        }

        state.write().await.tasks.insert(id, task.clone());
        created_tasks.push(task);
    }

    Ok(Json(serde_json::json!({ "tasks": created_tasks })))
}

// ────────────────────────────────────────────────────────────────────────────
// Task handlers
// ────────────────────────────────────────────────────────────────────────────

async fn list_tasks(
    State(state): State<ModesState>,
    Path(mode_str): Path<String>,
    Query(q): Query<RecordIdQuery>,
) -> JsonResult {
    let _mode = parse_mode(&mode_str)?;
    let store = state.read().await;
    let tasks: Vec<_> = store.tasks.values()
        .filter(|t| t.record_id == q.record_id)
        .cloned()
        .collect();
    Ok(Json(serde_json::json!({ "tasks": tasks })))
}

async fn approve_task(
    State(state): State<ModesState>,
    Path((_mode_str, id)): Path<(String, String)>,
) -> JsonResult {
    let mut store = state.write().await;
    let task = store.tasks.get_mut(&id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Task not found"))?;
    task.approval_status = "approved".into();
    task.status          = "approved".into();

    // Update matching approval item
    for appr in store.approvals.values_mut() {
        if appr.task_id == id { appr.status = "approved".into(); }
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn run_task(
    State(state): State<ModesState>,
    Path((mode_str, id)): Path<(String, String)>,
) -> JsonResult {
    let mode = parse_mode(&mode_str)?;
    let task = {
        let store = state.read().await;
        store.tasks.get(&id).cloned()
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Task not found"))?
    };

    if task.approval_required && task.approval_status != "approved" {
        return Err(err(StatusCode::FORBIDDEN, "Approval required before running this task"));
    }

    // Mark running
    {
        let mut store = state.write().await;
        if let Some(t) = store.tasks.get_mut(&id) { t.status = "running".into(); }
    }

    let now = Utc::now().to_rfc3339();
    let result_id = format!("res_{}", Uuid::new_v4().simple());

    let content = match mode {
        BusinessMode::Agency => format!(
            "## {}\n\nTask completed successfully.\n\n**What worked:** Agent executed task in sequence.\n**What failed:** N/A\n**Next action:** Review output and proceed to next task.\n**Owner:** {}",
            task.title, task.assigned_agent
        ),
        BusinessMode::Growth => format!(
            "## {}\n\nCreative output ready for review.\n\n**What worked:** Generated variants as requested.\n**What failed:** N/A\n**Next action:** Review and approve before publishing.\n**Owner:** {}",
            task.title, task.assigned_agent
        ),
        BusinessMode::School => format!(
            "## {}\n\nContent created and ready for student-facing review.\n\n**What worked:** Content aligns with curriculum goals.\n**What failed:** N/A\n**Next action:** Approve before publishing to students.\n**Owner:** {}",
            task.title, task.assigned_agent
        ),
    };

    let result = ModeResult {
        id:               result_id.clone(),
        task_id:          id.clone(),
        record_id:        task.record_id.clone(),
        mode:             mode.clone(),
        title:            task.title.clone(),
        output_type:      "markdown".into(),
        content_markdown: content,
        what_worked:      "Task completed as planned.".into(),
        what_failed:      String::new(),
        next_action:      "Review output and approve next task.".into(),
        owner:            task.assigned_agent.clone(),
        status:           "completed".into(),
        started_at:       now.clone(),
        completed_at:     now,
    };

    {
        let mut store = state.write().await;
        store.results.insert(result_id, result.clone());
        if let Some(t) = store.tasks.get_mut(&id) { t.status = "completed".into(); }
    }

    Ok(Json(serde_json::json!({ "result": result })))
}

// ────────────────────────────────────────────────────────────────────────────
// Approval handlers
// ────────────────────────────────────────────────────────────────────────────

async fn list_approvals(
    State(state): State<ModesState>,
    Path(mode_str): Path<String>,
    Query(q): Query<RecordIdQuery>,
) -> JsonResult {
    let _mode = parse_mode(&mode_str)?;
    let store = state.read().await;
    let approvals: Vec<_> = store.approvals.values()
        .filter(|a| a.record_id == q.record_id)
        .cloned()
        .collect();
    Ok(Json(serde_json::json!({ "approvals": approvals })))
}

// ────────────────────────────────────────────────────────────────────────────
// Result handlers
// ────────────────────────────────────────────────────────────────────────────

async fn list_results(
    State(state): State<ModesState>,
    Path(mode_str): Path<String>,
    Query(q): Query<RecordIdQuery>,
) -> JsonResult {
    let _mode = parse_mode(&mode_str)?;
    let store = state.read().await;
    let results: Vec<_> = store.results.values()
        .filter(|r| r.record_id == q.record_id)
        .cloned()
        .collect();
    Ok(Json(serde_json::json!({ "results": results })))
}
