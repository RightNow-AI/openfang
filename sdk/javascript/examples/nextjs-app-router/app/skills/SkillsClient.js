'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

function normalizeSkill(raw, i) {
  return {
    id: raw?.id ?? raw?.name ?? `skill-${i}`,
    name: raw?.name ?? raw?.id ?? 'Skill',
    description: raw?.description ?? '',
    version: raw?.version ?? '',
    enabled: raw?.enabled !== false,
    tags: Array.isArray(raw?.tags) ? raw.tags : [],
    entry_point: raw?.entry_point ?? '',
  };
}

export default function SkillsClient({ initialSkills }) {
  const [skills, setSkills] = useState(initialSkills ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/skills');
      const raw = Array.isArray(data) ? data : data?.skills ?? [];
      setSkills(raw.map(normalizeSkill));
    } catch (e) {
      setError(e.message || 'Could not load skills.');
    }
    setLoading(false);
  }, []);

  return (
    <div>
      <div className="page-header">
        <h1>Skills</h1>
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
        {skills.length === 0 && !error && (
          <div className="empty-state">
            No skills loaded. Add skill crates or plugins to your OpenFang config.
          </div>
        )}
        {skills.length > 0 && (
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
            {skills.map(s => (
              <div key={s.id} className="card">
                <div style={{ fontWeight: 700, fontSize: 14, marginBottom: 6 }}>{s.name}</div>
                {s.description && (
                  <div className="text-sm text-dim" style={{ marginBottom: 8 }}>{s.description}</div>
                )}
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
                  {s.version && <span className="badge badge-muted">v{s.version}</span>}
                  {s.enabled
                    ? <span className="badge badge-success">Enabled</span>
                    : <span className="badge badge-muted">Disabled</span>
                  }
                  {s.tags.map(tag => (
                    <span key={tag} className="badge badge-dim">{tag}</span>
                  ))}
                </div>
                {s.entry_point && (
                  <div className="text-xs text-muted" style={{ marginTop: 8 }}>
                    <code>{s.entry_point}</code>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
