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

#[derive(Clone)]
pub struct StudioStore {
    pub workspaces: HashMap<String, StudioWorkspace>,
    pub drafts: HashMap<String, Vec<StudioDraft>>,
    pub stages: HashMap<String, Vec<StudioStage>>,
    pub jobs: HashMap<String, Vec<StudioJob>>,
    pub events: HashMap<String, Vec<StudioEvent>>,
    pub approvals: HashMap<String, StudioApproval>,
}

pub type StudioState = Arc<RwLock<StudioStore>>;

pub fn router(state: StudioState) -> Router {
    Router::new()
        .route(
            "/api/studio/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route(
            "/api/studio/workspaces/{workspace_id}",
            get(get_workspace).patch(update_workspace),
        )
        .route(
            "/api/studio/workspaces/{workspace_id}/drafts",
            get(list_drafts).post(create_draft),
        )
        .route(
            "/api/studio/workspaces/{workspace_id}/stages",
            get(list_stages).patch(update_stage),
        )
        .route("/api/studio/jobs", get(list_jobs).post(create_job))
        .route("/api/studio/jobs/{job_id}", get(get_job))
        .route("/api/studio/events", get(list_events))
        .route("/api/studio/approvals", post(approve_target))
        .with_state(state)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioStageKey {
    Brief,
    Research,
    Script,
    Assets,
    Render,
    Approval,
    Schedule,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioWorkspaceStatus {
    Active,
    Queued,
    Approved,
    Scheduled,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioApprovalStatus {
    Pending,
    Approved,
    ChangesRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioDraftStatus {
    Draft,
    Review,
    Approved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioStageStatus {
    Complete,
    Active,
    Queued,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioEventKind {
    System,
    Stage,
    Approval,
    Job,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StudioApprovalTargetType {
    Draft,
    Stage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioWorkspace {
    pub id: String,
    pub title: String,
    pub client_name: String,
    pub objective: String,
    pub primary_channel: String,
    pub output_format: String,
    pub status: StudioWorkspaceStatus,
    pub current_stage: StudioStageKey,
    pub approval_status: StudioApprovalStatus,
    pub summary: String,
    pub active_draft_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioDraft {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub format: String,
    pub stage: StudioStageKey,
    pub status: StudioDraftStatus,
    pub owner: String,
    pub summary: String,
    pub assets_required: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioStage {
    pub id: String,
    pub workspace_id: String,
    pub key: StudioStageKey,
    pub label: String,
    pub status: StudioStageStatus,
    pub owner: String,
    pub next_action: String,
    pub notes: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioJob {
    pub id: String,
    pub workspace_id: String,
    pub label: String,
    pub job_type: String,
    pub provider: String,
    pub status: StudioJobStatus,
    pub progress: u8,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioEvent {
    pub id: String,
    pub workspace_id: String,
    pub kind: StudioEventKind,
    pub title: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudioApproval {
    pub id: String,
    pub workspace_id: String,
    pub target_type: StudioApprovalTargetType,
    pub target_id: String,
    pub status: StudioApprovalStatus,
    pub requested_by: String,
    pub requested_at: String,
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct StudioIndexResponse {
    pub workspaces: Vec<StudioWorkspace>,
    pub jobs: Vec<StudioJob>,
    pub summary: StudioIndexSummary,
}

#[derive(Debug, Serialize)]
pub struct StudioIndexSummary {
    pub live_workspaces: usize,
    pub active_jobs: usize,
    pub approval_backlog: usize,
}

#[derive(Debug, Serialize)]
pub struct StudioWorkspaceDetailResponse {
    pub workspace: StudioWorkspace,
    pub drafts: Vec<StudioDraft>,
    pub stages: Vec<StudioStage>,
    pub jobs: Vec<StudioJob>,
    pub events: Vec<StudioEvent>,
    pub approval: Option<StudioApproval>,
}

#[derive(Debug, Serialize)]
pub struct StudioDraftListResponse {
    pub drafts: Vec<StudioDraft>,
}

#[derive(Debug, Serialize)]
pub struct StudioStageListResponse {
    pub stages: Vec<StudioStage>,
}

#[derive(Debug, Serialize)]
pub struct StudioJobListResponse {
    pub jobs: Vec<StudioJob>,
}

#[derive(Debug, Serialize)]
pub struct StudioEventListResponse {
    pub events: Vec<StudioEvent>,
}

#[derive(Debug, Serialize)]
pub struct StudioWorkspaceEnvelope {
    pub workspace: StudioWorkspace,
}

#[derive(Debug, Serialize)]
pub struct StudioDraftEnvelope {
    pub draft: StudioDraft,
}

#[derive(Debug, Serialize)]
pub struct StudioJobEnvelope {
    pub job: StudioJob,
}

#[derive(Debug, Serialize)]
pub struct StudioApprovalEnvelope {
    pub approval: StudioApproval,
}

#[derive(Debug, Deserialize)]
pub struct CreateStudioWorkspaceRequest {
    pub title: Option<String>,
    pub client_name: Option<String>,
    pub objective: Option<String>,
    pub primary_channel: Option<String>,
    pub output_format: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStudioWorkspaceRequest {
    pub title: Option<String>,
    pub client_name: Option<String>,
    pub objective: Option<String>,
    pub primary_channel: Option<String>,
    pub output_format: Option<String>,
    pub status: Option<StudioWorkspaceStatus>,
    pub current_stage: Option<StudioStageKey>,
    pub approval_status: Option<StudioApprovalStatus>,
}

#[derive(Debug, Deserialize)]
pub struct CreateStudioDraftRequest {
    pub title: Option<String>,
    pub format: Option<String>,
    pub stage: Option<StudioStageKey>,
    pub owner: Option<String>,
    pub summary: Option<String>,
    pub assets_required: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStudioStageRequest {
    pub key: StudioStageKey,
    pub status: Option<StudioStageStatus>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateStudioJobRequest {
    pub workspace_id: String,
    pub label: Option<String>,
    pub job_type: Option<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StudioJobsQuery {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StudioEventsQuery {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct StudioApprovalRequest {
    pub workspace_id: String,
    pub target_type: StudioApprovalTargetType,
    pub target_id: String,
    pub status: StudioApprovalStatus,
}

type JsonResult<T> = Result<Json<T>, (StatusCode, Json<serde_json::Value>)>;

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn err(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": message })))
}

fn stage_label(stage: &StudioStageKey) -> &'static str {
    match stage {
        StudioStageKey::Brief => "Brief",
        StudioStageKey::Research => "Research",
        StudioStageKey::Script => "Script",
        StudioStageKey::Assets => "Assets",
        StudioStageKey::Render => "Render",
        StudioStageKey::Approval => "Approval",
        StudioStageKey::Schedule => "Schedule",
    }
}

fn next_action(stage: &StudioStageKey) -> &'static str {
    match stage {
        StudioStageKey::Brief => "Lock the brief and success metric.",
        StudioStageKey::Research => "Pull source material and angle inventory.",
        StudioStageKey::Script => "Finalize the script and on-screen structure.",
        StudioStageKey::Assets => "Request all footage, captions, and overlays.",
        StudioStageKey::Render => "Queue renders across the selected providers.",
        StudioStageKey::Approval => "Resolve review notes before scheduling.",
        StudioStageKey::Schedule => "Publish the approved cut to channel ops.",
    }
}

fn next_stage_key(stage: &StudioStageKey) -> Option<StudioStageKey> {
    match stage {
        StudioStageKey::Brief => Some(StudioStageKey::Research),
        StudioStageKey::Research => Some(StudioStageKey::Script),
        StudioStageKey::Script => Some(StudioStageKey::Assets),
        StudioStageKey::Assets => Some(StudioStageKey::Render),
        StudioStageKey::Render => Some(StudioStageKey::Approval),
        StudioStageKey::Approval => Some(StudioStageKey::Schedule),
        StudioStageKey::Schedule => None,
    }
}

fn push_event(store: &mut StudioStore, workspace_id: &str, kind: StudioEventKind, title: String, message: String) {
    let events = store.events.entry(workspace_id.to_string()).or_default();
    events.insert(
        0,
        StudioEvent {
            id: Uuid::new_v4().to_string(),
            workspace_id: workspace_id.to_string(),
            kind,
            title,
            message,
            created_at: now_iso(),
        },
    );
}

fn default_stages(workspace: &StudioWorkspace) -> Vec<StudioStage> {
    let all = [
        StudioStageKey::Brief,
        StudioStageKey::Research,
        StudioStageKey::Script,
        StudioStageKey::Assets,
        StudioStageKey::Render,
        StudioStageKey::Approval,
        StudioStageKey::Schedule,
    ];
    let current_index = all
        .iter()
        .position(|stage| stage == &workspace.current_stage)
        .unwrap_or(0);

    all.iter()
        .enumerate()
        .map(|(index, stage)| StudioStage {
            id: format!("{}-{}", workspace.id, stage_label(stage).to_lowercase()),
            workspace_id: workspace.id.clone(),
            key: stage.clone(),
            label: stage_label(stage).to_string(),
            status: if index < current_index {
                StudioStageStatus::Complete
            } else if index == current_index {
                StudioStageStatus::Active
            } else {
                StudioStageStatus::Queued
            },
            owner: match stage {
                StudioStageKey::Brief => "strategy-lead".to_string(),
                StudioStageKey::Research => "research-agent".to_string(),
                StudioStageKey::Script => "writer".to_string(),
                StudioStageKey::Assets => "creative-ops".to_string(),
                StudioStageKey::Render => "media-pipeline".to_string(),
                StudioStageKey::Approval => "human-review".to_string(),
                StudioStageKey::Schedule => "channel-ops".to_string(),
            },
            next_action: next_action(stage).to_string(),
            notes: if index < current_index {
                format!("{} is complete.", stage_label(stage))
            } else if index == current_index {
                format!("{} is in flight.", stage_label(stage))
            } else {
                format!("{} is queued.", stage_label(stage))
            },
            updated_at: workspace.updated_at.clone(),
        })
        .collect()
}

fn default_drafts(workspace: &StudioWorkspace) -> Vec<StudioDraft> {
    vec![
        StudioDraft {
            id: format!("{}-draft-1", workspace.id),
            workspace_id: workspace.id.clone(),
            title: format!("{} main cut", workspace.title),
            format: workspace.output_format.clone(),
            stage: workspace.current_stage.clone(),
            status: if workspace.approval_status == StudioApprovalStatus::Approved {
                StudioDraftStatus::Approved
            } else {
                StudioDraftStatus::Review
            },
            owner: "studio-director".to_string(),
            summary: format!(
                "Primary {} draft aligned to the {} stage.",
                workspace.output_format.replace('_', " "),
                stage_label(&workspace.current_stage)
            ),
            assets_required: vec![
                "Hook line".to_string(),
                "B-roll selection".to_string(),
                "Caption pass".to_string(),
            ],
            updated_at: workspace.updated_at.clone(),
        },
        StudioDraft {
            id: format!("{}-draft-2", workspace.id),
            workspace_id: workspace.id.clone(),
            title: format!("{} alternate hook", workspace.title),
            format: workspace.output_format.clone(),
            stage: StudioStageKey::Script,
            status: StudioDraftStatus::Draft,
            owner: "writer".to_string(),
            summary: "Backup angle focused on urgency and proof.".to_string(),
            assets_required: vec![
                "Alternative opener".to_string(),
                "Supporting testimonial".to_string(),
            ],
            updated_at: workspace.updated_at.clone(),
        },
    ]
}

fn default_jobs(workspace: &StudioWorkspace) -> Vec<StudioJob> {
    vec![
        StudioJob {
            id: format!("{}-job-render", workspace.id),
            workspace_id: workspace.id.clone(),
            label: "Render v1 social cut".to_string(),
            job_type: "render".to_string(),
            provider: "runway+ffmpeg".to_string(),
            status: if workspace.current_stage == StudioStageKey::Render {
                StudioJobStatus::Running
            } else {
                StudioJobStatus::Completed
            },
            progress: if workspace.current_stage == StudioStageKey::Render { 62 } else { 100 },
            created_at: workspace.updated_at.clone(),
            updated_at: workspace.updated_at.clone(),
        },
        StudioJob {
            id: format!("{}-job-package", workspace.id),
            workspace_id: workspace.id.clone(),
            label: "Package captions and export bundle".to_string(),
            job_type: "packaging".to_string(),
            provider: "openfang-media".to_string(),
            status: if workspace.current_stage == StudioStageKey::Approval
                || workspace.current_stage == StudioStageKey::Schedule
            {
                StudioJobStatus::Running
            } else {
                StudioJobStatus::Queued
            },
            progress: if workspace.current_stage == StudioStageKey::Approval
                || workspace.current_stage == StudioStageKey::Schedule
            {
                35
            } else {
                0
            },
            created_at: workspace.updated_at.clone(),
            updated_at: workspace.updated_at.clone(),
        },
    ]
}

fn default_approval(workspace: &StudioWorkspace) -> StudioApproval {
    StudioApproval {
        id: format!("{}-approval", workspace.id),
        workspace_id: workspace.id.clone(),
        target_type: StudioApprovalTargetType::Draft,
        target_id: workspace
            .active_draft_id
            .clone()
            .unwrap_or_else(|| format!("{}-draft-1", workspace.id)),
        status: workspace.approval_status.clone(),
        requested_by: "studio-director".to_string(),
        requested_at: workspace.updated_at.clone(),
        summary: "Human review required before scheduling the latest exported cut.".to_string(),
    }
}

struct StudioSeed<'a> {
    title: &'a str,
    client_name: &'a str,
    objective: &'a str,
    primary_channel: &'a str,
    output_format: &'a str,
    current_stage: StudioStageKey,
    approval_status: StudioApprovalStatus,
    status: StudioWorkspaceStatus,
}

fn seed_workspace(store: &mut StudioStore, seed: StudioSeed<'_>) {
    let workspace_id = Uuid::new_v4().to_string();
    let updated_at = now_iso();
    let workspace = StudioWorkspace {
        id: workspace_id.clone(),
        title: seed.title.to_string(),
        client_name: seed.client_name.to_string(),
        objective: seed.objective.to_string(),
        primary_channel: seed.primary_channel.to_string(),
        output_format: seed.output_format.to_string(),
        status: seed.status,
        current_stage: seed.current_stage,
        approval_status: seed.approval_status,
        summary: "Workspace seeded from the in-memory studio router.".to_string(),
        active_draft_id: Some(format!("{}-draft-1", workspace_id)),
        updated_at,
    };
    store
        .drafts
        .insert(workspace_id.clone(), default_drafts(&workspace));
    store
        .stages
        .insert(workspace_id.clone(), default_stages(&workspace));
    store.jobs.insert(workspace_id.clone(), default_jobs(&workspace));
    store.events.insert(
        workspace_id.clone(),
        vec![StudioEvent {
            id: Uuid::new_v4().to_string(),
            workspace_id: workspace_id.clone(),
            kind: StudioEventKind::Stage,
            title: "Workspace advanced".to_string(),
            message: format!(
                "Current stage moved to {}.",
                stage_label(&workspace.current_stage)
            ),
            created_at: workspace.updated_at.clone(),
        }],
    );
    store
        .approvals
        .insert(workspace_id.clone(), default_approval(&workspace));
    store.workspaces.insert(workspace_id, workspace);
}

impl Default for StudioStore {
    fn default() -> Self {
        let mut store = Self {
            workspaces: HashMap::new(),
            drafts: HashMap::new(),
            stages: HashMap::new(),
            jobs: HashMap::new(),
            events: HashMap::new(),
            approvals: HashMap::new(),
        };
        seed_workspace(
            &mut store,
            StudioSeed {
                title: "Founder story launch",
                client_name: "Northstar Labs",
                objective: "Ship a founder-led launch cut for the waitlist push.",
                primary_channel: "linkedin",
                output_format: "vertical_video",
                current_stage: StudioStageKey::Approval,
                approval_status: StudioApprovalStatus::Pending,
                status: StudioWorkspaceStatus::Active,
            },
        );
        seed_workspace(
            &mut store,
            StudioSeed {
                title: "UGC testimonial sprint",
                client_name: "Signal Forge",
                objective: "Turn customer proof into a three-cut paid social package.",
                primary_channel: "meta_ads",
                output_format: "paid_social_bundle",
                current_stage: StudioStageKey::Render,
                approval_status: StudioApprovalStatus::ChangesRequested,
                status: StudioWorkspaceStatus::Queued,
            },
        );
        store
    }
}

fn workspace_detail(store: &StudioStore, workspace_id: &str) -> Option<StudioWorkspaceDetailResponse> {
    let workspace = store.workspaces.get(workspace_id)?.clone();
    Some(StudioWorkspaceDetailResponse {
        drafts: store.drafts.get(workspace_id).cloned().unwrap_or_default(),
        stages: store.stages.get(workspace_id).cloned().unwrap_or_default(),
        jobs: store.jobs.get(workspace_id).cloned().unwrap_or_default(),
        events: store.events.get(workspace_id).cloned().unwrap_or_default(),
        approval: store.approvals.get(workspace_id).cloned(),
        workspace,
    })
}

async fn list_workspaces(State(state): State<StudioState>) -> Json<StudioIndexResponse> {
    let store = state.read().await;
    let mut workspaces = store.workspaces.values().cloned().collect::<Vec<_>>();
    workspaces.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    let mut jobs = store
        .jobs
        .values()
        .flat_map(|workspace_jobs| workspace_jobs.clone())
        .collect::<Vec<_>>();
    jobs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    Json(StudioIndexResponse {
        summary: StudioIndexSummary {
            live_workspaces: workspaces.len(),
            active_jobs: jobs
                .iter()
                .filter(|job| {
                    job.status == StudioJobStatus::Queued || job.status == StudioJobStatus::Running
                })
                .count(),
            approval_backlog: workspaces
                .iter()
                .filter(|workspace| workspace.approval_status == StudioApprovalStatus::Pending)
                .count(),
        },
        workspaces,
        jobs,
    })
}

async fn create_workspace(
    State(state): State<StudioState>,
    Json(request): Json<CreateStudioWorkspaceRequest>,
) -> Json<StudioWorkspaceEnvelope> {
    let workspace_id = Uuid::new_v4().to_string();
    let updated_at = now_iso();
    let title = request.title.unwrap_or_else(|| "Untitled workspace".to_string());
    let client_name = request
        .client_name
        .unwrap_or_else(|| "OpenFang client".to_string());
    let objective = request.objective.unwrap_or_else(|| {
        "Create a production-ready creator brief and move it into the media pipeline."
            .to_string()
    });
    let primary_channel = request
        .primary_channel
        .unwrap_or_else(|| "youtube_shorts".to_string());
    let output_format = request
        .output_format
        .unwrap_or_else(|| "vertical_video".to_string());

    let workspace = StudioWorkspace {
        id: workspace_id.clone(),
        title: title.clone(),
        client_name,
        objective,
        primary_channel,
        output_format,
        status: StudioWorkspaceStatus::Active,
        current_stage: StudioStageKey::Brief,
        approval_status: StudioApprovalStatus::Pending,
        summary: format!("{} is at the brief stage and ready for stage handoff.", title),
        active_draft_id: Some(format!("{}-draft-1", workspace_id)),
        updated_at,
    };

    let mut store = state.write().await;
    store
        .drafts
        .insert(workspace_id.clone(), default_drafts(&workspace));
    store
        .stages
        .insert(workspace_id.clone(), default_stages(&workspace));
    store.jobs.insert(workspace_id.clone(), Vec::new());
    store.events.insert(workspace_id.clone(), Vec::new());
    store
        .approvals
        .insert(workspace_id.clone(), default_approval(&workspace));
    store.workspaces.insert(workspace_id.clone(), workspace.clone());
    push_event(
        &mut store,
        &workspace_id,
        StudioEventKind::System,
        "Workspace created".to_string(),
        format!("Studio workspace '{}' was created.", workspace.title),
    );

    Json(StudioWorkspaceEnvelope { workspace })
}

async fn get_workspace(
    Path(workspace_id): Path<String>,
    State(state): State<StudioState>,
) -> JsonResult<StudioWorkspaceDetailResponse> {
    let store = state.read().await;
    let detail = workspace_detail(&store, &workspace_id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Studio workspace not found"))?;
    Ok(Json(detail))
}

async fn update_workspace(
    Path(workspace_id): Path<String>,
    State(state): State<StudioState>,
    Json(request): Json<UpdateStudioWorkspaceRequest>,
) -> JsonResult<StudioWorkspaceEnvelope> {
    let mut store = state.write().await;
    let (updated, current_stage, updated_at, approval_status) = {
        let workspace = store
            .workspaces
            .get_mut(&workspace_id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Studio workspace not found"))?;

        if let Some(title) = request.title {
            workspace.title = title;
        }
        if let Some(client_name) = request.client_name {
            workspace.client_name = client_name;
        }
        if let Some(objective) = request.objective {
            workspace.objective = objective;
        }
        if let Some(primary_channel) = request.primary_channel {
            workspace.primary_channel = primary_channel;
        }
        if let Some(output_format) = request.output_format {
            workspace.output_format = output_format;
        }
        if let Some(status) = request.status {
            workspace.status = status;
        }
        if let Some(current_stage) = request.current_stage {
            workspace.current_stage = current_stage;
        }
        if let Some(approval_status) = request.approval_status.clone() {
            workspace.approval_status = approval_status;
        }
        workspace.updated_at = now_iso();
        (
            workspace.clone(),
            workspace.current_stage.clone(),
            workspace.updated_at.clone(),
            request.approval_status.clone(),
        )
    };

    if let Some(approval_status) = approval_status {
        if let Some(approval) = store.approvals.get_mut(&workspace_id) {
            approval.status = approval_status;
            approval.requested_at = updated_at.clone();
        }
    }

    if let Some(stages) = store.stages.get_mut(&workspace_id) {
        for stage in stages.iter_mut() {
            stage.status = if stage.key == current_stage {
                StudioStageStatus::Active
            } else if stage_position(&stage.key) < stage_position(&current_stage) {
                StudioStageStatus::Complete
            } else {
                StudioStageStatus::Queued
            };
            stage.updated_at = updated_at.clone();
        }
    }

    push_event(
        &mut store,
        &workspace_id,
        StudioEventKind::System,
        "Workspace updated".to_string(),
        format!("Studio workspace '{}' metadata was updated.", updated.title),
    );
    Ok(Json(StudioWorkspaceEnvelope { workspace: updated }))
}

fn stage_position(stage: &StudioStageKey) -> usize {
    match stage {
        StudioStageKey::Brief => 0,
        StudioStageKey::Research => 1,
        StudioStageKey::Script => 2,
        StudioStageKey::Assets => 3,
        StudioStageKey::Render => 4,
        StudioStageKey::Approval => 5,
        StudioStageKey::Schedule => 6,
    }
}

async fn list_drafts(
    Path(workspace_id): Path<String>,
    State(state): State<StudioState>,
) -> JsonResult<StudioDraftListResponse> {
    let store = state.read().await;
    if !store.workspaces.contains_key(&workspace_id) {
        return Err(err(StatusCode::NOT_FOUND, "Studio workspace not found"));
    }
    Ok(Json(StudioDraftListResponse {
        drafts: store.drafts.get(&workspace_id).cloned().unwrap_or_default(),
    }))
}

async fn create_draft(
    Path(workspace_id): Path<String>,
    State(state): State<StudioState>,
    Json(request): Json<CreateStudioDraftRequest>,
) -> JsonResult<StudioDraftEnvelope> {
    let mut store = state.write().await;
    let (workspace_title, output_format, current_stage) = {
        let workspace = store
            .workspaces
            .get(&workspace_id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Studio workspace not found"))?;
        (
            workspace.title.clone(),
            workspace.output_format.clone(),
            workspace.current_stage.clone(),
        )
    };
    let draft = StudioDraft {
        id: Uuid::new_v4().to_string(),
        workspace_id: workspace_id.clone(),
        title: request.title.unwrap_or_else(|| format!("{} draft", workspace_title)),
        format: request.format.unwrap_or(output_format),
        stage: request.stage.unwrap_or(current_stage),
        status: StudioDraftStatus::Draft,
        owner: request.owner.unwrap_or_else(|| "studio-director".to_string()),
        summary: request.summary.unwrap_or_else(|| {
            "New draft created from the studio workspace detail surface.".to_string()
        }),
        assets_required: request.assets_required.unwrap_or_else(|| {
            vec!["Narration".to_string(), "Thumbnail frame".to_string()]
        }),
        updated_at: now_iso(),
    };
    let draft_id = draft.id.clone();
    let updated_at = draft.updated_at.clone();
    store.drafts.entry(workspace_id.clone()).or_default().insert(0, draft.clone());
    if let Some(workspace) = store.workspaces.get_mut(&workspace_id) {
        workspace.active_draft_id = Some(draft_id);
        workspace.updated_at = updated_at;
    }
    push_event(
        &mut store,
        &workspace_id,
        StudioEventKind::System,
        "Draft created".to_string(),
        format!("Draft '{}' was added to the workspace.", draft.title),
    );
    Ok(Json(StudioDraftEnvelope { draft }))
}

async fn list_stages(
    Path(workspace_id): Path<String>,
    State(state): State<StudioState>,
) -> JsonResult<StudioStageListResponse> {
    let store = state.read().await;
    if !store.workspaces.contains_key(&workspace_id) {
        return Err(err(StatusCode::NOT_FOUND, "Studio workspace not found"));
    }
    Ok(Json(StudioStageListResponse {
        stages: store.stages.get(&workspace_id).cloned().unwrap_or_default(),
    }))
}

async fn update_stage(
    Path(workspace_id): Path<String>,
    State(state): State<StudioState>,
    Json(request): Json<UpdateStudioStageRequest>,
) -> JsonResult<StudioStageListResponse> {
    let mut store = state.write().await;
    let (current_stage, updated_at) = {
        let workspace = store
            .workspaces
            .get_mut(&workspace_id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Studio workspace not found"))?;

        if let Some(status) = request.status.clone() {
            if status == StudioStageStatus::Active {
                workspace.current_stage = request.key.clone();
            }
            if status == StudioStageStatus::Complete {
                if let Some(next_stage) = next_stage_key(&request.key) {
                    workspace.current_stage = next_stage;
                }
            }
        }
        workspace.updated_at = now_iso();
        (workspace.current_stage.clone(), workspace.updated_at.clone())
    };

    let stages = store
        .stages
        .get_mut(&workspace_id)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Studio stages not found"))?;

    for stage in stages.iter_mut() {
        if stage.key == request.key {
            if let Some(status) = request.status.clone() {
                stage.status = status;
            }
            if let Some(notes) = request.notes.clone() {
                stage.notes = notes;
            }
            stage.updated_at = updated_at.clone();
        } else if stage_position(&stage.key) < stage_position(&current_stage) {
            stage.status = StudioStageStatus::Complete;
            stage.updated_at = updated_at.clone();
        } else if stage.key == current_stage {
            stage.status = StudioStageStatus::Active;
            stage.updated_at = updated_at.clone();
        } else {
            stage.status = StudioStageStatus::Queued;
            stage.updated_at = updated_at.clone();
        }
    }

    let response_stages = stages.clone();
    push_event(
        &mut store,
        &workspace_id,
        StudioEventKind::Stage,
        "Stage updated".to_string(),
        format!("Stage '{}' was updated.", stage_label(&request.key)),
    );
    Ok(Json(StudioStageListResponse { stages: response_stages }))
}

async fn list_jobs(
    Query(query): Query<StudioJobsQuery>,
    State(state): State<StudioState>,
) -> Json<StudioJobListResponse> {
    let store = state.read().await;
    let jobs = if let Some(workspace_id) = query.workspace_id {
        store.jobs.get(&workspace_id).cloned().unwrap_or_default()
    } else {
        store
            .jobs
            .values()
            .flat_map(|workspace_jobs| workspace_jobs.clone())
            .collect::<Vec<_>>()
    };
    Json(StudioJobListResponse { jobs })
}

async fn create_job(
    State(state): State<StudioState>,
    Json(request): Json<CreateStudioJobRequest>,
) -> JsonResult<StudioJobEnvelope> {
    let mut store = state.write().await;
    let workspace_title = {
        let workspace = store
            .workspaces
            .get(&request.workspace_id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Studio workspace not found"))?;
        workspace.title.clone()
    };
    let created_at = now_iso();
    let job = StudioJob {
        id: Uuid::new_v4().to_string(),
        workspace_id: request.workspace_id.clone(),
        label: request
            .label
            .unwrap_or_else(|| format!("Render {}", workspace_title)),
        job_type: request.job_type.unwrap_or_else(|| "render".to_string()),
        provider: request.provider.unwrap_or_else(|| "openfang-media".to_string()),
        status: StudioJobStatus::Queued,
        progress: 0,
        created_at: created_at.clone(),
        updated_at: created_at.clone(),
    };
    if let Some(workspace) = store.workspaces.get_mut(&request.workspace_id) {
        workspace.updated_at = created_at;
    }
    store
        .jobs
        .entry(request.workspace_id.clone())
        .or_default()
        .insert(0, job.clone());
    push_event(
        &mut store,
        &request.workspace_id,
        StudioEventKind::Job,
        "Job queued".to_string(),
        format!("Job '{}' was queued.", job.label),
    );
    Ok(Json(StudioJobEnvelope { job }))
}

async fn get_job(
    Path(job_id): Path<String>,
    State(state): State<StudioState>,
) -> JsonResult<StudioJobEnvelope> {
    let store = state.read().await;
    let job = store
        .jobs
        .values()
        .flat_map(|workspace_jobs| workspace_jobs.iter())
        .find(|job| job.id == job_id)
        .cloned()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Studio job not found"))?;
    Ok(Json(StudioJobEnvelope { job }))
}

async fn list_events(
    Query(query): Query<StudioEventsQuery>,
    State(state): State<StudioState>,
) -> Json<StudioEventListResponse> {
    let store = state.read().await;
    let mut events = if let Some(workspace_id) = query.workspace_id {
        store.events.get(&workspace_id).cloned().unwrap_or_default()
    } else {
        store
            .events
            .values()
            .flat_map(|workspace_events| workspace_events.clone())
            .collect::<Vec<_>>()
    };
    events.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Json(StudioEventListResponse { events })
}

async fn approve_target(
    State(state): State<StudioState>,
    Json(request): Json<StudioApprovalRequest>,
) -> JsonResult<StudioApprovalEnvelope> {
    let mut store = state.write().await;
    let updated_at = {
        let workspace = store
            .workspaces
            .get_mut(&request.workspace_id)
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Studio workspace not found"))?;
        workspace.approval_status = request.status.clone();
        if request.status == StudioApprovalStatus::Approved
            && workspace.current_stage == StudioStageKey::Approval
        {
            workspace.current_stage = StudioStageKey::Schedule;
            workspace.status = StudioWorkspaceStatus::Scheduled;
        }
        workspace.updated_at = now_iso();
        workspace.updated_at.clone()
    };

    let approval = StudioApproval {
        id: format!("{}-approval", request.workspace_id),
        workspace_id: request.workspace_id.clone(),
        target_type: request.target_type,
        target_id: request.target_id.clone(),
        status: request.status.clone(),
        requested_by: "studio-director".to_string(),
        requested_at: updated_at,
        summary: match request.status {
            StudioApprovalStatus::Approved => {
                "Approval was granted and the workspace can move to scheduling.".to_string()
            }
            StudioApprovalStatus::Pending => {
                "Approval is pending review before the workspace can advance.".to_string()
            }
            StudioApprovalStatus::ChangesRequested => {
                "Changes were requested before the workspace can advance.".to_string()
            }
        },
    };
    store
        .approvals
        .insert(request.workspace_id.clone(), approval.clone());
    push_event(
        &mut store,
        &request.workspace_id,
        StudioEventKind::Approval,
        "Approval updated".to_string(),
        format!("Approval status changed to '{:?}'.", approval.status),
    );
    Ok(Json(StudioApprovalEnvelope { approval }))
}