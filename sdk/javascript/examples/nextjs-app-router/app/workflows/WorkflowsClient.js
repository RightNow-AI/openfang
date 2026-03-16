'use client';
import { useState, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import { apiClient } from '../../lib/api-client';
import { workApi } from '../../lib/work-api';

export default function WorkflowsClient({ initialWorkflows }) {
  const [workflows, setWorkflows] = useState(initialWorkflows ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [running, setRunning] = useState({});
  const [createdItems, setCreatedItems] = useState({});
  const router = useRouter();

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/workflows');
      setWorkflows(Array.isArray(data) ? data : data?.workflows ?? []);
    } catch (e) {
      setError(e.message || 'Could not load workflows.');
    }
    setLoading(false);
  }, []);

  const run = useCallback(async (template) => {
    setRunning(prev => ({ ...prev, [template.id]: true }));
    setError('');
    try {
      const created = await workApi.createWork({
        title: template.name,
        description: template.description || '',
        work_type: 'workflow',
        payload: { workflow_template_id: template.id },
      });
      setCreatedItems(prev => ({ ...prev, [template.id]: { ok: true, id: created.id } }));
      // Navigate to the new work item detail page
      router.push(`/work/${created.id}`);
    } catch (e) {
      setCreatedItems(prev => ({ ...prev, [template.id]: { ok: false, msg: e.message || 'Run failed.' } }));
    }
    setRunning(prev => ({ ...prev, [template.id]: false }));
  }, [router]);

  return (
    <div data-cy="workflows-page">
      <div className="page-header">
        <h1>Workflows</h1>
        <div className="flex items-center gap-2">
          <span className="text-dim text-sm">{workflows.length} workflow{workflows.length !== 1 ? 's' : ''}</span>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div data-cy="workflows-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}
        {!error && workflows.length === 0 && (
          <div data-cy="workflows-empty" className="empty-state">
            <span style={{ fontSize: 28, opacity: 0.4 }}>▶</span>
            <div>
              <div style={{ fontWeight: 600, color: 'var(--text-secondary)', marginBottom: 4 }}>No workflows defined</div>
              <div className="text-dim text-sm">Create workflow TOML files to add automation pipelines.</div>
            </div>
          </div>
        )}
        {workflows.length > 0 && (
          <div data-cy="workflows-table" className="card" style={{ padding: 0, overflow: 'hidden' }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th style={{ width: '28%' }}>Name</th>
                  <th>Description</th>
                  <th style={{ width: 60 }}>Steps</th>
                  <th style={{ width: 100 }}>Status</th>
                  <th style={{ width: 80 }}></th>
                </tr>
              </thead>
              <tbody>
                {workflows.map(w => (
                  <tr data-cy="workflow-row" key={w.id}>
                    <td style={{ fontWeight: 600, color: 'var(--text)' }}>{w.name}</td>
                    <td style={{ fontSize: 12, color: 'var(--text-dim)', maxWidth: 280 }}>
                      {w.description || <span className="text-muted">—</span>}
                    </td>
                    <td style={{ fontSize: 12, color: 'var(--text-dim)' }}>{w.steps ?? '—'}</td>
                    <td>
                      {createdItems[w.id] ? (
                        <span
                          data-cy="workflow-result-badge"
                          className={`badge ${createdItems[w.id].ok ? 'badge-success' : 'badge-error'}`}
                        >
                          {createdItems[w.id].ok ? 'Queued' : 'Error'}
                        </span>
                      ) : (
                        <span className="badge badge-dim">ready</span>
                      )}
                    </td>
                    <td style={{ textAlign: 'right' }}>
                      <button
                        data-cy="workflow-run-btn"
                        className="btn btn-primary btn-xs"
                        onClick={() => run(w)}
                        disabled={!!running[w.id]}
                        title={`Run ${w.name}`}
                      >
                        {running[w.id] ? '…' : '▶ Run'}
                      </button>
                    </td>
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
