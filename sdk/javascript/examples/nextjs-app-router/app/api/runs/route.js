/**
 * POST /api/runs
 *
 * Creates a new top-level run through alive and returns the runId immediately.
 * The run progresses asynchronously; subscribe to GET /api/runs/:runId/events for updates.
 *
 * Body: { sessionId?: string, message: string, playbookId?: string, workspaceId?: string, clientId?: string, context?: object }
 * Response: { runId: string, entryAgent: 'alive', status: 'queued' | 'running' }
 */

import { NextResponse } from 'next/server';
import { randomUUID } from 'node:crypto';
import { aliveService } from '../../../lib/alive-service';
import { getFounderPlaybook } from '../../../lib/founder-playbooks';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  // List recent runs
  const { runStore } = await import('../../../lib/run-store');
  try {
    const runs = await runStore.listRecent(50);
    return NextResponse.json({ runs });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

export async function POST(request) {
  try {
    const body = await request.json();
    const message = String(body?.message ?? '').trim();
    const playbookId = String(body?.playbookId ?? '').trim() || null;
    const workspaceId = String(body?.workspaceId ?? '').trim() || null;
    const incomingContext = body?.context && typeof body.context === 'object' && !Array.isArray(body.context)
      ? body.context
      : null;
    const clientId = String(body?.clientId ?? incomingContext?.client_id ?? '').trim() || null;
    const context = {
      ...(incomingContext ?? {}),
      ...(clientId ? { client_id: clientId } : {}),
      ...(playbookId ? { playbook_id: playbookId } : {}),
      ...(workspaceId ? { workspace_id: workspaceId } : {}),
    };

    if (!message) {
      return NextResponse.json({ error: 'message is required' }, { status: 400 });
    }

    if (playbookId && !getFounderPlaybook(playbookId)) {
      return NextResponse.json({ error: 'Unknown playbookId' }, { status: 400 });
    }

    // Use provided sessionId or generate a new one
    const sessionId = String(body?.sessionId ?? '').trim() || randomUUID();

    const { runId, status } = await aliveService.start({
      sessionId,
      message,
      playbookId,
      workspaceId,
      context: Object.keys(context).length > 0 ? context : null,
    });

    return NextResponse.json(
      { runId, entryAgent: 'alive', status, sessionId, playbookId, workspaceId, clientId },
      { status: 201 },
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
