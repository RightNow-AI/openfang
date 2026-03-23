import { NextRequest, NextResponse } from 'next/server';
import {
  createFallbackApproval,
  createFallbackDraft,
  createFallbackJob,
  createFallbackWorkspace,
  getFallbackStudioIndex,
  getFallbackStudioWorkspace,
} from '../../../studio/lib/studio-fixtures';
import {
  approveStudioDraftStage,
  createStudioDraft,
  createStudioWorkspace,
  ensureScriptArtifact,
  getDraftEventBuffer,
  getStudioDraft,
  getStudioWorkspaceDashboard,
  runResearchGeneration,
  runScriptGeneration,
  runEditGeneration,
  runPublishGeneration,
  runVisualGeneration,
  runVoiceGeneration,
  subscribeDraftEvents,
  updateStudioDraft,
} from '../../../studio/lib/studio-runtime';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

export async function proxyJson(path: string, init?: RequestInit) {
  const response = await fetch(`${BASE}${path}`, {
    cache: 'no-store',
    ...init,
  });
  const text = await response.text();
  return new NextResponse(text || '{}', {
    status: response.status,
    headers: { 'Content-Type': 'application/json' },
  });
}

export async function readJson(request: NextRequest) {
  return request.json().catch(() => ({}));
}

export function fallbackIndexResponse() {
  return NextResponse.json(getFallbackStudioIndex());
}

export function fallbackWorkspaceResponse(workspaceId: string) {
  return NextResponse.json(getFallbackStudioWorkspace(workspaceId));
}

export function fallbackCreateWorkspaceResponse(body: Record<string, unknown>) {
  const name = String(body.name ?? body.title ?? '').trim() || 'New channel workspace';
  const workspace = createStudioWorkspace({
    name,
    niche: String(body.niche ?? body.objective ?? '').trim() || 'faceless shorts',
    platform: String(body.platform ?? body.primary_channel ?? 'youtube').includes('tiktok') ? 'tiktok' : 'youtube',
    language: String(body.language ?? 'en').trim() || 'en',
    publishGoalPerDay: Number(body.publishGoalPerDay ?? 1) || 1,
  });
  return NextResponse.json({ workspace }, { status: 201 });
}

export function fallbackDraftListResponse(workspaceId: string) {
  const payload = getStudioWorkspaceDashboard(workspaceId);
  return NextResponse.json({ drafts: payload?.drafts ?? getFallbackStudioWorkspace(workspaceId).drafts });
}

export function fallbackDraftCreateResponse(workspaceId: string, body: Record<string, unknown>) {
  const topic = String(body.topic ?? body.title ?? '').trim() || 'Untitled short';
  const draft = createStudioDraft(workspaceId, { topic }) ?? createFallbackDraft(workspaceId, body);
  return NextResponse.json({ draft }, { status: 201 });
}

export function fallbackStageListResponse(workspaceId: string) {
  return NextResponse.json({ stages: getFallbackStudioWorkspace(workspaceId).stages });
}

export function fallbackStagePatchResponse(workspaceId: string, body: Record<string, unknown>) {
  const detail = getFallbackStudioWorkspace(workspaceId);
  const stageKey = typeof body.key === 'string' ? body.key : detail.workspace.current_stage;
  return NextResponse.json({
    stages: detail.stages.map((stage) =>
      stage.key === stageKey
        ? {
            ...stage,
            status: typeof body.status === 'string' ? body.status : stage.status,
            notes: typeof body.notes === 'string' ? body.notes : stage.notes,
            updated_at: new Date().toISOString(),
          }
        : stage,
    ),
  });
}

export function fallbackJobsResponse(workspaceId?: string | null) {
  const index = getFallbackStudioIndex();
  const jobs = workspaceId ? index.jobs.filter((job) => job.workspace_id === workspaceId) : index.jobs;
  return NextResponse.json({ jobs });
}

export function fallbackCreateJobResponse(body: Record<string, unknown>) {
  const workspaceId = typeof body.workspace_id === 'string' ? body.workspace_id : 'studio-demo';
  return NextResponse.json({ job: createFallbackJob(workspaceId, body) }, { status: 201 });
}

export function fallbackJobResponse(jobId: string) {
  const job = getFallbackStudioIndex().jobs.find((item) => item.id === jobId) ?? createFallbackJob('studio-demo', { id: jobId, label: 'Fallback job' });
  return NextResponse.json({ job });
}

export function fallbackEventsResponse(workspaceId?: string | null) {
  const detail = getFallbackStudioWorkspace(workspaceId || 'studio-demo');
  return NextResponse.json({ events: detail.events });
}

export function fallbackApprovalResponse(body: Record<string, unknown>) {
  const workspaceId = typeof body.workspace_id === 'string' ? body.workspace_id : 'studio-demo';
  const targetId = typeof body.target_id === 'string' ? body.target_id : `${workspaceId}-draft-1`;
  const status = body.status === 'approved' || body.status === 'changes_requested' ? body.status : 'pending';
  return NextResponse.json({ approval: createFallbackApproval(workspaceId, targetId, status) });
}

export function fallbackDraftResponse(draftId: string) {
  const payload = getStudioDraft(draftId);
  if (!payload) {
    return NextResponse.json({ error: 'Draft not found' }, { status: 404 });
  }
  return NextResponse.json(payload.draft);
}

export function fallbackDraftPatchResponse(draftId: string, body: Record<string, unknown>) {
  const nextArtifacts = body.approveScript ? { script: ensureScriptArtifact(draftId)?.artifacts.script ?? null } : undefined;
  const draft = updateStudioDraft(draftId, {
    stage: typeof body.stage === 'string' ? body.stage as never : undefined,
    status: typeof body.status === 'string' ? body.status as never : undefined,
    topic: typeof body.topic === 'string' ? body.topic : undefined,
    failureMessage: typeof body.failureMessage === 'string' ? body.failureMessage : undefined,
    artifacts: nextArtifacts,
  });
  if (!draft) {
    return NextResponse.json({ error: 'Draft not found' }, { status: 404 });
  }
  return NextResponse.json({ draft });
}

export function fallbackDraftEventsStream(draftId: string, signal?: AbortSignal) {
  const existing = getStudioDraft(draftId);
  if (!existing) {
    return new Response(`event: error\ndata: ${JSON.stringify({ type: 'error', message: 'Draft not found' })}\n\n`, {
      status: 404,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache, no-transform',
        Connection: 'keep-alive',
        'X-Accel-Buffering': 'no',
      },
    });
  }

  const encoder = new TextEncoder();
  const headers = {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache, no-transform',
    Connection: 'keep-alive',
    'X-Accel-Buffering': 'no',
  };

  const stream = new ReadableStream({
    start(controller) {
      const push = (payload: unknown) => controller.enqueue(encoder.encode(`data: ${JSON.stringify(payload)}\n\n`));
      for (const event of getDraftEventBuffer(draftId)) {
        push(event);
      }

      const unsubscribe = subscribeDraftEvents(draftId, (event) => {
        push(event);
      });

      const heartbeat = setInterval(() => {
        controller.enqueue(encoder.encode(': heartbeat\n\n'));
      }, 20000);

      const close = () => {
        clearInterval(heartbeat);
        unsubscribe();
        try {
          controller.close();
        } catch {}
      };

      signal?.addEventListener('abort', close);
    },
  });

  return new Response(stream, { headers });
}

export function fallbackDraftApproveResponse(draftId: string, body: Record<string, unknown>) {
  const selectedAngleIndex = typeof body.selectedAngleIndex === 'number'
    ? body.selectedAngleIndex
    : typeof body.selectionIndex === 'number'
      ? body.selectionIndex
      : null;

  const draft = approveStudioDraftStage(draftId, {
    stage: typeof body.stage === 'string' ? body.stage : undefined,
    notes: typeof body.notes === 'string' ? body.notes : undefined,
    selectedAngleIndex,
  });

  if (!draft) {
    return NextResponse.json({ error: 'Draft not found' }, { status: 404 });
  }

  return NextResponse.json({ draft });
}

export function fallbackDraftRunResponse(draftId: string, action: 'research' | 'script' | 'voice' | 'visuals' | 'edit' | 'publish', body: Record<string, unknown>) {
  let jobId: string | null = null;
  if (action === 'research') {
    jobId = runResearchGeneration(draftId, {
      sources: Array.isArray(body.sources) ? body.sources.filter((value): value is string => typeof value === 'string') : undefined,
      maxCandidates: typeof body.maxCandidates === 'number' ? body.maxCandidates : undefined,
    });
  }
  if (action === 'script') {
    jobId = runScriptGeneration(draftId, {
      tone: typeof body.tone === 'string' ? body.tone : undefined,
      targetDurationSec: typeof body.targetDurationSec === 'number' ? body.targetDurationSec : undefined,
      includeCta: typeof body.includeCta === 'boolean' ? body.includeCta : undefined,
    });
  }
  if (action === 'voice') {
    jobId = runVoiceGeneration(draftId, {
      voiceId: typeof body.voiceId === 'string' ? body.voiceId : undefined,
      speed: typeof body.speed === 'number' ? body.speed : undefined,
    });
  }
  if (action === 'visuals') {
    jobId = runVisualGeneration(draftId, {
      style: typeof body.style === 'string' ? body.style : undefined,
      sceneCount: typeof body.sceneCount === 'number' ? body.sceneCount : undefined,
    });
  }
  if (action === 'edit') {
    jobId = runEditGeneration(draftId, {
      renderMode: body.renderMode === 'final' ? 'final' : 'preview',
      burnSubtitles: Boolean(body.burnSubtitles),
    });
  }
  if (action === 'publish') {
    jobId = runPublishGeneration(draftId, {
      mode: typeof body.mode === 'string' ? body.mode : 'immediate',
    });
  }

  if (!jobId) {
    return NextResponse.json({ error: 'Draft not found' }, { status: 404 });
  }

  return NextResponse.json({ ok: true, jobId }, { status: 202 });
}
