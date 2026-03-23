import { NextResponse } from 'next/server';
import { runStore } from '../../../../lib/run-store';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function normalizeWorkspaceBody(body) {
  return {
    workspaceId: String(body?.workspaceId ?? '').trim() || undefined,
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

export async function GET(request) {
  try {
    const { searchParams } = new URL(request.url);
    const clientId = String(searchParams.get('clientId') ?? '').trim() || null;
    const workspaces = await runStore.listFounderWorkspaces({ clientId });
    return NextResponse.json({ workspaces });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

export async function POST(request) {
  try {
    const body = normalizeWorkspaceBody(await request.json());

    if (!body.clientId) {
      return NextResponse.json({ error: 'clientId is required' }, { status: 400 });
    }

    if (!body.companyName) {
      return NextResponse.json({ error: 'companyName is required' }, { status: 400 });
    }

    const workspace = await runStore.upsertFounderWorkspace(body);
    return NextResponse.json({ workspace }, { status: 201 });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}