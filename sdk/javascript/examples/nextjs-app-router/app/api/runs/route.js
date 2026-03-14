/**
 * POST /api/runs
 *
 * Creates a new top-level run through alive and returns the runId immediately.
 * The run progresses asynchronously; subscribe to GET /api/runs/:runId/events for updates.
 *
 * Body: { sessionId?: string, message: string }
 * Response: { runId: string, entryAgent: 'alive', status: 'queued' | 'running' }
 */

import { NextResponse } from 'next/server';
import { randomUUID } from 'node:crypto';
import { aliveService } from '../../../lib/alive-service';

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

    if (!message) {
      return NextResponse.json({ error: 'message is required' }, { status: 400 });
    }

    // Use provided sessionId or generate a new one
    const sessionId = String(body?.sessionId ?? '').trim() || randomUUID();

    const { runId, status } = await aliveService.start({ sessionId, message });

    return NextResponse.json(
      { runId, entryAgent: 'alive', status, sessionId },
      { status: 201 },
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
