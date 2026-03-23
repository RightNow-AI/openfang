export type StudioStageKey =
  | 'brief'
  | 'research'
  | 'script'
  | 'assets'
  | 'render'
  | 'approval'
  | 'schedule';

export type StudioStatus = 'active' | 'queued' | 'approved' | 'scheduled' | 'blocked';

export type StudioJobStatus = 'queued' | 'running' | 'completed' | 'failed';

export interface StudioWorkspace {
  id: string;
  title: string;
  client_name: string;
  objective: string;
  primary_channel: string;
  output_format: string;
  status: StudioStatus;
  current_stage: StudioStageKey;
  approval_status: 'pending' | 'approved' | 'changes_requested';
  summary: string;
  active_draft_id: string | null;
  updated_at: string;
}

export interface StudioDraft {
  id: string;
  workspace_id: string;
  title: string;
  format: string;
  stage: StudioStageKey;
  status: 'draft' | 'review' | 'approved';
  owner: string;
  summary: string;
  assets_required: string[];
  updated_at: string;
}

export interface StudioStage {
  id: string;
  workspace_id: string;
  key: StudioStageKey;
  label: string;
  status: 'complete' | 'active' | 'queued' | 'blocked';
  owner: string;
  next_action: string;
  notes: string;
  updated_at: string;
}

export interface StudioJob {
  id: string;
  workspace_id: string;
  label: string;
  job_type: string;
  provider: string;
  status: StudioJobStatus;
  progress: number;
  created_at: string;
  updated_at: string;
}

export interface StudioEvent {
  id: string;
  workspace_id: string;
  kind: 'system' | 'stage' | 'approval' | 'job';
  title: string;
  message: string;
  created_at: string;
}

export interface StudioApproval {
  id: string;
  workspace_id: string;
  target_type: 'draft' | 'stage';
  target_id: string;
  status: 'pending' | 'approved' | 'changes_requested';
  requested_by: string;
  requested_at: string;
  summary: string;
}

export interface StudioIndexPayload {
  workspaces: StudioWorkspace[];
  jobs: StudioJob[];
  summary: {
    live_workspaces: number;
    active_jobs: number;
    approval_backlog: number;
  };
}

export interface StudioWorkspaceDetailPayload {
  workspace: StudioWorkspace;
  drafts: StudioDraft[];
  stages: StudioStage[];
  jobs: StudioJob[];
  events: StudioEvent[];
  approval: StudioApproval | null;
}

export interface CreateStudioWorkspaceInput {
  title: string;
  client_name: string;
  objective: string;
  primary_channel: string;
  output_format: string;
}

export type StudioDraftPipelineStage =
  | 'research'
  | 'script'
  | 'voice'
  | 'visuals'
  | 'edit'
  | 'publish';

export type StudioDraftRuntimeStatus =
  | 'Draft'
  | 'Running'
  | 'AwaitingApproval'
  | 'Ready'
  | 'Failed'
  | 'Queued'
  | 'Published';

export type StudioArtifactType =
  | 'ResearchPack'
  | 'ScriptVersion'
  | 'Script'
  | 'VoiceTrack'
  | 'ImageAsset'
  | 'ScenePlan'
  | 'PreviewRender'
  | 'FinalRender';

export type StudioPipelineEventType =
  | 'job.started'
  | 'job.progress'
  | 'artifact.created'
  | 'draft.stage_changed'
  | 'job.failed';

export interface StudioScriptPayload {
  hook: string;
  body: string;
  cta: string;
  wordCount: number;
}

export interface StudioArtifact {
  id: string;
  artifactType: StudioArtifactType;
  label?: string;
  url?: string | null;
  posterUrl?: string | null;
  json?: Record<string, unknown> | StudioScriptPayload | null;
  createdAt: string;
}

export interface StudioArtifactMap {
  research?: StudioArtifact | null;
  script?: StudioArtifact | null;
  voice?: StudioArtifact | null;
  visuals?: StudioArtifact[];
  previewRender?: StudioArtifact | null;
  finalRender?: StudioArtifact | null;
}

export interface StudioDraftRecord {
  id: string;
  workspaceId: string;
  workspaceName: string;
  topic: string;
  playbook: string;
  format: string;
  targetDurationSec: number;
  stage: StudioDraftPipelineStage;
  status: StudioDraftRuntimeStatus;
  updatedAt: string;
  createdAt: string;
  artifacts: StudioArtifactMap;
  failureMessage?: string | null;
}

export interface StudioWorkspaceStats {
  publishedLast7Days: number;
  scheduledCount: number;
}

export interface StudioWorkspaceRecord {
  id: string;
  name: string;
  niche: string;
  platform: 'youtube' | 'tiktok';
  language: string;
  publishGoalPerDay: number;
  createdAt: string;
  updatedAt: string;
  stats: StudioWorkspaceStats;
}

export interface StudioPolicyAlert {
  id: string;
  workspaceId: string;
  message: string;
  severity: 'warning' | 'danger';
  draftId?: string | null;
}

export interface StudioWorkspaceDashboardPayload {
  workspace: StudioWorkspaceRecord;
  drafts: StudioDraftRecord[];
  alerts: StudioPolicyAlert[];
}

export interface StudioDraftPagePayload {
  draft: StudioDraftRecord;
  workspace: StudioWorkspaceRecord;
}

export interface StudioPipelineEvent {
  type: StudioPipelineEventType;
  draftId: string;
  timestamp: string;
  jobId?: string;
  stage?: string;
  progress?: number;
  artifactType?: string;
  count?: number;
  status?: string;
  error?: string;
  artifact?: StudioArtifact;
}

export interface CreateStudioWorkspaceWizardInput {
  name: string;
  platform: 'youtube' | 'tiktok';
  niche: string;
  language?: string;
  publishGoalPerDay: number;
}

export interface CreateStudioDraftInput {
  topic: string;
  playbook?: string;
  format?: string;
  targetDurationSec?: number;
}

export interface UpdateStudioDraftInput {
  topic?: string;
  stage?: StudioDraftPipelineStage;
  status?: StudioDraftRuntimeStatus;
  failureMessage?: string | null;
  artifacts?: Partial<StudioArtifactMap>;
}
