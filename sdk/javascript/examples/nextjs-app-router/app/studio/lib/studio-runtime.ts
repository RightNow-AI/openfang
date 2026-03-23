import { randomUUID } from 'crypto';
import type {
  CreateStudioDraftInput,
  CreateStudioWorkspaceWizardInput,
  StudioArtifact,
  StudioArtifactMap,
  StudioDraftPagePayload,
  StudioDraftPipelineStage,
  StudioDraftRecord,
  StudioDraftRuntimeStatus,
  StudioPipelineEvent,
  StudioPolicyAlert,
  StudioScriptPayload,
  StudioWorkspaceDashboardPayload,
  StudioWorkspaceRecord,
  UpdateStudioDraftInput,
} from './studio-types';

type DraftListener = (event: StudioPipelineEvent) => void;

type RuntimeState = {
  workspaces: Map<string, StudioWorkspaceRecord>;
  drafts: Map<string, StudioDraftRecord>;
  workspaceDrafts: Map<string, string[]>;
  draftEvents: Map<string, StudioPipelineEvent[]>;
  listeners: Map<string, Set<DraftListener>>;
  alerts: Map<string, StudioPolicyAlert[]>;
};

const runtimeGlobal = globalThis as typeof globalThis & {
  __openfangStudioRuntime?: RuntimeState;
};

const VISUAL_STYLE_PALETTE: Record<string, { accent: string; surface: string; ink: string }> = {
  documentary: { accent: '#7c5c2e', surface: '#efe2c3', ink: '#1f2937' },
  cinematic: { accent: '#0f172a', surface: '#dbe4f5', ink: '#f8fafc' },
  anime: { accent: '#ef4444', surface: '#fde2f3', ink: '#1f2937' },
  '3d_animation': { accent: '#0f766e', surface: '#d6f5ef', ink: '#0f172a' },
};

function nowIso() {
  return new Date().toISOString();
}

function slug(input: string) {
  return input
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 48);
}

function dataUriSvg(title: string, subtitle: string, accent: string, surface: string, ink: string) {
  const svg = `
    <svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720" viewBox="0 0 1280 720" fill="none">
      <rect width="1280" height="720" rx="48" fill="${surface}"/>
      <rect x="38" y="38" width="1204" height="644" rx="32" fill="${accent}" opacity="0.14"/>
      <rect x="96" y="96" width="430" height="528" rx="26" fill="${accent}" opacity="0.18"/>
      <circle cx="310" cy="242" r="96" fill="${accent}" opacity="0.22"/>
      <path d="M222 426C266 354 378 354 422 426C466 498 432 586 322 586C212 586 178 498 222 426Z" fill="${accent}" opacity="0.32"/>
      <text x="602" y="210" fill="${ink}" font-family="Inter, Arial, sans-serif" font-size="36" font-weight="700">${title}</text>
      <text x="602" y="276" fill="${ink}" font-family="Inter, Arial, sans-serif" font-size="22" opacity="0.74">${subtitle}</text>
      <rect x="602" y="334" width="426" height="16" rx="8" fill="${accent}" opacity="0.26"/>
      <rect x="602" y="376" width="364" height="16" rx="8" fill="${accent}" opacity="0.16"/>
      <rect x="602" y="418" width="404" height="16" rx="8" fill="${accent}" opacity="0.16"/>
      <rect x="602" y="520" width="196" height="60" rx="18" fill="${accent}"/>
      <text x="632" y="558" fill="#ffffff" font-family="Inter, Arial, sans-serif" font-size="24" font-weight="700">AI Scene</text>
    </svg>
  `;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function makeScript(topic: string): StudioScriptPayload {
  const clean = topic.trim() || 'How to grow a faceless channel';
  return {
    hook: `How ${clean.toLowerCase()} scales faster than people expect`,
    body: `Start with the market reality, not motivation. Break the topic into four concrete scenes, one proof point, one cost constraint, and one repeatable publishing move. Keep every line visual enough that the generator can turn it into footage without needing a human-written prompt.`,
    cta: 'Follow for the next teardown and save this workflow for your team.',
    wordCount: 122,
  };
}

function makeResearchArtifact(topic: string, playbook: string, maxCandidates: number): StudioArtifact {
  const clean = topic.trim() || 'How to grow a faceless channel';
  const labels = [
    'Angle one',
    'Angle two',
    'Angle three',
    'Angle four',
  ];
  const candidates = Array.from({ length: Math.max(1, maxCandidates) }, (_, index) => ({
    title: `${labels[index] ?? `Angle ${index + 1}`}: ${clean}`,
    rationale: `Position the short around a concrete tension point so the hook earns the swipe-stop in the first two seconds.`,
    hook: index === 0
      ? `Everyone says ${clean.toLowerCase()} is easy. The numbers say the opposite.`
      : index === 1
        ? `The hidden system behind ${clean.toLowerCase()} is what actually changes the result.`
        : `${clean} looks simple until you see the production constraint nobody talks about.`,
  }));

  return {
    id: `artifact-research-${randomUUID()}`,
    artifactType: 'ResearchPack',
    label: 'Research pack',
    json: {
      playbook,
      candidates,
      selectedAngle: null,
    },
    createdAt: nowIso(),
  };
}

function makeScriptArtifact(topic: string, options?: { tone?: string; targetDurationSec?: number; includeCta?: boolean; selectedAngle?: string | null }): StudioArtifact {
  const tone = options?.tone ?? 'curious';
  const selectedAngle = options?.selectedAngle?.trim();
  const targetDurationSec = options?.targetDurationSec ?? 45;
  const includeCta = options?.includeCta ?? true;
  const script = makeScript(selectedAngle || topic);
  return {
    id: `artifact-script-${randomUUID()}`,
    artifactType: 'ScriptVersion',
    label: 'Approved script',
    json: {
      ...script,
      body: `${script.body} Tone: ${tone}. Target runtime: ${targetDurationSec} seconds.`,
      cta: includeCta ? script.cta : '',
      wordCount: Math.max(95, Math.round((targetDurationSec / 45) * script.wordCount)),
      tone,
      selectedAngle: selectedAngle || topic,
    },
    createdAt: nowIso(),
  };
}

function makeVisualArtifacts(topic: string, style: string, count: number): StudioArtifact[] {
  const palette = VISUAL_STYLE_PALETTE[style] ?? VISUAL_STYLE_PALETTE.documentary;
  return Array.from({ length: count }, (_, index) => ({
    id: `artifact-visual-${randomUUID()}`,
    artifactType: index % 2 === 0 ? 'ImageAsset' : 'ScenePlan',
    label: `Scene ${index + 1}`,
    url: dataUriSvg(`Scene ${index + 1}`, `${topic} · ${style.replace(/_/g, ' ')}`, palette.accent, palette.surface, palette.ink),
    posterUrl: dataUriSvg(`Preview ${index + 1}`, 'Rendered placeholder while Rust media hooks are offline', palette.accent, palette.surface, palette.ink),
    json: {
      sceneNumber: index + 1,
      prompt: `${style} visual for ${topic}`,
      style,
    },
    createdAt: nowIso(),
  }));
}

function makePreviewArtifact(topic: string, kind: 'PreviewRender' | 'FinalRender'): StudioArtifact {
  return {
    id: `artifact-render-${randomUUID()}`,
    artifactType: kind,
    label: kind === 'FinalRender' ? 'Final vertical render' : 'Preview vertical render',
    posterUrl: dataUriSvg(kind === 'FinalRender' ? 'Final render ready' : 'Preview render ready', `${topic} · 9:16 export`, '#111827', '#e5e7eb', '#111827'),
    json: {
      aspectRatio: '9:16',
      renderMode: kind === 'FinalRender' ? 'final' : 'preview',
    },
    createdAt: nowIso(),
  };
}

function createDraftRecord(workspace: StudioWorkspaceRecord, topic: string, seedScript = false, input?: Partial<CreateStudioDraftInput>): StudioDraftRecord {
  const createdAt = nowIso();
  const playbook = input?.playbook?.trim() || 'educational_shorts';
  const format = input?.format?.trim() || 'faceless_short';
  const targetDurationSec = input?.targetDurationSec ?? 45;
  const script = seedScript ? makeScriptArtifact(topic, { targetDurationSec }) : null;
  return {
    id: `draft_${randomUUID().replace(/-/g, '').slice(0, 14)}`,
    workspaceId: workspace.id,
    workspaceName: workspace.name,
    topic,
    playbook,
    format,
    targetDurationSec,
    stage: script ? 'script' : 'research',
    status: script ? 'AwaitingApproval' : 'Draft',
    createdAt,
    updatedAt: createdAt,
    artifacts: {
      research: null,
      script,
      visuals: [],
      voice: null,
      previewRender: null,
      finalRender: null,
    },
    failureMessage: null,
  };
}

function createWorkspaceRecord(input: CreateStudioWorkspaceWizardInput): StudioWorkspaceRecord {
  const timestamp = nowIso();
  return {
    id: `ws_${randomUUID().replace(/-/g, '').slice(0, 14)}`,
    name: input.name.trim(),
    niche: input.niche.trim(),
    platform: input.platform,
    language: input.language?.trim() || 'en',
    publishGoalPerDay: input.publishGoalPerDay,
    createdAt: timestamp,
    updatedAt: timestamp,
    stats: {
      publishedLast7Days: 0,
      scheduledCount: 0,
    },
  };
}

function emitEvent(state: RuntimeState, draftId: string, event: StudioPipelineEvent) {
  const next = [...(state.draftEvents.get(draftId) ?? []), event];
  state.draftEvents.set(draftId, next);
  for (const listener of state.listeners.get(draftId) ?? []) {
    listener(event);
  }
}

function patchDraftTimestamp(state: RuntimeState, draftId: string) {
  const draft = state.drafts.get(draftId);
  if (!draft) return;
  const updatedAt = nowIso();
  draft.updatedAt = updatedAt;
  const workspace = state.workspaces.get(draft.workspaceId);
  if (workspace) {
    workspace.updatedAt = updatedAt;
  }
}

function buildSeedState(): RuntimeState {
  const workspaces = new Map<string, StudioWorkspaceRecord>();
  const drafts = new Map<string, StudioDraftRecord>();
  const workspaceDrafts = new Map<string, string[]>();
  const draftEvents = new Map<string, StudioPipelineEvent[]>();
  const listeners = new Map<string, Set<DraftListener>>();
  const alerts = new Map<string, StudioPolicyAlert[]>();

  const workspaceA = createWorkspaceRecord({
    name: 'Legends League Shorts',
    platform: 'youtube',
    niche: 'faceless history and business explainers',
    language: 'en',
    publishGoalPerDay: 2,
  });
  workspaceA.stats = { publishedLast7Days: 6, scheduledCount: 2 };

  const workspaceB = createWorkspaceRecord({
    name: 'Arc Golf Stories',
    platform: 'youtube',
    niche: 'golf coaching, swing breakdowns, and premium apparel',
    language: 'en',
    publishGoalPerDay: 1,
  });
  workspaceB.stats = { publishedLast7Days: 3, scheduledCount: 1 };

  for (const workspace of [workspaceA, workspaceB]) {
    workspaces.set(workspace.id, workspace);
    workspaceDrafts.set(workspace.id, []);
    alerts.set(workspace.id, []);
  }

  const draftApproval = createDraftRecord(workspaceA, 'How to grow a faceless YouTube channel to 50k with AI tools', true);
  draftApproval.stage = 'script';
  draftApproval.status = 'AwaitingApproval';

  const draftRunning = createDraftRecord(workspaceA, 'The brutal production math behind daily Shorts output', true);
  draftRunning.stage = 'visuals';
  draftRunning.status = 'Running';
  draftRunning.artifacts.voice = {
    id: `artifact-voice-${randomUUID()}`,
    artifactType: 'VoiceTrack',
    label: 'Narration ready',
    url: null,
    json: { voiceId: 'adam', speed: 1.05 },
    createdAt: nowIso(),
  };

  const draftBacklog = createDraftRecord(workspaceB, 'Why premium golf brands win on systems, not just style', false);
  draftBacklog.stage = 'research';
  draftBacklog.status = 'Draft';

  const draftFailed = createDraftRecord(workspaceB, 'Three hooks that make golf coaching Shorts stop the scroll', true);
  draftFailed.stage = 'visuals';
  draftFailed.status = 'Failed';
  draftFailed.failureMessage = 'Gemini image generation was throttled and needs a retry.';

  const seeded = [draftApproval, draftRunning, draftBacklog, draftFailed];
  for (const draft of seeded) {
    drafts.set(draft.id, draft);
    workspaceDrafts.set(draft.workspaceId, [...(workspaceDrafts.get(draft.workspaceId) ?? []), draft.id]);
    draftEvents.set(draft.id, []);
  }

  alerts.set(workspaceB.id, [
    {
      id: `alert-${workspaceB.id}`,
      workspaceId: workspaceB.id,
      severity: 'danger',
      draftId: draftFailed.id,
      message: 'One visuals job failed and needs a rerun before the channel can publish on time.',
    },
  ]);

  return { workspaces, drafts, workspaceDrafts, draftEvents, listeners, alerts };
}

function getState() {
  if (!runtimeGlobal.__openfangStudioRuntime) {
    runtimeGlobal.__openfangStudioRuntime = buildSeedState();
  }
  return runtimeGlobal.__openfangStudioRuntime;
}

function cloneDraft(draft: StudioDraftRecord): StudioDraftRecord {
  return JSON.parse(JSON.stringify(draft)) as StudioDraftRecord;
}

function cloneWorkspace(workspace: StudioWorkspaceRecord): StudioWorkspaceRecord {
  return JSON.parse(JSON.stringify(workspace)) as StudioWorkspaceRecord;
}

function nextStageAfter(stage: StudioDraftPipelineStage): StudioDraftPipelineStage {
  switch (stage) {
    case 'research':
      return 'script';
    case 'script':
      return 'voice';
    case 'voice':
      return 'visuals';
    case 'visuals':
      return 'edit';
    case 'edit':
      return 'publish';
    case 'publish':
    default:
      return 'publish';
  }
}

function updateWorkspaceStats(state: RuntimeState, workspaceId: string) {
  const workspace = state.workspaces.get(workspaceId);
  if (!workspace) return;
  const drafts = listWorkspaceDrafts(workspaceId);
  workspace.stats = {
    publishedLast7Days: drafts.filter((draft) => draft.status === 'Published').length,
    scheduledCount: drafts.filter((draft) => draft.status === 'Queued').length,
  };
}

export function listWorkspaceDrafts(workspaceId: string): StudioDraftRecord[] {
  const state = getState();
  return (state.workspaceDrafts.get(workspaceId) ?? [])
    .map((draftId) => state.drafts.get(draftId))
    .filter((draft): draft is StudioDraftRecord => Boolean(draft))
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
    .map(cloneDraft);
}

export function listWorkspaceAlerts(workspaceId: string): StudioPolicyAlert[] {
  const state = getState();
  return JSON.parse(JSON.stringify(state.alerts.get(workspaceId) ?? [])) as StudioPolicyAlert[];
}

export function createStudioWorkspace(input: CreateStudioWorkspaceWizardInput): StudioWorkspaceRecord {
  const state = getState();
  const workspace = createWorkspaceRecord(input);
  state.workspaces.set(workspace.id, workspace);
  state.workspaceDrafts.set(workspace.id, []);
  state.alerts.set(workspace.id, []);
  return cloneWorkspace(workspace);
}

export function createStudioDraft(workspaceId: string, input: CreateStudioDraftInput): StudioDraftRecord | null {
  const state = getState();
  const workspace = state.workspaces.get(workspaceId);
  if (!workspace) return null;
  const draft = createDraftRecord(workspace, input.topic.trim() || 'Untitled short', false, input);
  state.drafts.set(draft.id, draft);
  state.workspaceDrafts.set(workspaceId, [draft.id, ...(state.workspaceDrafts.get(workspaceId) ?? [])]);
  state.draftEvents.set(draft.id, []);
  patchDraftTimestamp(state, draft.id);
  return cloneDraft(draft);
}

export function getStudioWorkspaceDashboard(workspaceId: string): StudioWorkspaceDashboardPayload | null {
  const state = getState();
  const workspace = state.workspaces.get(workspaceId);
  if (!workspace) return null;
  updateWorkspaceStats(state, workspaceId);
  return {
    workspace: cloneWorkspace(workspace),
    drafts: listWorkspaceDrafts(workspaceId),
    alerts: listWorkspaceAlerts(workspaceId),
  };
}

export function getStudioDraft(draftId: string): StudioDraftPagePayload | null {
  const state = getState();
  const draft = state.drafts.get(draftId);
  if (!draft) return null;
  const workspace = state.workspaces.get(draft.workspaceId);
  if (!workspace) return null;
  updateWorkspaceStats(state, draft.workspaceId);
  return {
    draft: cloneDraft(draft),
    workspace: cloneWorkspace(workspace),
  };
}

export function updateStudioDraft(draftId: string, input: UpdateStudioDraftInput): StudioDraftRecord | null {
  const state = getState();
  const draft = state.drafts.get(draftId);
  if (!draft) return null;

  if (typeof input.topic === 'string' && input.topic.trim()) {
    draft.topic = input.topic.trim();
  }
  if (input.stage) {
    draft.stage = input.stage;
  }
  if (input.status) {
    draft.status = input.status;
  }
  if (input.failureMessage !== undefined) {
    draft.failureMessage = input.failureMessage;
  }
  if (input.artifacts) {
    draft.artifacts = {
      ...draft.artifacts,
      ...input.artifacts,
      visuals: input.artifacts.visuals ?? draft.artifacts.visuals,
    };
  }
  patchDraftTimestamp(state, draftId);
  updateWorkspaceStats(state, draft.workspaceId);
  return cloneDraft(draft);
}

export function getDraftEventBuffer(draftId: string): StudioPipelineEvent[] {
  const state = getState();
  return JSON.parse(JSON.stringify(state.draftEvents.get(draftId) ?? [])) as StudioPipelineEvent[];
}

export function subscribeDraftEvents(draftId: string, listener: DraftListener) {
  const state = getState();
  const listeners = state.listeners.get(draftId) ?? new Set<DraftListener>();
  listeners.add(listener);
  state.listeners.set(draftId, listeners);
  return () => {
    const current = state.listeners.get(draftId);
    if (!current) return;
    current.delete(listener);
    if (current.size === 0) {
      state.listeners.delete(draftId);
    }
  };
}

function resetFailureAlert(state: RuntimeState, draft: StudioDraftRecord) {
  state.alerts.set(
    draft.workspaceId,
    (state.alerts.get(draft.workspaceId) ?? []).filter((alert) => alert.draftId !== draft.id),
  );
}

function registerFailureAlert(state: RuntimeState, draft: StudioDraftRecord, message: string) {
  const nextAlerts = (state.alerts.get(draft.workspaceId) ?? []).filter((alert) => alert.draftId !== draft.id);
  nextAlerts.unshift({
    id: `alert-${draft.id}`,
    workspaceId: draft.workspaceId,
    draftId: draft.id,
    severity: 'danger',
    message,
  });
  state.alerts.set(draft.workspaceId, nextAlerts);
}

function scheduleProgress(state: RuntimeState, draft: StudioDraftRecord, jobId: string, stage: StudioDraftPipelineStage, completion: () => void) {
  const checkpoints = [18, 44, 72, 100];
  checkpoints.forEach((progress, index) => {
    setTimeout(() => {
      if (!state.drafts.has(draft.id)) return;
      emitEvent(state, draft.id, {
        type: 'job.progress',
        draftId: draft.id,
        jobId,
        stage,
        progress,
        timestamp: nowIso(),
      });
      if (progress === 100) {
        completion();
      }
    }, 450 * (index + 1));
  });
}

type JobCompletionResult = {
  artifacts?: StudioArtifact | StudioArtifact[] | null;
  nextStage?: StudioDraftPipelineStage;
  nextStatus?: StudioDraftRuntimeStatus;
};

function startJob(
  draftId: string,
  stage: StudioDraftPipelineStage,
  completion: (draft: StudioDraftRecord) => JobCompletionResult,
) {
  const state = getState();
  const draft = state.drafts.get(draftId);
  if (!draft) return null;
  const jobId = `job_${slug(stage)}_${randomUUID().slice(0, 8)}`;
  draft.stage = stage;
  draft.status = 'Running';
  draft.failureMessage = null;
  patchDraftTimestamp(state, draftId);
  resetFailureAlert(state, draft);
  emitEvent(state, draftId, {
    type: 'job.started',
    draftId,
    jobId,
    stage,
    progress: 0,
    timestamp: nowIso(),
  });

  scheduleProgress(state, draft, jobId, stage, () => {
    const activeDraft = state.drafts.get(draftId);
    if (!activeDraft) return;
    const result = completion(activeDraft);
    const produced = result.artifacts ?? null;
    const artifacts = Array.isArray(produced) ? produced : produced ? [produced] : [];
    for (const artifact of artifacts) {
      emitEvent(state, draftId, {
        type: 'artifact.created',
        draftId,
        jobId,
        stage,
        artifactType: artifact.artifactType,
        artifact,
        count: artifacts.length,
        timestamp: nowIso(),
      });
    }
    activeDraft.status = result.nextStatus ?? (stage === 'publish' ? 'Queued' : 'Ready');
    activeDraft.stage = result.nextStage ?? nextStageAfter(stage);
    patchDraftTimestamp(state, draftId);
    updateWorkspaceStats(state, activeDraft.workspaceId);
    emitEvent(state, draftId, {
      type: 'draft.stage_changed',
      draftId,
      jobId,
      stage: activeDraft.stage,
      status: activeDraft.status,
      progress: 100,
      timestamp: nowIso(),
    });
  });
  return jobId;
}

export function ensureScriptArtifact(draftId: string) {
  const state = getState();
  const draft = state.drafts.get(draftId);
  if (!draft) return null;
  if (!draft.artifacts.script) {
    const selectedAngle = (draft.artifacts.research?.json as Record<string, unknown> | undefined)?.selectedAngle;
    draft.artifacts.script = makeScriptArtifact(draft.topic, {
      targetDurationSec: draft.targetDurationSec,
      selectedAngle: typeof selectedAngle === 'string' ? selectedAngle : null,
    });
    draft.stage = 'script';
    draft.status = 'AwaitingApproval';
    patchDraftTimestamp(state, draftId);
  }
  return cloneDraft(draft);
}

export function runResearchGeneration(draftId: string, payload: { sources?: string[]; maxCandidates?: number }) {
  return startJob(draftId, 'research', (draft) => {
    const artifact = makeResearchArtifact(draft.topic, draft.playbook, payload.maxCandidates ?? 3);
    draft.artifacts.research = artifact;
    return {
      artifacts: artifact,
      nextStage: 'research',
      nextStatus: 'AwaitingApproval',
    };
  });
}

export function runScriptGeneration(draftId: string, payload: { tone?: string; targetDurationSec?: number; includeCta?: boolean }) {
  return startJob(draftId, 'script', (draft) => {
    const researchJson = draft.artifacts.research?.json as Record<string, unknown> | undefined;
    const selectedAngle = typeof researchJson?.selectedAngle === 'string' ? researchJson.selectedAngle : null;
    const artifact = makeScriptArtifact(draft.topic, {
      tone: payload.tone,
      targetDurationSec: payload.targetDurationSec ?? draft.targetDurationSec,
      includeCta: payload.includeCta,
      selectedAngle,
    });
    draft.artifacts.script = artifact;
    draft.targetDurationSec = payload.targetDurationSec ?? draft.targetDurationSec;
    return {
      artifacts: artifact,
      nextStage: 'script',
      nextStatus: 'AwaitingApproval',
    };
  });
}

export function approveStudioDraftStage(
  draftId: string,
  payload: { stage?: string; notes?: string; selectedAngleIndex?: number | null },
) {
  const state = getState();
  const draft = state.drafts.get(draftId);
  if (!draft) return null;

  const stage = (payload.stage ?? draft.stage) as StudioDraftPipelineStage;

  if (stage === 'research' && draft.artifacts.research?.json && typeof payload.selectedAngleIndex === 'number') {
    const researchJson = draft.artifacts.research.json as Record<string, unknown>;
    const candidates = Array.isArray(researchJson.candidates) ? researchJson.candidates : [];
    const selectedCandidate = candidates[payload.selectedAngleIndex] as Record<string, unknown> | undefined;
    if (selectedCandidate && typeof selectedCandidate.title === 'string') {
      draft.artifacts.research.json = {
        ...researchJson,
        selectedAngle: selectedCandidate.title,
        selectedAngleIndex: payload.selectedAngleIndex,
        approvalNotes: payload.notes ?? null,
      };
    }
    draft.stage = 'script';
    draft.status = 'Draft';
  } else if (stage === 'script') {
    draft.stage = 'voice';
    draft.status = 'Draft';
  } else if (stage === 'voice') {
    draft.stage = 'visuals';
    draft.status = 'Draft';
  } else if (stage === 'visuals') {
    draft.stage = 'edit';
    draft.status = 'Draft';
  } else if (stage === 'edit') {
    draft.stage = 'publish';
    draft.status = 'Draft';
  } else {
    draft.status = 'Published';
    draft.stage = 'publish';
  }

  patchDraftTimestamp(state, draftId);
  updateWorkspaceStats(state, draft.workspaceId);
  emitEvent(state, draftId, {
    type: 'draft.stage_changed',
    draftId,
    stage: draft.stage,
    status: draft.status,
    timestamp: nowIso(),
  });
  return cloneDraft(draft);
}

export function runVoiceGeneration(draftId: string, payload: { voiceId?: string; speed?: number }) {
  return startJob(draftId, 'voice', (draft) => {
    const artifact: StudioArtifact = {
      id: `artifact-voice-${randomUUID()}`,
      artifactType: 'VoiceTrack',
      label: 'Narration ready',
      url: null,
      json: {
        voiceId: payload.voiceId ?? 'adam',
        speed: payload.speed ?? 1.05,
      },
      createdAt: nowIso(),
    };
    draft.artifacts.voice = artifact;
    return { artifacts: artifact };
  });
}

export function runVisualGeneration(draftId: string, payload: { style?: string; sceneCount?: number }) {
  return startJob(draftId, 'visuals', (draft) => {
    const visuals = makeVisualArtifacts(draft.topic, payload.style ?? 'documentary', payload.sceneCount ?? 4);
    draft.artifacts.visuals = visuals;
    return { artifacts: visuals };
  });
}

export function runEditGeneration(draftId: string, payload: { renderMode?: 'preview' | 'final'; burnSubtitles?: boolean }) {
  return startJob(draftId, 'edit', (draft) => {
    const artifact = makePreviewArtifact(draft.topic, payload.renderMode === 'final' ? 'FinalRender' : 'PreviewRender');
    if (artifact.artifactType === 'FinalRender') {
      draft.artifacts.finalRender = artifact;
    } else {
      draft.artifacts.previewRender = artifact;
    }
    return { artifacts: artifact };
  });
}

export function runPublishGeneration(draftId: string, payload: { mode?: string }) {
  return startJob(draftId, 'publish', (draft) => {
    if (!draft.artifacts.finalRender && draft.artifacts.previewRender) {
      draft.artifacts.finalRender = {
        ...draft.artifacts.previewRender,
        id: `artifact-final-${randomUUID()}`,
        artifactType: 'FinalRender',
        label: payload.mode === 'schedule' ? 'Scheduled final render' : 'Publish-ready final render',
      };
    }
    return {
      artifacts: draft.artifacts.finalRender ?? draft.artifacts.previewRender ?? null,
      nextStage: 'publish',
      nextStatus: payload.mode === 'schedule' ? 'Queued' : 'Published',
    };
  });
}

export function failDraftJob(draftId: string, message: string) {
  const state = getState();
  const draft = state.drafts.get(draftId);
  if (!draft) return null;
  draft.status = 'Failed';
  draft.failureMessage = message;
  patchDraftTimestamp(state, draftId);
  registerFailureAlert(state, draft, message);
  emitEvent(state, draftId, {
    type: 'job.failed',
    draftId,
    stage: draft.stage,
    error: message,
    status: 'Failed',
    timestamp: nowIso(),
  });
  return cloneDraft(draft);
}