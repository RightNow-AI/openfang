'use client';

/**
 * RunTimeline
 *
 * Shows the sequential steps of a run as they happen.
 *
 * Props:
 *   events    — RunEvent[]  (from SSE or replay buffer)
 *   status    — 'queued' | 'running' | 'completed' | 'failed' | 'cancelled'
 *   compact   — boolean (default false) — smaller inline version
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

function agentBadge(name) {
  return (
    <span
      style={{
        display: 'inline-block',
        padding: '1px 8px',
        borderRadius: 99,
        fontSize: 11,
        fontWeight: 600,
        letterSpacing: '0.03em',
        background: agentColor(name) + '22',
        color: agentColor(name),
        border: `1px solid ${agentColor(name)}44`,
        fontFamily: 'var(--font-mono, monospace)',
      }}
    >
      {name}
    </span>
  );
}

/** Convert a raw RunEvent into a human-readable step line */
function eventToStep(event) {
  switch (event.type) {
    case 'run.started':
      if (event.parentRunId) {
        return { label: `${event.agent} started`, agent: event.agent, kind: 'start' };
      }
      return { label: 'alive started', agent: 'alive', kind: 'start' };

    case 'run.routed':
      if (event.toAgent === event.fromAgent || event.toAgent === 'alive') {
        return { label: 'alive answering directly', agent: 'alive', kind: 'route' };
      }
      return {
        label: `alive → ${event.toAgent}`,
        agent: event.toAgent,
        detail: event.reason,
        kind: 'route',
      };

    case 'run.token':
      return null; // output rendered separately

    case 'run.status':
      return { label: `${event.agent}: ${event.status}`, agent: event.agent, kind: 'status' };

    case 'run.completed':
      if (event.agent === 'alive') {
        return { label: 'completed', agent: 'alive', kind: 'done' };
      }
      return { label: `${event.agent} done`, agent: event.agent, kind: 'done' };

    case 'run.failed':
      return { label: `${event.agent} failed`, agent: event.agent, detail: event.error, kind: 'fail' };

    default:
      return null;
  }
}

const STEP_ICON = {
  start: '▶',
  route: '→',
  status: '·',
  done: '✓',
  fail: '✕',
};

export default function RunTimeline({ events = [], status = 'queued', compact = false }) {
  const steps = useMemo(() => {
    const all = [];
    for (const event of events) {
      const step = eventToStep(event);
      if (step) all.push(step);
    }
    return all;
  }, [events]);

  if (steps.length === 0 && status === 'queued') {
    return (
      <div style={{ fontSize: 12, color: 'var(--text-dim)', padding: compact ? '4px 0' : '8px 0' }}>
        <span className="spinner" style={{ width: 10, height: 10, marginRight: 6 }} />
        queued…
      </div>
    );
  }

  return (
    <div
      data-cy="run-timeline"
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: compact ? 2 : 4,
        padding: compact ? '4px 0' : '8px 0',
        fontSize: compact ? 11 : 12,
      }}
    >
      {steps.map((step, i) => (
        <div
          key={i}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            opacity: step.kind === 'done' ? 1 : 0.85,
          }}
        >
          <span
            style={{
              color:
                step.kind === 'fail'
                  ? 'var(--error, #f87171)'
                  : step.kind === 'done'
                  ? 'var(--success, #4ade80)'
                  : agentColor(step.agent),
              width: 14,
              textAlign: 'center',
              flexShrink: 0,
              fontFamily: 'var(--font-mono, monospace)',
            }}
          >
            {STEP_ICON[step.kind] ?? '·'}
          </span>
          {agentBadge(step.agent)}
          <span style={{ color: 'var(--text-dim)' }}>{step.label}</span>
          {step.detail && (
            <span style={{ color: 'var(--text-dim)', fontStyle: 'italic', fontSize: 10 }}>
              — {step.detail}
            </span>
          )}
        </div>
      ))}

      {status === 'running' && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, color: 'var(--text-dim)' }}>
          <span style={{ width: 14, textAlign: 'center' }}>
            <span className="spinner" style={{ width: 10, height: 10, display: 'inline-block' }} />
          </span>
          <span>running…</span>
        </div>
      )}
    </div>
  );
}
