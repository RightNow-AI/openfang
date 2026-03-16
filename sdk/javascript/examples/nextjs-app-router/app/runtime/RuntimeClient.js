'use client';
import { useState, useCallback } from 'react';
import Link from 'next/link';
import { apiClient } from '../../lib/api-client';
import { workApi } from '../../lib/work-api';

const EXEC_STATUS_BADGE = {
  completed:             'badge-success',
  failed:                'badge-error',
  blocked:               'badge-error',
  running:               'badge-accent',
  waiting_approval:      'badge-warn',
  retry_scheduled:       'badge-warn',
  delegated_to_subagent: 'badge-info',
};

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

function fmtDate(iso) {
  if (!iso) return '—';
  try { return new Date(iso).toLocaleString(); } catch { return iso; }
}

export default function RuntimeClient({ initialRuntime }) {
  const [runtime, setRuntime] = useState(initialRuntime ?? {
    isUp: false, version: '', uptimeSec: null, agentCount: 0,
    networkUp: false, nodeId: '', peerCount: 0, peers: [],
  });
  const [execItems, setExecItems] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    const [hRes, sRes, nRes, pRes, wRes] = await Promise.allSettled([
      apiClient.get('/api/health'),
      apiClient.get('/api/status'),
      apiClient.get('/api/network/status'),
      apiClient.get('/api/peers'),
      workApi.getWork({ limit: 20 }),
    ]);
    if (hRes.status === 'rejected' && sRes.status === 'rejected') {
      setError('Could not reach daemon.');
    } else {
      const h = hRes.status === 'fulfilled' ? hRes.value : null;
      const s = sRes.status === 'fulfilled' ? sRes.value : null;
      const n = nRes.status === 'fulfilled' ? nRes.value : null;
      const p = pRes.status === 'fulfilled' ? pRes.value : null;
      const w = wRes.status === 'fulfilled' ? wRes.value : null;
      setRuntime(normalizeRuntime(h, s, n, p));
      const rawItems = Array.isArray(w?.items) ? w.items : [];
      setExecItems(rawItems.filter(i => i.status !== 'pending' && i.status !== 'ready').slice(0, 15));
    }
    setLoading(false);
  }, []);

  const { isUp, version, uptimeSec, agentCount, networkUp, nodeId, peerCount, peers } = runtime;

  return (
    <div data-cy="runtime-page">
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

        <div className="card" style={{ padding: 0, overflow: 'hidden', marginTop: 20 }} data-cy="runtime-execution-activity">
          <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--border-subtle)' }}>
            <span className="card-header" style={{ margin: 0 }}>Execution activity</span>
          </div>
          {execItems.length === 0 ? (
            <div style={{ padding: '20px 16px', color: 'var(--text-muted)', fontSize: 13 }}>No executed work items yet.</div>
          ) : (
            <table className="data-table">
              <thead>
                <tr><th style={{ width: '30%' }}>Title</th><th style={{ width: 120 }}>Status</th><th>Agent</th><th style={{ width: 160 }}>Completed</th><th style={{ width: 60 }}></th></tr>
              </thead>
              <tbody>
                {execItems.map(it => (
                  <tr key={it.id} data-cy="runtime-exec-item">
                    <td style={{ fontWeight: 500, maxWidth: 200 }} className="truncate">{it.title}</td>
                    <td>
                      <span className={`badge ${EXEC_STATUS_BADGE[it.status] ?? 'badge-dim'}`} style={{ fontSize: 11 }}>
                        {it.status?.replace(/_/g, ' ')}
                      </span>
                    </td>
                    <td style={{ fontSize: 12, color: 'var(--text-secondary)' }}>{it.assigned_agent_name || '—'}</td>
                    <td style={{ fontSize: 11, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{fmtDate(it.completed_at)}</td>
                    <td>
                      <Link href={`/work/${it.id}`} className="btn btn-ghost btn-xs" data-cy="runtime-exec-item-link">View</Link>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
