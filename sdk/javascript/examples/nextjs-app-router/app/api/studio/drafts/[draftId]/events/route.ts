import { NextRequest } from 'next/server';
import { fallbackDraftEventsStream } from '../../../_lib/studio-proxy';

export const dynamic = 'force-dynamic';

type Props = {
  params: Promise<{ draftId: string }>;
};

const RUST_V1_BASE = process.env.RUST_API_URL ?? process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

export async function GET(request: NextRequest, { params }: Props) {
  const { draftId } = await params;

  try {
    const upstream = await fetch(`${RUST_V1_BASE}/v1/drafts/${draftId}/events`, {
      headers: { Accept: 'text/event-stream' },
      cache: 'no-store',
      signal: request.signal,
    });

    if (!upstream.ok || !upstream.body) {
      throw new Error('upstream unavailable');
    }

    return new Response(upstream.body, {
      status: 200,
      headers: {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache, no-transform',
        Connection: 'keep-alive',
        'X-Accel-Buffering': 'no',
      },
    });
  } catch {
    return fallbackDraftEventsStream(draftId, request.signal);
  }
}