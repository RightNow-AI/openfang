'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import { apiClient } from '../../lib/api-client';
import { labelExecutionPath, pathBadgeClass } from '../../lib/planning-api';

// ── Helpers ──────────────────────────────────────────────────────────────────

function actionBadgeClass(action) {
  if (!action) return 'badge badge-dim';
  if (action === 'AgentSpawn' || action === 'AuthSuccess') return 'badge badge-success';
  if (['AgentKill', 'AgentTerminated', 'AuthFailure', 'CapabilityDenied'].includes(action)) return 'badge badge-error';
  if (action === 'RateLimited' || action === 'ToolInvoke') return 'badge badge-warn';
  return 'badge badge-created';
}

function providerBadgeClass(p) {
  if (p.auth_status === 'configured') {
    if (p.health === 'cooldown' || p.health === 'open') return 'badge badge-warn';
    return 'badge badge-success';
  }
  if (p.auth_status === 'not_set' || p.auth_status === 'missing') return 'badge badge-muted';
  return 'badge badge-dim';
}

function providerBadgeLabel(p) {
  if (p.health === 'cooldown') return 'cooldown';
  if (p.health === 'open') return 'CB open';
  if (p.auth_status === 'configured') return 'ready';
  return 'not set';
}

// ── Setup Checklist ───────────────────────────────────────────────────────────

function SetupChecklist({ providers, agents, channels }) {
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    setDismissed(localStorage.getItem('of-checklist-dismissed') === 'true');
  }, []);

  const items = [
    { key: 'provider', label: 'Configure an LLM provider', done: providers.some(p => p.auth_status === 'configured'), href: '/settings' },
    { key: 'agent', label: 'Create your first agent', done: agents.length > 0, href: '/chat' },
    { key: 'chat', label: 'Send your first message', done: typeof localStorage !== 'undefined' && localStorage.getItem('of-first-msg') === 'true', href: '/chat' },
    { key: 'channel', label: 'Connect a messaging channel', done: channels.length > 0, href: '/channels' },
  ];

  const doneCount = items.filter(i => i.done).length;
  const progress = (doneCount / items.length) * 100;

  if (dismissed || doneCount === items.length) return null;

  return (
    <div className="card mb-4" style={{ marginBottom: 20 }}>
      <div className="flex items-center justify-between gap-2" style={{ marginBottom: 12 }}>
        <div className="card-header" style={{ margin: 0 }}>Getting started — {doneCount}/{items.length}</div>
        <button
          className="btn btn-ghost btn-xs"
          onClick={() => { localStorage.setItem('of-checklist-dismissed', 'true'); setDismissed(true); }}
        >
          Dismiss
        </button>
      </div>
      <div style={{ height: 4, background: 'var(--surface3)', borderRadius: 4, marginBottom: 14 }}>
        <div style={{ height: '100%', width: `${progress}%`, background: 'var(--accent)', borderRadius: 4, transition: 'width 0.3s' }} />
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        {items.map(item => (
          <a key={item.key} href={item.href} className="flex items-center gap-2" style={{ textDecoration: 'none', opacity: item.done ? 0.55 : 1 }}>
            <span style={{ width: 16, height: 16, borderRadius: 4, background: item.done ? 'var(--success-subtle)' : 'var(--surface3)', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', fontSize: 10, color: item.done ? 'var(--success)' : 'transparent', flexShrink: 0 }}>
              {item.done ? '✓' : ''}
            </span>
            <span style={{ fontSize: 13, color: item.done ? 'var(--text-dim)' : 'var(--text-secondary)', textDecoration: item.done ? 'line-through' : 'none' }}>
              {item.label}
            </span>
          </a>
        ))}
      </div>
    </div>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function OverviewPage() {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [health, setHealth] = useState({});
  const [status, setStatus] = useState({});
  const [usageSummary, setUsageSummary] = useState({ total_tokens: 0, total_tools: 0, total_cost: 0, agent_count: 0 });
  const [recentAudit, setRecentAudit] = useState([]);
  const [channels, setChannels] = useState([]);
  const [providers, setProviders] = useState([]);
  const [mcpServers, setMcpServers] = useState([]);
  const [skillCount, setSkillCount] = useState(0);
  const [agents, setAgents] = useState([]);
  const [planningQueue, setPlanningQueue] = useState(null);
  const [lastRefresh, setLastRefresh] = useState(null);
  const refreshTimer = useRef(null);

  const loadAll = useCallback(async (silent = false) => {
    if (!silent) setLoading(true);
    setError('');
    try {
      const [healthData, statusData, usageData, auditData,
        channelsData, providersData, mcpData, skillsData, agentsData, workData,
      ] = await Promise.allSettled([
        apiClient.get('/api/health'),
        apiClient.get('/api/status'),
        apiClient.get('/api/usage'),
        apiClient.get('/api/audit/recent?n=8'),
        apiClient.get('/api/channels'),
        apiClient.get('/api/providers'),
        apiClient.get('/api/mcp/servers'),
        apiClient.get('/api/skills'),
        apiClient.get('/api/agents'),
        apiClient.get('/api/work?limit=100'),
      ]);

      if (healthData.status === 'fulfilled') setHealth(healthData.value || {});
      if (statusData.status === 'fulfilled') setStatus(statusData.value || {});
      else if (!silent) throw new Error(statusData.reason?.message || 'Connection failed');

      if (usageData.status === 'fulfilled') {
        const agents = usageData.value?.agents || [];
        setUsageSummary({
          total_tokens: agents.reduce((s, a) => s + (a.total_tokens || 0), 0),
          total_tools: agents.reduce((s, a) => s + (a.tool_calls || 0), 0),
          total_cost: agents.reduce((s, a) => s + (a.cost_usd || 0), 0),
          agent_count: agents.length,
        });
      }

      if (auditData.status === 'fulfilled') setRecentAudit(auditData.value?.entries || []);
      if (channelsData.status === 'fulfilled') setChannels((channelsData.value?.channels || []).filter(c => c.has_token));
      if (providersData.status === 'fulfilled') setProviders(providersData.value?.providers || []);
      if (mcpData.status === 'fulfilled') setMcpServers(mcpData.value?.servers || []);
      if (skillsData.status === 'fulfilled') setSkillCount((skillsData.value?.skills || []).length);
      if (agentsData.status === 'fulfilled') setAgents(agentsData.value || []);

      if (workData.status === 'fulfilled') {
        const items = workData.value?.items ?? workData.value ?? [];
        const arr = Array.isArray(items) ? items : [];
        const byPath = {};
        arr.forEach(it => {
          const path = it.payload?.execution_path || it.payload?.scope?.path || null;
          if (path) byPath[path] = (byPath[path] ?? 0) + 1;
        });
        setPlanningQueue({
          total: arr.length,
          pending: arr.filter(i => ['pending', 'ready'].includes(i.status)).length,
          running: arr.filter(i => i.status === 'running').length,
          waiting_approval: arr.filter(i => i.status === 'waiting_approval').length,
          failed: arr.filter(i => i.status === 'failed').length,
          byPath,
        });
      }

      setLastRefresh(Date.now());
    } catch (e) {
      if (!silent) setError(e.message || 'Could not load overview data.');
    }
    if (!silent) setLoading(false);
  }, []);

  useEffect(() => {
    loadAll();
    refreshTimer.current = setInterval(() => loadAll(true), 30000);
    return () => clearInterval(refreshTimer.current);
  }, [loadAll]);

  const configuredProviders = providers.filter(p => p.auth_status === 'configured');
  const connectedMcp = mcpServers.filter(s => s.status === 'connected');

  if (loading) return (
    <div data-cy="overview-page">
      <div className="page-header"><h1>Overview</h1></div>
      <div className="loading-state"><div className="spinner" /><span>Loading overview…</span></div>
    </div>
  );

  if (error) return (
    <div data-cy="overview-page">
      <div className="page-header"><h1>Overview</h1><button className="btn btn-ghost btn-sm" onClick={() => loadAll()}>Retry</button></div>
      <div className="error-state">⚠ {error}</div>
    </div>
  );

  return (
    <div data-cy="overview-page">
      <div className="page-header">
        <h1>Overview</h1>
        <div className="flex items-center gap-2">
          {lastRefresh && <span className="text-xs text-muted">Last refreshed {new Date(lastRefresh).toLocaleTimeString()}</span>}
          <button className="btn btn-ghost btn-sm" onClick={() => loadAll()}>Refresh</button>
        </div>
      </div>

      <div className="page-body">
        <SetupChecklist providers={providers} agents={agents} channels={channels} />

        {/* Stats row */}
        <div className="grid grid-4 page-section">
          <div className="stat-card">
            <div className="stat-label">Agents</div>
            <div className="stat-value">{usageSummary.agent_count}</div>
            <div className="stat-sub">{agents.length} configured</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Tokens</div>
            <div className="stat-value">{usageSummary.total_tokens.toLocaleString()}</div>
            <div className="stat-sub">this session</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Tool calls</div>
            <div className="stat-value">{usageSummary.total_tools}</div>
            <div className="stat-sub">all agents</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Cost (USD)</div>
            <div className="stat-value">${usageSummary.total_cost.toFixed(4)}</div>
            <div className="stat-sub">estimated</div>
          </div>
        </div>

        {/* Providers + MCP row */}
        <div className="grid grid-2 page-section">
          <div className="card">
            <div className="card-header">
              LLM Providers
              <span style={{ fontSize: 11, color: 'var(--text-muted)', fontWeight: 400 }}>
                {configuredProviders.length}/{providers.length} configured
              </span>
            </div>
            {providers.length === 0 ? (
              <div className="text-dim text-sm" style={{ padding: '8px 0' }}>No providers found — <a href="/settings" className="text-accent">add one</a></div>
            ) : (
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 8 }}>
                {providers.map(p => (
                  <span key={p.id} className={providerBadgeClass(p)} title={p.display_name + ' — ' + (p.auth_status || p.health || '')}>
                    {p.display_name || p.id} · {providerBadgeLabel(p)}
                  </span>
                ))}
              </div>
            )}
          </div>

          <div className="card">
            <div className="card-header">
              MCP Servers
              <span style={{ fontSize: 11, color: 'var(--text-muted)', fontWeight: 400 }}>
                {connectedMcp.length}/{mcpServers.length} connected
              </span>
            </div>
            {mcpServers.length === 0 ? (
              <div className="text-dim text-sm" style={{ padding: '8px 0' }}>No MCP servers — <a href="/settings" className="text-accent">configure one</a></div>
            ) : (
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 8 }}>
                {mcpServers.map(s => (
                  <span key={s.name} className={`badge ${s.status === 'connected' ? 'badge-success' : 'badge-error'}`}>
                    {s.name} · {s.status}
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Channels + Skills row */}
        <div className="grid grid-2 page-section">
          <div className="card">
            <div className="card-header">Active Channels</div>
            {channels.length === 0 ? (
              <div className="text-dim text-sm" style={{ padding: '8px 0' }}>No channels connected — <a href="/channels" className="text-accent">connect one</a></div>
            ) : (
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 8 }}>
                {channels.map(ch => (
                  <span key={ch.name} className="badge badge-success">{ch.display_name || ch.name}</span>
                ))}
              </div>
            )}
          </div>

          <div className="card">
            <div className="card-header">Skills</div>
            <div className="stat-value" style={{ fontSize: 28, margin: '4px 0' }}>{skillCount}</div>
            <a href="/skills" className="text-sm text-accent">Browse skills →</a>
          </div>
        </div>

        {/* Planning Queue */}
        {planningQueue && (
          <div className="card page-section" data-cy="planning-queue-card">
            <div className="card-header">
              Work Queue
              <a href="/inbox" style={{ fontSize: 11, fontWeight: 500, color: 'var(--accent)' }}>View all →</a>
            </div>
            <div className="grid grid-4">
              {[
                { label: 'Pending / Ready',  value: planningQueue.pending,          badge: 'warn' },
                { label: 'Running',          value: planningQueue.running,          badge: 'info' },
                { label: 'Needs approval',   value: planningQueue.waiting_approval, badge: 'created' },
                { label: 'Failed',           value: planningQueue.failed,           badge: 'error' },
              ].map(({ label, value, badge }) => (
                <div key={label} style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
                  <div className="stat-label">{label}</div>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span className="stat-value" style={{ fontSize: 24 }}>{value}</span>
                    {value > 0 && <span className={`badge badge-${badge}`}>{value}</span>}
                  </div>
                </div>
              ))}
            </div>
            {Object.keys(planningQueue.byPath).length > 0 && (
              <div style={{ marginTop: 14, display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center' }}>
                <span className="text-xs text-muted">By execution path:</span>
                {Object.entries(planningQueue.byPath).map(([path, count]) => (
                  <span key={path} className={`badge ${pathBadgeClass(path)}`} data-cy={`planning-path-badge-${path}`}>
                    {labelExecutionPath(path)} · {count}
                  </span>
                ))}
              </div>
            )}
          </div>
        )}

        {/* System health */}
        <div className="card page-section">
          <div className="card-header">System Health</div>
          <div className="grid grid-3">
            {[
              { label: 'Status',  value: health.status  || status.status  || '—' },
              { label: 'Version', value: health.version || status.version || '—' },
              { label: 'Uptime',  value: status.uptime ? `${Math.round(status.uptime / 60)}m` : '—' },
            ].map(({ label, value }) => (
              <div key={label}>
                <div className="stat-label">{label}</div>
                <div className="text-mono" style={{ fontSize: 14, fontWeight: 600, marginTop: 4 }}>{value}</div>
              </div>
            ))}
          </div>
        </div>

        {/* Recent audit log */}
        {recentAudit.length > 0 && (
          <div className="card">
            <div className="card-header">
              Recent Audit Events
              <a href="/logs" style={{ fontSize: 11, fontWeight: 500, color: 'var(--accent)' }}>View all →</a>
            </div>
            <table className="data-table" style={{ marginTop: 8 }}>
              <thead>
                <tr>
                  <th>Action</th>
                  <th>Subject</th>
                  <th>Time</th>
                </tr>
              </thead>
              <tbody>
                {recentAudit.map((entry, i) => (
                  <tr key={i}>
                    <td><span className={actionBadgeClass(entry.action)}>{entry.action || '—'}</span></td>
                    <td className="text-dim text-sm truncate" style={{ maxWidth: 240 }}>{entry.subject || entry.agent_id || '—'}</td>
                    <td className="text-muted text-xs text-mono">{entry.timestamp ? new Date(entry.timestamp).toLocaleTimeString() : '—'}</td>
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
