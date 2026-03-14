/**
 * GET /api/agents
 *
 * Returns the full agent list split into public and internal.
 * The frontend should only surface publicAgents.
 * internalAgents is provided for dev-mode / agent trace views.
 */

import { NextResponse } from 'next/server';
import { agentRegistry } from '../../../lib/agent-registry';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  try {
    const [publicAgents, all] = await Promise.all([
      agentRegistry.listPublic(),
      agentRegistry.listAll(),
    ]);
    const internalAgents = all.filter((a) => a.visibility === 'internal');
    return NextResponse.json({ publicAgents, internalAgents });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 502 });
  }
}
