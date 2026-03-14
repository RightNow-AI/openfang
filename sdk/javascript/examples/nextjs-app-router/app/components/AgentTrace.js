'use client';

/**
 * AgentTrace
 *
 * Shows the run tree as a nested indented list.
 *
 *   alive
 *     → planner
 *     → researcher
 *     → writer
 *
 * Props:
 *   events  — RunEvent[]
 *   runId   — string (the root runId)
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

const STATUS_ICON = {
  queued: '○',
  running: '◌',
  completed: '●',
  failed: '✕',
  cancelled: '–',
};

/** Build a flat list of { runId, agent, status, parentRunId } from events */
function buildNodes(events, rootRunId) {
  const nodes = new Map();

  for (const event of events) {
    if (event.type === 'run.started') {
      if (!nodes.has(event.runId)) {
        nodes.set(event.runId, {
          runId: event.runId,
          agent: event.agent,
          status: 'running',
          parentRunId: event.parentRunId ?? null,
        });
      }
    }
    if (event.type === 'run.completed' && nodes.has(event.runId)) {
      nodes.get(event.runId).status = 'completed';
    }
    if (event.type === 'run.failed' && nodes.has(event.runId)) {
      nodes.get(event.runId).status = 'failed';
    }
    if (event.type === 'run.status' && nodes.has(event.runId)) {
      nodes.get(event.runId).status = event.status;
    }
  }

  // Ensure root node is always present
  if (!nodes.has(rootRunId)) {
    nodes.set(rootRunId, { runId: rootRunId, agent: 'alive', status: 'queued', parentRunId: null });
  }

  return nodes;
}

function renderNode(node, nodes, depth = 0) {
  const children = [...nodes.values()].filter((n) => n.parentRunId === node.runId);
  const icon = STATUS_ICON[node.status] ?? '○';
  const color = agentColor(node.agent);

  return (
    <div key={node.runId} style={{ paddingLeft: depth * 16 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          padding: '2px 0',
          fontSize: 12,
        }}
      >
        <span
          style={{
            color: node.status === 'completed' ? color : node.status === 'failed' ? 'var(--error, #f87171)' : color,
            fontFamily: 'var(--font-mono, monospace)',
            width: 12,
          }}
        >
          {icon}
        </span>
        <span
          style={{
            color,
            fontWeight: 600,
            fontFamily: 'var(--font-mono, monospace)',
            fontSize: 11,
          }}
        >
          {node.agent}
        </span>
        <span style={{ color: 'var(--text-dim)', fontSize: 10 }}>{node.status}</span>
      </div>
      {children.map((child) => renderNode(child, nodes, depth + 1))}
    </div>
  );
}

export default function AgentTrace({ events = [], runId }) {
  const nodes = useMemo(() => buildNodes(events, runId), [events, runId]);
  const root = nodes.get(runId);

  if (!root) return null;

  return (
    <div
      data-cy="agent-trace"
      style={{
        background: 'var(--bg-elevated)',
        border: '1px solid var(--border)',
        borderRadius: 'var(--radius-sm)',
        padding: '8px 12px',
      }}
    >
      <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-dim)', letterSpacing: '0.08em', marginBottom: 6 }}>
        AGENT TRACE
      </div>
      {renderNode(root, nodes, 0)}
    </div>
  );
}
