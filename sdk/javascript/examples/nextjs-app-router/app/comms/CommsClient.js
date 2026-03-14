'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

const KIND_LABELS = {
  agent_message: 'Message',
  agent_spawned: 'Spawned',
  agent_terminated: 'Terminated',
  task_posted: 'Task Posted',
  task_claimed: 'Task Claimed',
  task_completed: 'Task Completed',
};

function kindBadge(kind) {
  if (kind === 'agent_message') return <span className="badge badge-info">Message</span>;
  if (kind === 'agent_spawned') return <span className="badge badge-success">Spawned</span>;
  if (kind === 'agent_terminated') return <span className="badge badge-muted">Terminated</span>;
  if (kind === 'task_posted') return <span className="badge badge-created">Task Posted</span>;
  if (kind === 'task_claimed') return <span className="badge badge-warn">Task Claimed</span>;
  if (kind === 'task_completed') return <span className="badge badge-success">Task Done</span>;
  return <span className="badge badge-dim">{KIND_LABELS[kind] ?? kind}</span>;
}

function stateBadge(state) {
  const s = (state || '').toLowerCase();
  if (s === 'running') return <span className="badge badge-success">{state}</span>;
  if (s === 'suspended' || s === 'idle') return <span className="badge badge-muted">{state}</span>;
  if (s === 'error') return <span className="badge badge-error">{state}</span>;
  return <span className="badge badge-dim">{state || 'unknown'}</span>;
}

function fmtTime(ts) {
  if (!ts) return '—';
  try {
    return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  } catch {
    return ts;
  }
}

export default function CommsClient({ initialTopology, initialEvents }) {
  const [topology, setTopology] = useState(initialTopology ?? { nodes: [], edges: [] });
  const [events, setEvents] = useState(initialEvents ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [tab, setTab] = useState('topology');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const [topoData, eventsData] = await Promise.all([
        apiClient.get('/api/comms/topology'),
        apiClient.get('/api/comms/events?limit=50'),
      ]);
      if (topoData) setTopology(topoData);
      if (Array.isArray(eventsData)) setEvents(eventsData);
    } catch (e) {
      setError(e.message || 'Could not load comms data.');
    }
    setLoading(false);
  }, []);

  const nodes = topology?.nodes ?? [];
  const edges = topology?.edges ?? [];

  const tabStyle = (active) => ({
    padding: '5px 14px',
    borderRadius: 'var(--radius-sm)',
    fontSize: 13,
    fontWeight: active ? 600 : 400,
    cursor: 'pointer',
    background: active ? 'var(--accent-subtle)' : 'transparent',
    color: active ? 'var(--accent)' : 'var(--text-dim)',
    border: '1px solid ' + (active ? 'rgba(255,106,26,0.2)' : 'transparent'),
  });

  return (
    <div data-cy="comms-page">
      <div className="page-header">
        <h1>Comms</h1>
        <div className="flex items-center gap-2">
          <span className="text-dim text-sm">{nodes.length} agent{nodes.length !== 1 ? 's' : ''}</span>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div data-cy="comms-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}

        {/* Tabs */}
        <div style={{ display: 'flex', gap: 6, marginBottom: 16 }}>
          <button data-cy="comms-tab-topology" style={tabStyle(tab === 'topology')} onClick={() => setTab('topology')}>
            Topology ({nodes.length})
          </button>
          <button data-cy="comms-tab-events" style={tabStyle(tab === 'events')} onClick={() => setTab('events')}>
            Events ({events.length})
          </button>
        </div>

        {/* Topology tab */}
        {tab === 'topology' && (
          <div data-cy="comms-topology-panel">
            {nodes.length === 0 && !error && (
              <div data-cy="comms-empty-topology" className="empty-state">No agents in topology yet.</div>
            )}
            {nodes.length > 0 && (
              <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
                <table data-cy="comms-topology-table" className="data-table">
                  <thead>
                    <tr>
                      <th>Agent</th>
                      <th>Model</th>
                      <th>State</th>
                      <th>Connections</th>
                    </tr>
                  </thead>
                  <tbody>
                    {nodes.map(node => {
                      const connections = edges.filter(e => e.from === node.id || e.to === node.id).length;
                      return (
                        <tr key={node.id}>
                          <td style={{ fontWeight: 600 }}>{node.name}</td>
                          <td><code style={{ fontSize: 11 }}>{node.model || '—'}</code></td>
                          <td>{stateBadge(node.state)}</td>
                          <td style={{ fontSize: 12, color: 'var(--text-dim)' }}>
                            {connections > 0 ? connections : '—'}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
            {edges.length > 0 && (
              <div style={{ marginTop: 16 }}>
                <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase', letterSpacing: '0.5px' }}>
                  Edges ({edges.length})
                </div>
                <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
                  <table className="data-table">
                    <thead>
                      <tr><th>From</th><th>Kind</th><th>To</th></tr>
                    </thead>
                    <tbody>
                      {edges.map((e, i) => {
                        const fromNode = nodes.find(n => n.id === e.from);
                        const toNode = nodes.find(n => n.id === e.to);
                        return (
                          <tr key={i}>
                            <td>{fromNode?.name ?? e.from}</td>
                            <td><span className="badge badge-dim">{e.kind}</span></td>
                            <td>{toNode?.name ?? e.to}</td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </div>
        )}

        {/* Events tab */}
        {tab === 'events' && (
          <div data-cy="comms-events-panel">
            {events.length === 0 && !error && (
              <div data-cy="comms-empty-events" className="empty-state">No communication events yet.</div>
            )}
            {events.length > 0 && (
              <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
                <table data-cy="comms-events-table" className="data-table">
                  <thead>
                    <tr>
                      <th>Time</th>
                      <th>Kind</th>
                      <th>From</th>
                      <th>To</th>
                      <th>Detail</th>
                    </tr>
                  </thead>
                  <tbody>
                    {events.map(ev => (
                      <tr key={ev.id}>
                        <td style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>
                          {fmtTime(ev.timestamp)}
                        </td>
                        <td>{kindBadge(ev.kind)}</td>
                        <td style={{ fontSize: 12 }}>{ev.source_name || ev.source_id || '—'}</td>
                        <td style={{ fontSize: 12 }}>{ev.target_name || ev.target_id || '—'}</td>
                        <td style={{ fontSize: 12, color: 'var(--text-dim)', maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {ev.detail || '—'}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
