'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

function normalizeRuntime(h, s, n, p) {
  const health = h ?? {};
  const status = s ?? {};
  const network = n ?? {};
  const peersRaw = Array.isArray(p) ? p : p?.peers ?? [];
  return {
    isUp: health.status === 'ok' || health.healthy === true,
    version: health.version ?? status.version ?? '',
    uptimeSec: health.uptime_seconds ?? status.uptime_seconds ?? null,
    agentCount: status.agent_count ?? status.agents ?? 0,
    networkUp: network.connected ?? network.status === 'connected' ?? false,
    nodeId: network.node_id ?? '',
    peerCount: network.peer_count ?? peersRaw.length,
    peers: peersRaw.map(peer => ({
      id: peer?.id ?? peer?.peer_id ?? '',
      address: peer?.address ?? peer?.addr ?? '',
      latencyMs: peer?.latency_ms ?? null,
      status: peer?.status ?? 'unknown',
    })),
  };
}

function uptime(s) {
  if (s === null || s === undefined) return '—';
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = Math.floor(s % 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec}s`;
  return `${sec}s`;
}

function StatusDot({ ok }) {
  return (
    <span style={{
      display: 'inline-block', width: 8, height: 8, borderRadius: '50%',
      background: ok ? 'var(--success)' : 'var(--error)', marginRight: 6,
    }} />
  );
}

export default function RuntimeClient({ initialRuntime }) {
  const [runtime, setRuntime] = useState(initialRuntime ?? {
    isUp: false, version: '', uptimeSec: null, agentCount: 0,
    networkUp: false, nodeId: '', peerCount: 0, peers: [],
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    const [hRes, sRes, nRes, pRes] = await Promise.allSettled([
      apiClient.get('/api/health'),
      apiClient.get('/api/status'),
      apiClient.get('/api/network/status'),
      apiClient.get('/api/peers'),
    ]);
    if (hRes.status === 'rejected' && sRes.status === 'rejected') {
      setError('Could not reach daemon.');
    } else {
      const h = hRes.status === 'fulfilled' ? hRes.value : null;
      const s = sRes.status === 'fulfilled' ? sRes.value : null;
      const n = nRes.status === 'fulfilled' ? nRes.value : null;
      const p = pRes.status === 'fulfilled' ? pRes.value : null;
      setRuntime(normalizeRuntime(h, s, n, p));
    }
    setLoading(false);
  }, []);

  const { isUp, version, uptimeSec, agentCount, networkUp, nodeId, peerCount, peers } = runtime;

  return (
    <div>
      <div className="page-header">
        <h1>Runtime</h1>
        <div className="flex items-center gap-2">
          <span style={{ fontSize: 13 }}>
            <StatusDot ok={isUp} />
            <span className={isUp ? '' : 'text-dim'}>{isUp ? 'Daemon healthy' : 'Daemon offline'}</span>
          </span>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Loading…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div className="error-state">⚠ {error} <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button></div>
        )}

        <div className="grid grid-4" style={{ marginBottom: 20 }}>
          <div className="stat-card">
            <div className="stat-label">Status</div>
            <div className="stat-value" style={{ fontSize: 18 }}>
              <StatusDot ok={isUp} />{isUp ? 'Online' : 'Offline'}
            </div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Version</div>
            <div className="stat-value" style={{ fontSize: 18 }}>{version || '—'}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Uptime</div>
            <div className="stat-value" style={{ fontSize: 18 }}>{uptime(uptimeSec)}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Agents loaded</div>
            <div className="stat-value" style={{ fontSize: 18 }}>{agentCount}</div>
          </div>
        </div>

        <div className="card" style={{ marginBottom: 20 }}>
          <div className="card-header">OFP Network</div>
          <div className="flex items-center gap-2" style={{ marginBottom: 8 }}>
            <StatusDot ok={networkUp} />
            <span>{networkUp ? 'Connected to OFP mesh' : 'Not connected to OFP mesh'}</span>
            {nodeId && <span className="badge badge-muted">{nodeId}</span>}
          </div>
          {peerCount > 0 && (
            <div className="text-sm text-dim">{peerCount} peer(s) visible</div>
          )}
        </div>

        {peers.length > 0 && (
          <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
            <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--border-subtle)' }}>
              <span className="card-header" style={{ margin: 0 }}>Connected peers ({peers.length})</span>
            </div>
            <table className="data-table">
              <thead>
                <tr><th>Peer ID</th><th>Address</th><th>Latency</th><th>Status</th></tr>
              </thead>
              <tbody>
                {peers.map((p, i) => (
                  <tr key={p.id || i}>
                    <td><code style={{ fontSize: 11 }}>{p.id || '—'}</code></td>
                    <td>{p.address || '—'}</td>
                    <td>{p.latencyMs != null ? `${p.latencyMs}ms` : '—'}</td>
                    <td>
                      <span className={`badge ${p.status === 'connected' ? 'badge-success' : 'badge-muted'}`}>
                        {p.status}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {!isUp && !error && (
          <div className="info-card">
            <p>Start the daemon with <code>openfang start</code> to see runtime details.</p>
          </div>
        )}
      </div>
    </div>
  );
}
