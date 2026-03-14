'use client';

/**
 * ChatClient — run-based conversation interface.
 *
 * All messages enter through alive.
 * Each assistant response is a Run with a timeline, agent trace, and output.
 *
 * Architecture:
 *   POST /api/runs  →  alive service  →  specialist  →  done
 *   GET  /api/runs/:id/events  (SSE)  →  live events streamed here
 */

import { useState, useCallback, useRef, useEffect } from 'react';
import Composer from '../components/Composer';
import RunTimeline from '../components/RunTimeline';
import RunOutput from '../components/RunOutput';
import AgentTrace from '../components/AgentTrace';

// ─── helpers ──────────────────────────────────────────────────────────────────

function terminalStatus(status) {
  return status === 'completed' || status === 'failed' || status === 'cancelled';
}

function statusFromEvent(event) {
  if (event.type === 'run.started') return 'running';
  if (event.type === 'run.completed') return 'completed';
  if (event.type === 'run.failed') return 'failed';
  if (event.type === 'run.status' && event.status === 'cancelled') return 'cancelled';
  return null;
}

// ─── ChatClient ───────────────────────────────────────────────────────────────

export default function ChatClient() {
  const [turns, setTurns] = useState([]);
  const [running, setRunning] = useState(false);
  const [showTrace, setShowTrace] = useState(false);
  const [error, setError] = useState('');
  const [sessionId] = useState(() =>
    typeof crypto !== 'undefined' ? crypto.randomUUID() : String(Date.now()),
  );

  const sseRef = useRef(null);
  const bottomRef = useRef(null);
  const activeTurnIdRef = useRef(null);

  // Scroll to bottom when turns update
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [turns, running]);

  // Clean up SSE on unmount
  useEffect(() => {
    return () => sseRef.current?.close();
  }, []);

  const handleSubmit = useCallback(
    async (message) => {
      if (running) return;
      setError('');
      setRunning(true);

      const turnId = crypto.randomUUID();
      activeTurnIdRef.current = turnId;

      // Optimistic turn — no runId yet
      setTurns((prev) => [
        ...prev,
        { id: turnId, userMessage: message, runId: null, events: [], status: 'queued' },
      ]);

      let runId;
      try {
        const res = await fetch('/api/runs', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ sessionId, message }),
        });
        const data = await res.json();
        if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
        runId = data.runId;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        setTurns((prev) => prev.map((t) => (t.id === turnId ? { ...t, status: 'failed' } : t)));
        setRunning(false);
        return;
      }

      // Attach runId to turn
      setTurns((prev) =>
        prev.map((t) => (t.id === turnId ? { ...t, runId, status: 'running' } : t)),
      );

      // Open SSE
      const source = new EventSource(`/api/runs/${runId}/events`);
      sseRef.current = source;

      source.onmessage = (e) => {
        if (activeTurnIdRef.current !== turnId) return;

        let event;
        try { event = JSON.parse(e.data); } catch { return; }

        setTurns((prev) =>
          prev.map((t) => {
            if (t.id !== turnId) return t;
            const newEvents = [...t.events, event];
            const newStatus = statusFromEvent(event) ?? t.status;
            return { ...t, events: newEvents, status: newStatus };
          }),
        );

        // Close when parent run reaches terminal state
        const isParentTerminal =
          (event.type === 'run.completed' || event.type === 'run.failed') &&
          event.runId === runId;

        if (isParentTerminal) {
          source.close();
          sseRef.current = null;
          setRunning(false);
        }
      };

      source.onerror = () => {
        source.close();
        sseRef.current = null;
        setTurns((prev) =>
          prev.map((t) =>
            t.id === turnId && !terminalStatus(t.status) ? { ...t, status: 'failed' } : t,
          ),
        );
        setError('Connection lost. Check that the daemon is running.');
        setRunning(false);
      };
    },
    [running, sessionId],
  );

  const handleCancel = useCallback(async () => {
    const activeTurn = turns.find((t) => t.id === activeTurnIdRef.current);
    if (!activeTurn?.runId) return;
    sseRef.current?.close();
    sseRef.current = null;
    try { await fetch(`/api/runs/${activeTurn.runId}/cancel`, { method: 'POST' }); } catch {}
    setTurns((prev) =>
      prev.map((t) =>
        t.id === activeTurnIdRef.current ? { ...t, status: 'cancelled' } : t,
      ),
    );
    setRunning(false);
  }, [turns]);

  return (
    <div data-cy="chat-page" style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - 48px)', minHeight: 500 }}>
      {/* ── Header ── */}
      <div className="page-header">
        <div className="flex items-center gap-3">
          <h1>Chat</h1>
          <span
            title="All messages route through alive"
            style={{
              display: 'inline-flex', alignItems: 'center', gap: 5,
              padding: '3px 10px', borderRadius: 99, fontSize: 11, fontWeight: 700,
              background: 'var(--accent)22', color: 'var(--accent)',
              border: '1px solid var(--accent)44',
              fontFamily: 'var(--font-mono, monospace)',
              userSelect: 'none',
            }}
          >
            <span style={{ width: 6, height: 6, borderRadius: '50%', background: 'var(--accent)' }} />
            alive
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => setShowTrace((v) => !v)}
            style={{ fontSize: 11 }}
          >
            {showTrace ? 'Hide trace' : 'Show trace'}
          </button>
          {running && (
            <button
              className="btn btn-ghost btn-sm"
              onClick={handleCancel}
              style={{ color: 'var(--error, #f87171)' }}
            >
              Cancel
            </button>
          )}
        </div>
      </div>

      {/* ── Conversation ── */}
      <div
        data-cy="chat-messages"
        style={{
          flex: 1, overflowY: 'auto', padding: '8px 0 16px',
          display: 'flex', flexDirection: 'column', gap: 16,
        }}
      >
        {turns.length === 0 && (
          <div data-cy="chat-empty-state" className="empty-state" style={{ marginTop: 60 }}>
            Send a message. alive will route it to the right specialist.
          </div>
        )}

        {error && (
          <div data-cy="chat-error" className="error-state" style={{ marginTop: 8 }}>
            ⚠ {error}
          </div>
        )}

        {turns.map((turn) => (
          <div key={turn.id} style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {/* User message */}
            <div
              data-cy="message-bubble"
              style={{ display: 'flex', justifyContent: 'flex-end' }}
            >
              <div
                style={{
                  maxWidth: '72%', padding: '10px 14px',
                  borderRadius: '14px 14px 4px 14px',
                  background: 'var(--accent)', color: '#fff',
                  fontSize: 13, lineHeight: 1.6,
                  whiteSpace: 'pre-wrap', wordBreak: 'break-word',
                  boxShadow: 'var(--shadow-xs)',
                }}
              >
                {turn.userMessage}
              </div>
            </div>

            {/* Run response */}
            {turn.runId && (
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                <div
                  style={{
                    flex: 1, padding: '12px 14px',
                    borderRadius: '4px 14px 14px 14px',
                    background: 'var(--bg-elevated)',
                    border: '1px solid var(--border)',
                    boxShadow: 'var(--shadow-xs)',
                    display: 'flex', flexDirection: 'column', gap: 10,
                  }}
                >
                  <RunTimeline
                    events={turn.events}
                    status={turn.status}
                    compact={terminalStatus(turn.status)}
                  />
                  {(turn.events.some((e) => e.type === 'run.token') || terminalStatus(turn.status)) && (
                    <RunOutput events={turn.events} status={turn.status} />
                  )}
                  {turn.status === 'cancelled' && (
                    <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>Cancelled</div>
                  )}
                </div>

                {showTrace && (
                  <div style={{ width: 160, flexShrink: 0 }}>
                    <AgentTrace events={turn.events} runId={turn.runId} />
                  </div>
                )}
              </div>
            )}
          </div>
        ))}

        <div ref={bottomRef} />
      </div>

      {/* ── Composer ── */}
      <div style={{ borderTop: '1px solid var(--border)', paddingTop: 12 }}>
        <Composer onSubmit={handleSubmit} disabled={running} />
      </div>
    </div>
  );
}
