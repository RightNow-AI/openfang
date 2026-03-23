import { NextResponse } from 'next/server';
import { runStore } from '../../../../../../../lib/run-store';
import { ALLOWED_TASK_STATUSES, taskStore } from '../../../../../../../lib/task-store';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function PATCH(request, { params }) {
  const { workspaceId, taskId } = await params;

  try {
    const workspace = await runStore.getFounderWorkspace(workspaceId);
    if (!workspace) {
      return NextResponse.json({ error: 'Workspace not found' }, { status: 404 });
    }

    let body = null;
    try {
      body = await request.json();
    } catch {
      return NextResponse.json({ error: 'Malformed JSON payload' }, { status: 400 });
    }

    const status = String(body?.status ?? '').trim();
    if (!ALLOWED_TASK_STATUSES.includes(status)) {
      return NextResponse.json(
        { error: `Invalid status. Must be one of: ${ALLOWED_TASK_STATUSES.join(', ')}` },
        { status: 400 },
      );
    }

    const task = await taskStore.updateTaskStatus(workspaceId, taskId, status);
    return NextResponse.json({ task });
  } catch (err) {
    console.error(`[PATCH /api/founder/workspaces/${workspaceId}/tasks/${taskId}]`, err);
    const message = err instanceof Error ? err.message : String(err);
    if (message.includes('Task not found') || message.includes('Authorization blocked')) {
      return NextResponse.json({ error: 'Not Found or Unauthorized' }, { status: 404 });
    }
    return NextResponse.json({ error: 'Internal Server Error' }, { status: 500 });
  }
}