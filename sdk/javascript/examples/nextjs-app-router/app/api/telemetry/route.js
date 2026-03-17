/**
 * POST /api/telemetry
 *
 * Receives fire-and-forget events from the browser and logs them server-side.
 * Returns 204 — no body. Failures here do not propagate to the client.
 */
import { NextResponse } from 'next/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(request) {
  try {
    const body = await request.json();
    if (body?.event) {
      const props = body.props && Object.keys(body.props).length
        ? ` ${JSON.stringify(body.props)}`
        : '';
      const ts = body.ts ? ` @${new Date(body.ts).toISOString()}` : '';
      console.log(`[telemetry] ${body.event}${props}${ts}`);
    }
  } catch {
    // malformed payload — ignore
  }
  return new NextResponse(null, { status: 204 });
}
