'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

function normalizeAgent(raw) {
  return {
    id: raw?.id ?? '',
    name: raw?.name ?? raw?.id ?? 'Unnamed agent',
    model: raw?.model ?? '',
    provider: raw?.provider ?? '',
    status: raw?.status ?? raw?.loop_state ?? 'unknown',
    memory_backend: raw?.memory_backend ?? raw?.memory?.backend ?? 'default',
  };
}

function statusBadge(status) {
  const s = (status || '').toLowerCase();
  if (s === 'idle' || s === 'ready') return <span className="badge badge-success">{status}</span>;
  if (s === 'running' || s === 'busy') return <span className="badge badge-warn">{status}</span>;
  if (s === 'error' || s === 'failed') return <span className="badge badge-error">{status}</span>;
  if (s === 'stopped' || s === 'offline') return <span className="badge badge-muted">{status}</span>;
  return <span className="badge badge-dim">{status || 'unknown'}</span>;
}

export default function SessionsClient({ initialAgents }) {
  const [agents, setAgents] = useState(initialAgents ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/agents');
      const raw = Array.isArray(data) ? data : data?.agents ?? [];
      setAgents(raw.map(normalizeAgent));
    } catch (e) {
      setError(e.message || 'Could not load agents.');
    }
    setLoading(false);
  }, []);

  return (
    <div>
      <div className="page-header">
        <h1>Sessions</h1>
        <div className="flex items-center gap-2">
          <span className="text-dim text-sm">{agents.length} agent{agents.length !== 1 ? 's' : ''}</span>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}
        {agents.length === 0 && !error && (
          <div className="empty-state">No agents loaded. Define agents in your <code>agents/</code> directory.</div>
        )}
        {agents.length > 0 && (
          <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Model</th>
                  <th>Provider</th>
                  <th>Status</th>
                  <th>Memory</th>
                </tr>
              </thead>
              <tbody>
                {agents.map(a => (
                  <tr key={a.id}>
                    <td style={{ fontWeight: 600 }}>{a.name}</td>
                    <td><code style={{ fontSize: 12 }}>{a.model || '—'}</code></td>
                    <td>{a.provider || '—'}</td>
                    <td>{statusBadge(a.status)}</td>
                    <td style={{ fontSize: 12, color: 'var(--text-dim)' }}>{a.memory_backend}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
