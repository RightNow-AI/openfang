import { NextResponse } from 'next/server';
import { runStore } from '../../../../../lib/run-store';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function normalizeWorkspacePatch(body) {
  return {
    clientId: String(body?.clientId ?? '').trim(),
    name: String(body?.name ?? '').trim(),
    companyName: String(body?.companyName ?? '').trim(),
    idea: String(body?.idea ?? '').trim(),
    stage: String(body?.stage ?? '').trim(),
    playbookDefaults: body?.playbookDefaults && typeof body.playbookDefaults === 'object' && !Array.isArray(body.playbookDefaults)
      ? body.playbookDefaults
      : null,
  };
}

export async function GET(_request, { params }) {
  const { workspaceId } = await params;

  try {
    const workspace = await runStore.getFounderWorkspace(workspaceId);
    if (!workspace) {
      return NextResponse.json({ error: 'Workspace not found' }, { status: 404 });
    }

    return NextResponse.json({ workspace });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

export async function PUT(request, { params }) {
  const { workspaceId } = await params;

  try {
    const existing = await runStore.getFounderWorkspace(workspaceId);
    if (!existing) {
      return NextResponse.json({ error: 'Workspace not found' }, { status: 404 });
    }

    const patch = normalizeWorkspacePatch(await request.json());
    const workspace = await runStore.upsertFounderWorkspace({
      workspaceId,
      clientId: patch.clientId || existing.clientId,
      name: patch.name || existing.name,
      companyName: patch.companyName || existing.companyName,
      idea: patch.idea || existing.idea,
      stage: patch.stage || existing.stage,
      playbookDefaults: patch.playbookDefaults ?? existing.playbookDefaults,
    });

    return NextResponse.json({ workspace });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

export const PATCH = PUT;