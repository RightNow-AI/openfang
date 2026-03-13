'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

function normalizeUsage(u, b, ab) {
  const usage = u ?? {};
  const budget = b ?? {};
  const agentBudgets = Array.isArray(ab) ? ab : ab?.agents ?? [];
  return {
    totalTokens: usage.total_tokens ?? usage.tokens ?? 0,
    promptTokens: usage.prompt_tokens ?? usage.input_tokens ?? 0,
    completionTokens: usage.completion_tokens ?? usage.output_tokens ?? 0,
    totalCost: usage.total_cost ?? budget.spent ?? 0,
    totalRequests: usage.total_requests ?? usage.requests ?? 0,
    budgetLimit: budget.limit ?? budget.budget_limit ?? null,
    budgetSpent: budget.spent ?? budget.total_spent ?? usage.total_cost ?? 0,
    agentBudgets: agentBudgets.map(a => ({
      agentId: a?.agent_id ?? '',
      name: a?.name ?? a?.agent_name ?? a?.agent_id ?? 'Unknown',
      totalTokens: a?.total_tokens ?? a?.tokens ?? 0,
      totalCost: a?.total_cost ?? a?.cost ?? a?.spent ?? 0,
      totalRequests: a?.total_requests ?? a?.requests ?? 0,
    })),
  };
}

function fmtCost(n) {
  if (n === null || n === undefined || isNaN(n)) return '$0.0000';
  return `$${Number(n).toFixed(4)}`;
}

function fmtNum(n) {
  if (n === null || n === undefined) return '0';
  return Number(n).toLocaleString();
}

export default function AnalyticsClient({ initialStats }) {
  const [stats, setStats] = useState(initialStats ?? {
    totalTokens: 0, promptTokens: 0, completionTokens: 0,
    totalCost: 0, totalRequests: 0, budgetLimit: null, budgetSpent: 0, agentBudgets: [],
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    const [uRes, bRes, abRes] = await Promise.allSettled([
      apiClient.get('/api/usage'),
      apiClient.get('/api/budget'),
      apiClient.get('/api/budget/agents'),
    ]);
    if (uRes.status === 'rejected' && bRes.status === 'rejected') {
      setError('Could not load usage data.');
    } else {
      const u = uRes.status === 'fulfilled' ? uRes.value : null;
      const b = bRes.status === 'fulfilled' ? bRes.value : null;
      const ab = abRes.status === 'fulfilled' ? abRes.value : null;
      setStats(normalizeUsage(u, b, ab));
    }
    setLoading(false);
  }, []);

  const { totalTokens, promptTokens, completionTokens, totalCost, totalRequests, budgetLimit, budgetSpent, agentBudgets } = stats;
  const pct = budgetLimit > 0 ? Math.min(100, (budgetSpent / budgetLimit) * 100) : null;

  return (
    <div>
      <div className="page-header">
        <h1>Analytics</h1>
        <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
          {loading ? 'Loading…' : 'Refresh'}
        </button>
      </div>
      <div className="page-body">
        {error && (
          <div className="error-state">⚠ {error} <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button></div>
        )}

        <div className="grid grid-4" style={{ marginBottom: 20 }}>
          <div className="stat-card">
            <div className="stat-label">Total tokens</div>
            <div className="stat-value">{fmtNum(totalTokens)}</div>
            {promptTokens > 0 && (
              <div className="stat-sub">{fmtNum(promptTokens)} in / {fmtNum(completionTokens)} out</div>
            )}
          </div>
          <div className="stat-card">
            <div className="stat-label">Total cost</div>
            <div className="stat-value">{fmtCost(totalCost)}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Requests</div>
            <div className="stat-value">{fmtNum(totalRequests)}</div>
          </div>
          <div className="stat-card">
            <div className="stat-label">Budget remaining</div>
            <div className="stat-value" style={{ color: pct > 80 ? 'var(--error)' : 'var(--text)' }}>
              {budgetLimit ? fmtCost(budgetLimit - budgetSpent) : '—'}
            </div>
            {budgetLimit && <div className="stat-sub">{fmtCost(budgetLimit)} limit</div>}
          </div>
        </div>

        {pct !== null && (
          <div className="card" style={{ marginBottom: 20 }}>
            <div className="card-header">Budget usage</div>
            <div style={{ background: 'var(--surface2)', borderRadius: 4, height: 8, overflow: 'hidden' }}>
              <div style={{
                height: '100%',
                width: `${pct}%`,
                background: pct > 80 ? 'var(--error)' : pct > 60 ? 'var(--warning)' : 'var(--accent)',
                borderRadius: 4,
                transition: 'width 0.3s',
              }} />
            </div>
            <div className="text-sm text-dim" style={{ marginTop: 6 }}>
              {fmtCost(budgetSpent)} spent of {fmtCost(budgetLimit)} ({pct.toFixed(1)}%)
            </div>
          </div>
        )}

        {agentBudgets.length > 0 && (
          <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
            <div style={{ padding: '12px 16px', borderBottom: '1px solid var(--border-subtle)' }}>
              <span className="card-header" style={{ margin: 0 }}>Per-agent spend</span>
            </div>
            <table className="data-table">
              <thead>
                <tr>
                  <th>Agent</th>
                  <th>Tokens</th>
                  <th>Cost</th>
                  <th>Requests</th>
                </tr>
              </thead>
              <tbody>
                {agentBudgets.map((a, i) => (
                  <tr key={a.agentId || i}>
                    <td style={{ fontWeight: 600 }}>{a.name}</td>
                    <td>{fmtNum(a.totalTokens)}</td>
                    <td>{fmtCost(a.totalCost)}</td>
                    <td>{fmtNum(a.totalRequests)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        {totalTokens === 0 && totalCost === 0 && !error && (
          <div className="empty-state">No usage data recorded yet. Send a message to start tracking.</div>
        )}
      </div>
    </div>
  );
}
