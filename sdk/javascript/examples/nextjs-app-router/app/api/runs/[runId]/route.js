/**
 * GET /api/runs/:runId
 *
 * Returns the current state of a run including its replay buffer of events
 * and any child runs.
 */

import { NextResponse } from 'next/server';
import { runStore } from '../../../../lib/run-store';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(_request, { params }) {
  const { runId } = await params;

  try {
    const run = await runStore.get(runId);
    if (!run) {
      return NextResponse.json({ error: 'Run not found' }, { status: 404 });
    }

    const children = await runStore.getChildren(runId);

    return NextResponse.json({
      ...run,
      children: children.map((c) => ({
        runId: c.runId,
        agent: c.agent,
        status: c.status,
        startedAt: c.startedAt,
        updatedAt: c.updatedAt,
        output: c.output,
        error: c.error,
      })),
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
