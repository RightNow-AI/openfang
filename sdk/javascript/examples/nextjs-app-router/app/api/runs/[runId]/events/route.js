/**
 * GET /api/runs/:runId/events
 *
 * Server-Sent Events stream for a run.
 *
 * On connect:
 *   1. Replay all events already emitted (from the run's event buffer).
 *   2. Subscribe to the event bus for future events.
 *   3. When run.completed or run.failed arrives, close the stream.
 *
 * The client should handle EventSource reconnection automatically.
 * On reconnect, all events are replayed from the beginning (idempotent by design).
 */

import { runStore } from '../../../../../lib/run-store';
import { eventBus } from '../../../../../lib/event-bus';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const SSE_HEADERS = {
  'Content-Type': 'text/event-stream',
  'Cache-Control': 'no-cache, no-transform',
  Connection: 'keep-alive',
  'X-Accel-Buffering': 'no', // disable nginx buffering
};

function sseEvent(eventName, payload) {
  return `event: ${eventName}\ndata: ${JSON.stringify(payload)}\n\n`;
}

export async function GET(request, { params }) {
  const { runId } = await params;

  // Validate run exists
  const run = await runStore.get(runId);
  if (!run) {
    return new Response(sseEvent('error', { type: 'error', message: 'Run not found' }), {
      status: 404,
      headers: SSE_HEADERS,
    });
  }

  const encoder = new TextEncoder();

  const stream = new ReadableStream({
    start(controller) {
      function enqueue(text) {
        try {
          controller.enqueue(encoder.encode(text));
        } catch {
          // controller may already be closed
        }
      }

      function close() {
        try {
          controller.close();
        } catch {
          // already closed
        }
      }

      // ── 1. Replay buffered events ─────────────────────────────────────────
      const buffered = run.events ?? [];
      for (const event of buffered) {
        enqueue(sseEvent(event.type, event));
      }

      // ── 2. If run already terminal, close immediately ─────────────────────
      if (run.status === 'completed' || run.status === 'failed' || run.status === 'cancelled') {
        close();
        return;
      }

      // ── 3. Subscribe to live events ───────────────────────────────────────
      const unsubscribe = eventBus.subscribe(runId, (event) => {
        enqueue(sseEvent(event.type, event));

        const isTerminal =
          event.type === 'run.completed' ||
          event.type === 'run.failed' ||
          (event.type === 'run.status' && event.status === 'cancelled');

        // Close after the terminal event for the parent run (not child runs)
        if (isTerminal && event.runId === runId) {
          setTimeout(() => {
            unsubscribe();
            close();
          }, 50);
        }
      });

      // ── 4. Clean up on client disconnect ─────────────────────────────────
      request.signal.addEventListener('abort', () => {
        unsubscribe();
        close();
      });

      // ── 5. Keepalive heartbeat (every 20 s) ──────────────────────────────
      const heartbeat = setInterval(() => {
        enqueue(': heartbeat\n\n');
      }, 20_000);

      request.signal.addEventListener('abort', () => clearInterval(heartbeat));
    },
  });

  return new Response(stream, { headers: SSE_HEADERS });
}
