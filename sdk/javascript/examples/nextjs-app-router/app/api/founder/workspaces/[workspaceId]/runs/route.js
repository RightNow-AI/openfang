import { NextResponse } from 'next/server';
import { runStore } from '../../../../../../lib/run-store';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function normalizeFounderRunBody(body, workspaceId) {
  return {
    runId: String(body?.runId ?? '').trim(),
    workspaceId,
    playbookId: String(body?.playbookId ?? '').trim() || null,
    prompt: String(body?.prompt ?? '').trim(),
    status: String(body?.status ?? '').trim() || 'completed',
    summary: String(body?.summary ?? '').trim(),
    citations: Array.isArray(body?.citations) ? body.citations : [],
    nextActions: Array.isArray(body?.nextActions) ? body.nextActions : [],
  };
}

export async function GET(request, { params }) {
  const { workspaceId } = await params;

  try {
    const workspace = await runStore.getFounderWorkspace(workspaceId);
    if (!workspace) {
      return NextResponse.json({ error: 'Workspace not found' }, { status: 404 });
    }

    const { searchParams } = new URL(request.url);
    const runId = String(searchParams.get('runId') ?? '').trim();

    if (runId) {
      const run = await runStore.getFounderRun(workspaceId, runId);
      if (!run) {
        return NextResponse.json({ error: 'Founder run not found' }, { status: 404 });
      }
      return NextResponse.json({ run });
    }

    const runs = await runStore.listFounderRunsByWorkspace(workspaceId);
    return NextResponse.json({ runs });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

export async function POST(request, { params }) {
  const { workspaceId } = await params;

  try {
    const workspace = await runStore.getFounderWorkspace(workspaceId);
    if (!workspace) {
      return NextResponse.json({ error: 'Workspace not found' }, { status: 404 });
    }

    const run = normalizeFounderRunBody(await request.json(), workspaceId);
    if (!run.runId) {
      return NextResponse.json({ error: 'runId is required' }, { status: 400 });
    }

    const savedRun = await runStore.saveFounderRun(run);
    return NextResponse.json({ run: savedRun }, { status: 201 });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}