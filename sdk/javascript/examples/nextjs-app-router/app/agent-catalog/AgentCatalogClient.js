'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

function normalizeEntry(raw) {
  return {
    catalog_id: raw?.catalog_id ?? '',
    agent_id: raw?.agent_id ?? '',
    name: raw?.name ?? raw?.agent_id ?? 'Unknown',
    description: raw?.description ?? '',
    division: raw?.division ?? '',
    source: raw?.source ?? 'native',
    source_label: raw?.source_label ?? '',
    tags: Array.isArray(raw?.tags) ? raw.tags : [],
    enabled: raw?.enabled ?? true,
    best_for: raw?.best_for ?? '',
    avoid_for: raw?.avoid_for ?? '',
    example: raw?.example ?? '',
    purpose: raw?.purpose ?? '',
    role: raw?.role ?? '',
  };
}

export default function AgentCatalogClient({ initialEntries }) {
  const [entries, setEntries] = useState(initialEntries ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [toggling, setToggling] = useState({});
  const [filter, setFilter] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/agents/catalog');
      const raw = Array.isArray(data?.agents) ? data.agents : Array.isArray(data) ? data : [];
      setEntries(raw.map(normalizeEntry));
    } catch (e) {
      setError(e.message || 'Could not load catalog.');
    }
    setLoading(false);
  }, []);

  const toggleEnabled = useCallback(async (catalogId, currentEnabled) => {
    setToggling(prev => ({ ...prev, [catalogId]: true }));
    try {
      await apiClient.put(`/api/agents/catalog/${encodeURIComponent(catalogId)}/enabled`, {
        enabled: !currentEnabled,
      });
      setEntries(prev =>
        prev.map(e => e.catalog_id === catalogId ? { ...e, enabled: !currentEnabled } : e)
      );
    } catch (e) {
      setError(e.message || 'Could not update agent.');
    }
    setToggling(prev => ({ ...prev, [catalogId]: false }));
  }, []);

  const filtered = filter.trim()
    ? entries.filter(e =>
        e.name.toLowerCase().includes(filter.toLowerCase()) ||
        e.description.toLowerCase().includes(filter.toLowerCase()) ||
        e.division.toLowerCase().includes(filter.toLowerCase()) ||
        e.tags.some(t => t.toLowerCase().includes(filter.toLowerCase()))
      )
    : entries;

  return (
    <div data-cy="catalog-page">
      <div className="page-header">
        <h1>Agent Catalog</h1>
        <div className="flex items-center gap-2">
          <input
            data-cy="catalog-filter"
            type="search"
            value={filter}
            onChange={e => setFilter(e.target.value)}
            placeholder="Filter agents…"
            style={{
              padding: '5px 10px',
              borderRadius: 'var(--radius-sm)',
              border: '1px solid var(--border)',
              background: 'var(--bg-elevated)',
              color: 'var(--text)',
              fontSize: 13,
              width: 180,
              outline: 'none',
            }}
          />
          <span className="text-dim text-sm">{filtered.length} / {entries.length}</span>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div data-cy="catalog-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}
        {!error && entries.length === 0 && (
          <div data-cy="catalog-empty" className="empty-state">No agents in catalog. Add agent.toml files to the agents/ directory.</div>
        )}
        {filtered.length === 0 && filter && (
          <div data-cy="catalog-filter-empty" className="empty-state">No agents match "{filter}".</div>
        )}
        <div data-cy="catalog-grid" className="grid grid-auto" style={{ gap: 16 }}>
          {filtered.map(entry => (
            <div key={entry.catalog_id} data-cy="catalog-card" className="card" style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
                <div style={{ minWidth: 0 }}>
                  <div className="card-header" style={{ marginBottom: 2 }}>{entry.name}</div>
                  <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 3 }}>
                    {entry.division && (
                      <span className="badge badge-info" style={{ fontSize: 10 }}>{entry.division}</span>
                    )}
                    <span
                      className={`badge ${entry.source === 'imported' ? 'badge-warning' : 'badge-success'}`}
                      style={{ fontSize: 10 }}
                    >
                      {entry.source === 'imported' ? 'Imported' : 'Native'}
                    </span>
                  </div>
                  {entry.role && (
                    <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 2 }}>{entry.role}</div>
                  )}
                </div>
                <button
                  data-cy="catalog-toggle-btn"
                  className={`btn btn-sm ${entry.enabled ? 'btn-ghost' : 'btn-primary'}`}
                  style={entry.enabled ? { color: 'var(--success)', borderColor: 'var(--success)' } : {}}
                  onClick={() => toggleEnabled(entry.catalog_id, entry.enabled)}
                  disabled={!!toggling[entry.catalog_id]}
                >
                  {toggling[entry.catalog_id] ? '…' : entry.enabled ? 'Enabled' : 'Disabled'}
                </button>
              </div>

              {entry.description && (
                <p style={{ margin: 0, fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5 }}>
                  {entry.description}
                </p>
              )}

              {entry.best_for && (
                <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>
                  <span style={{ fontWeight: 600 }}>Best for:</span> {entry.best_for}
                </div>
              )}

              {entry.example && (
                <div style={{
                  fontSize: 11,
                  background: 'var(--surface2)',
                  borderRadius: 'var(--radius-sm)',
                  padding: '5px 9px',
                  color: 'var(--text-dim)',
                  fontStyle: 'italic',
                }}>
                  "{entry.example}"
                </div>
              )}

              {entry.tags.length > 0 && (
                <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 'auto' }}>
                  {entry.tags.map(tag => (
                    <span key={tag} className="badge badge-dim">{tag}</span>
                  ))}
                </div>
              )}

              {entry.source_label && (
                <div style={{ fontSize: 10, color: 'var(--text-muted)' }}>
                  {entry.source_label}
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
