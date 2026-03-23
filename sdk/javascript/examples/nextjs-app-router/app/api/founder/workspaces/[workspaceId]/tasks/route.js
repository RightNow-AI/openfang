import { NextResponse } from 'next/server';
import { runStore } from '../../../../../../lib/run-store';
import { taskStore } from '../../../../../../lib/task-store';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(_request, { params }) {
  const { workspaceId } = await params;

  try {
    const workspace = await runStore.getFounderWorkspace(workspaceId);
    if (!workspace) {
      return NextResponse.json({ error: 'Workspace not found' }, { status: 404 });
    }

    const tasks = await taskStore.getTasksByWorkspace(workspaceId);
    return NextResponse.json({ tasks });
  } catch (err) {
    console.error(`[GET /api/founder/workspaces/${workspaceId}/tasks]`, err);
    return NextResponse.json({ error: 'Internal Server Error' }, { status: 500 });
  }
}