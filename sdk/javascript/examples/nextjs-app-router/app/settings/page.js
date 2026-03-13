'use client';

import { useState, useEffect, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

export default function SettingsPage() {
  const [config, setConfig] = useState(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await apiClient.get('/api/config');
      setConfig(data);
    } catch {
      // config endpoint may not exist yet
      setConfig(null);
    }
    setLoading(false);
  }, []);

  useEffect(() => { load(); }, [load]);

  return (
    <div>
      <div className="page-header">
        <h1>Settings</h1>
        <button className="btn btn-ghost btn-sm" onClick={load} disabled={loading}>Refresh</button>
      </div>
      <div className="page-body">
        {loading && <div className="loading-state"><div className="spinner" /></div>}
        {!loading && !config && (
          <div className="info-card">
            <h4>Configuration</h4>
            <p>Settings are managed via <code>~/.openfang/config.toml</code>. A settings UI for editing config in-browser is coming soon.</p>
            <div style={{ marginTop: 12 }}>
              <a href="https://github.com/RightNow-AI/openfang/blob/main/docs/configuration.md" target="_blank" rel="noreferrer" className="btn btn-ghost btn-sm">
                View config docs ↗
              </a>
            </div>
          </div>
        )}
        {!loading && config && (
          <div className="card">
            <div className="card-header">Active configuration</div>
            <pre style={{ fontSize: 12, overflow: 'auto', margin: 0, color: 'var(--text-dim)' }}>
              {JSON.stringify(config, null, 2)}
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}
