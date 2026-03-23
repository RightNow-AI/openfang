import type {
  CreateStudioWorkspaceInput,
  StudioApproval,
  StudioDraft,
  StudioEvent,
  StudioIndexPayload,
  StudioJob,
  StudioStage,
  StudioStageKey,
  StudioWorkspace,
  StudioWorkspaceDetailPayload,
} from './studio-types';

const STAGE_BLUEPRINT: Array<{ key: StudioStageKey; label: string; owner: string; next_action: string }> = [
  { key: 'brief', label: 'Brief', owner: 'Strategy lead', next_action: 'Lock the offer, audience, and conversion target.' },
  { key: 'research', label: 'Research', owner: 'Research agent', next_action: 'Pull market angles, hooks, and objections from source notes.' },
  { key: 'script', label: 'Script', owner: 'Writer', next_action: 'Turn the winning angle into a production-ready script.' },
  { key: 'assets', label: 'Assets', owner: 'Creative ops', next_action: 'Request footage, stills, captions, and voice requirements.' },
  { key: 'render', label: 'Render', owner: 'Media pipeline', next_action: 'Queue render jobs across the selected provider stack.' },
  { key: 'approval', label: 'Approval', owner: 'Human review', next_action: 'Resolve review notes before the draft is scheduled.' },
  { key: 'schedule', label: 'Schedule', owner: 'Channel ops', next_action: 'Push the approved cut into the publishing calendar.' },
];

function isoAt(minutesAgo: number) {
  return new Date(Date.now() - minutesAgo * 60_000).toISOString();
}

function stageLabel(key: StudioStageKey) {
  return STAGE_BLUEPRINT.find((stage) => stage.key === key)?.label ?? key;
}

function makeWorkspace(seed: {
  id: string;
  title: string;
  client_name: string;
  objective: string;
  primary_channel: string;
  output_format: string;
  status: StudioWorkspace['status'];
  current_stage: StudioStageKey;
  approval_status: StudioWorkspace['approval_status'];
  summary: string;
  updated_at: string;
}): StudioWorkspace {
  return {
    ...seed,
    active_draft_id: `${seed.id}-draft-1`,
  };
}

function makeStages(workspace: StudioWorkspace): StudioStage[] {
  const activeIndex = STAGE_BLUEPRINT.findIndex((stage) => stage.key === workspace.current_stage);
  return STAGE_BLUEPRINT.map((stage, index) => ({
    id: `${workspace.id}-${stage.key}`,
    workspace_id: workspace.id,
    key: stage.key,
    label: stage.label,
    owner: stage.owner,
    next_action: stage.next_action,
    notes:
      index < activeIndex
        ? `${stage.label} is complete and handed off downstream.`
        : index === activeIndex
          ? `The team is actively working the ${stage.label.toLowerCase()} step.`
          : `${stage.label} is queued behind the current stage.`,
    status: index < activeIndex ? 'complete' : index === activeIndex ? 'active' : 'queued',
    updated_at: workspace.updated_at,
  }));
}

function makeDrafts(workspace: StudioWorkspace): StudioDraft[] {
  return [
    {
      id: `${workspace.id}-draft-1`,
      workspace_id: workspace.id,
      title: `${workspace.title} main cut`,
      format: workspace.output_format,
      stage: workspace.current_stage,
      status: workspace.approval_status === 'approved' ? 'approved' : 'review',
      owner: 'studio-director',
      summary: `Primary ${workspace.output_format.replace(/_/g, ' ')} draft aligned to the ${stageLabel(workspace.current_stage)} stage.`,
      assets_required: ['Hook line', 'B-roll selection', 'Caption pass'],
      updated_at: workspace.updated_at,
    },
    {
      id: `${workspace.id}-draft-2`,
      workspace_id: workspace.id,
      title: `${workspace.title} alternate hook`,
      format: workspace.output_format,
      stage: 'script',
      status: 'draft',
      owner: 'writer',
      summary: 'Backup angle focused on urgency and proof.',
      assets_required: ['Alternative opener', 'Supporting testimonial'],
      updated_at: isoAt(220),
    },
  ];
}

function makeJobs(workspace: StudioWorkspace): StudioJob[] {
  return [
    {
      id: `${workspace.id}-job-render`,
      workspace_id: workspace.id,
      label: 'Render v1 social cut',
      job_type: 'render',
      provider: 'Runway + ffmpeg',
      status: workspace.current_stage === 'render' ? 'running' : 'completed',
      progress: workspace.current_stage === 'render' ? 62 : 100,
      created_at: isoAt(180),
      updated_at: workspace.updated_at,
    },
    {
      id: `${workspace.id}-job-package`,
      workspace_id: workspace.id,
      label: 'Package captions + export bundle',
      job_type: 'packaging',
      provider: 'openfang-media',
      status: workspace.current_stage === 'approval' || workspace.current_stage === 'schedule' ? 'running' : 'queued',
      progress: workspace.current_stage === 'approval' || workspace.current_stage === 'schedule' ? 35 : 0,
      created_at: isoAt(95),
      updated_at: isoAt(40),
    },
  ];
}

function makeEvents(workspace: StudioWorkspace): StudioEvent[] {
  return [
    {
      id: `${workspace.id}-event-1`,
      workspace_id: workspace.id,
      kind: 'stage',
      title: 'Workspace advanced',
      message: `Current stage moved to ${stageLabel(workspace.current_stage)}.`,
      created_at: workspace.updated_at,
    },
    {
      id: `${workspace.id}-event-2`,
      workspace_id: workspace.id,
      kind: 'job',
      title: 'Render pipeline synced',
      message: 'Job state refreshed from the media pipeline.',
      created_at: isoAt(75),
    },
    {
      id: `${workspace.id}-event-3`,
      workspace_id: workspace.id,
      kind: 'approval',
      title: 'Approval lane updated',
      message: `Approval status is ${workspace.approval_status.replace(/_/g, ' ')}.`,
      created_at: isoAt(45),
    },
  ];
}

function makeApproval(workspace: StudioWorkspace): StudioApproval {
  return {
    id: `${workspace.id}-approval`,
    workspace_id: workspace.id,
    target_type: 'draft',
    target_id: workspace.active_draft_id ?? `${workspace.id}-draft-1`,
    status: workspace.approval_status,
    requested_by: 'studio-director',
    requested_at: isoAt(50),
    summary: 'Human review required before scheduling the latest exported cut.',
  };
}

export function getFallbackStudioIndex(): StudioIndexPayload {
  const workspaces = [
    makeWorkspace({
      id: 'studio-demo',
      title: 'Founder story launch',
      client_name: 'Northstar Labs',
      objective: 'Ship a founder-led launch cut for the waitlist push.',
      primary_channel: 'linkedin',
      output_format: 'vertical_video',
      status: 'active',
      current_stage: 'approval',
      approval_status: 'pending',
      summary: 'One active draft is waiting on review notes before it moves into scheduling.',
      updated_at: isoAt(18),
    }),
    makeWorkspace({
      id: 'studio-ugc',
      title: 'UGC testimonial sprint',
      client_name: 'Signal Forge',
      objective: 'Turn customer proof into a three-cut paid social package.',
      primary_channel: 'meta_ads',
      output_format: 'paid_social_bundle',
      status: 'queued',
      current_stage: 'render',
      approval_status: 'changes_requested',
      summary: 'Render jobs are active while the team waits on updated testimonial subtitles.',
      updated_at: isoAt(52),
    }),
  ];
  const jobs = workspaces.flatMap(makeJobs).sort((left, right) => right.updated_at.localeCompare(left.updated_at));
  return {
    workspaces,
    jobs,
    summary: {
      live_workspaces: workspaces.length,
      active_jobs: jobs.filter((job) => job.status === 'queued' || job.status === 'running').length,
      approval_backlog: workspaces.filter((workspace) => workspace.approval_status === 'pending').length,
    },
  };
}

export function getFallbackStudioWorkspace(workspaceId: string): StudioWorkspaceDetailPayload {
  const index = getFallbackStudioIndex();
  const matched = index.workspaces.find((workspace) => workspace.id === workspaceId);
  const workspace = matched ?? makeWorkspace({
    id: workspaceId,
    title: 'New studio workspace',
    client_name: 'OpenFang demo',
    objective: 'Stand up a new creator workflow and hand it off to approvals.',
    primary_channel: 'youtube_shorts',
    output_format: 'storyboard_pack',
    status: 'active',
    current_stage: 'brief',
    approval_status: 'pending',
    summary: 'Fresh workspace with no persisted backend state yet; using the local fallback contract.',
    updated_at: isoAt(8),
  });

  return {
    workspace,
    drafts: makeDrafts(workspace),
    stages: makeStages(workspace),
    jobs: makeJobs(workspace),
    events: makeEvents(workspace),
    approval: makeApproval(workspace),
  };
}

export function createFallbackWorkspace(input: Partial<CreateStudioWorkspaceInput>): StudioWorkspaceDetailPayload {
  const workspaceId = `studio-${Date.now()}`;
  const detail = getFallbackStudioWorkspace(workspaceId);
  return {
    ...detail,
    workspace: {
      ...detail.workspace,
      title: input.title?.trim() || 'Untitled workspace',
      client_name: input.client_name?.trim() || 'OpenFang client',
      objective: input.objective?.trim() || 'Create a production-ready creator brief.',
      primary_channel: input.primary_channel?.trim() || 'youtube_shorts',
      output_format: input.output_format?.trim() || 'vertical_video',
    },
  };
}

export function createFallbackDraft(workspaceId: string, input: Partial<StudioDraft>): StudioDraft {
  return {
    id: `draft-${Date.now()}`,
    workspace_id: workspaceId,
    title: input.title?.trim() || 'New draft',
    format: input.format?.trim() || 'vertical_video',
    stage: input.stage ?? 'script',
    status: input.status ?? 'draft',
    owner: input.owner?.trim() || 'studio-director',
    summary: input.summary?.trim() || 'Fallback draft created by the Next.js studio proxy.',
    assets_required: input.assets_required ?? ['Narration', 'Thumbnail frame'],
    updated_at: new Date().toISOString(),
  };
}

export function createFallbackJob(workspaceId: string, input: Partial<StudioJob>): StudioJob {
  return {
    id: `job-${Date.now()}`,
    workspace_id: workspaceId,
    label: input.label?.trim() || 'Queue studio job',
    job_type: input.job_type?.trim() || 'render',
    provider: input.provider?.trim() || 'openfang-media',
    status: input.status ?? 'queued',
    progress: input.progress ?? 0,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
}

export function createFallbackApproval(workspaceId: string, targetId: string, status: StudioApproval['status']): StudioApproval {
  return {
    ...makeApproval(getFallbackStudioWorkspace(workspaceId).workspace),
    target_id: targetId,
    status,
    requested_at: new Date().toISOString(),
  };
}
