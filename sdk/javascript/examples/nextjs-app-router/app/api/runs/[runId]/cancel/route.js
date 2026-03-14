/**
 * POST /api/runs/:runId/cancel
 *
 * Cancels a run and any of its queued/running children.
 */

import { NextResponse } from 'next/server';
import { aliveService } from '../../../../../lib/alive-service';
import { runStore } from '../../../../../lib/run-store';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(_request, { params }) {
  const { runId } = await params;

  try {
    const run = await runStore.get(runId);
    if (!run) {
      return NextResponse.json({ error: 'Run not found' }, { status: 404 });
    }

    await aliveService.cancel(runId);

    return NextResponse.json({ runId, status: 'cancelled' });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
