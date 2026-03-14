'use client';

/**
 * RunOutput
 *
 * Renders the streaming output from a run, tagging each chunk with the
 * agent that produced it.
 *
 * Props:
 *   events  — RunEvent[]  (from SSE or replay buffer)
 *   status  — RunStatus
 */

import { useMemo } from 'react';

const AGENT_COLOR = {
  alive: 'var(--accent)',
  coder: '#4ade80',
  debugger: '#f97316',
  'code-reviewer': '#a78bfa',
  researcher: '#38bdf8',
  planner: '#fbbf24',
  writer: '#f472b6',
};

function agentColor(name) {
  return AGENT_COLOR[name] ?? 'var(--text-dim)';
}

export default function RunOutput({ events = [], status = 'queued' }) {
  // Collect all token chunks in order
  const chunks = useMemo(() => {
    return events
      .filter((e) => e.type === 'run.token')
      .map((e) => ({ agent: e.agent, content: e.content }));
  }, [events]);

  const finalOutput = useMemo(() => {
    // The last run.completed on the root agent is the canonical output
    const completed = events.filter((e) => e.type === 'run.completed');
    if (completed.length === 0) return null;
    const last = completed[completed.length - 1];
    return typeof last.output === 'string' ? last.output : null;
  }, [events]);

  if (chunks.length === 0 && !finalOutput) {
    if (status === 'queued' || status === 'running') {
      return (
        <div
          data-cy="run-output"
          style={{ color: 'var(--text-dim)', fontSize: 13, padding: '8px 0', minHeight: 40 }}
        >
          <span className="spinner" style={{ width: 12, height: 12, marginRight: 8, display: 'inline-block' }} />
          Waiting for response…
        </div>
      );
    }
    return null;
  }

  // Show live token chunks while running, final output when done
  const textToShow = finalOutput ?? chunks.map((c) => c.content).join('');
  const agentTag = chunks[0]?.agent ?? 'alive';

  return (
    <div data-cy="run-output" style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          fontSize: 11,
          color: 'var(--text-dim)',
          marginBottom: 4,
        }}
      >
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: agentColor(agentTag),
            flexShrink: 0,
          }}
        />
        <span style={{ color: agentColor(agentTag), fontWeight: 600, fontFamily: 'var(--font-mono, monospace)' }}>
          {agentTag}
        </span>
        {status === 'running' && (
          <span className="spinner" style={{ width: 10, height: 10, marginLeft: 4 }} />
        )}
      </div>
      <div
        style={{
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-word',
          lineHeight: 1.6,
          fontSize: 14,
          color: 'var(--text)',
        }}
      >
        {textToShow}
        {status === 'running' && (
          <span
            style={{
              display: 'inline-block',
              width: 2,
              height: '1em',
              background: 'var(--accent)',
              marginLeft: 2,
              verticalAlign: 'text-bottom',
              animation: 'blink 1s step-end infinite',
            }}
          />
        )}
      </div>
    </div>
  );
}
