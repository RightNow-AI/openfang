'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

function normalizeChannel(raw, i) {
  return {
    id: raw?.id ?? raw?.name ?? `ch-${i}`,
    name: raw?.name ?? raw?.id ?? 'Channel',
    type: raw?.type ?? raw?.adapter ?? '',
    adapter: raw?.adapter ?? raw?.type ?? '',
    status: raw?.status ?? raw?.state ?? 'unknown',
    description: raw?.description ?? '',
    agent_id: raw?.agent_id ?? '',
  };
}

function statusBadge(status) {
  const s = (status || '').toLowerCase();
  if (s === 'connected' || s === 'active' || s === 'enabled') return <span className="badge badge-success">Connected</span>;
  if (s === 'disconnected' || s === 'inactive' || s === 'disabled') return <span className="badge badge-muted">Disconnected</span>;
  if (s === 'error' || s === 'failed') return <span className="badge badge-error">Error</span>;
  return <span className="badge badge-dim">{status}</span>;
}

export default function ChannelsClient({ initialChannels }) {
  const [channels, setChannels] = useState(initialChannels ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/channels');
      const raw = Array.isArray(data) ? data : data?.channels ?? [];
      setChannels(raw.map(normalizeChannel));
    } catch (e) {
      setError(e.message || 'Could not load channels.');
    }
    setLoading(false);
  }, []);

  return (
    <div>
      <div className="page-header">
        <h1>Channels</h1>
        <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
          {loading ? 'Loading…' : 'Refresh'}
        </button>
      </div>
      <div className="page-body">
        {error && (
          <div className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}
        {channels.length === 0 && !error && (
          <div className="empty-state">
            No channels configured. Add channel adapters in your <code>openfang.toml</code>.
          </div>
        )}
        {channels.length > 0 && (
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
            {channels.map(ch => (
              <div key={ch.id} className="card">
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 8 }}>
                  <span style={{ fontWeight: 700, fontSize: 14 }}>{ch.name}</span>
                  {statusBadge(ch.status)}
                </div>
                {ch.type && <div className="text-sm text-dim" style={{ marginBottom: 4 }}>Type: <strong>{ch.type}</strong></div>}
                {ch.adapter && ch.adapter !== ch.type && (
                  <div className="text-sm text-dim" style={{ marginBottom: 4 }}>Adapter: <strong>{ch.adapter}</strong></div>
                )}
                {ch.description && <div className="text-sm text-muted">{ch.description}</div>}
                {ch.agent_id && <div className="text-xs text-muted" style={{ marginTop: 4 }}>Agent: {ch.agent_id}</div>}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
