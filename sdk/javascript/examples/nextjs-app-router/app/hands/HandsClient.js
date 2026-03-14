'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

function categoryBadge(cat) {
  const c = (cat || '').toLowerCase();
  if (c === 'productivity') return <span className="badge badge-info">{cat}</span>;
  if (c === 'content') return <span className="badge badge-created">{cat}</span>;
  if (c === 'data') return <span className="badge badge-warn">{cat}</span>;
  if (c === 'communication') return <span className="badge badge-success">{cat}</span>;
  return <span className="badge badge-dim">{cat || 'general'}</span>;
}

export default function HandsClient({ initialHands }) {
  const [hands, setHands] = useState(initialHands ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [expanded, setExpanded] = useState({});

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/hands');
      setHands(Array.isArray(data?.hands) ? data.hands : Array.isArray(data) ? data : []);
    } catch (e) {
      setError(e.message || 'Could not load hands.');
    }
    setLoading(false);
  }, []);

  const toggle = (id) => setExpanded(prev => ({ ...prev, [id]: !prev[id] }));

  const ready = hands.filter(h => h.requirements_met);
  const notReady = hands.filter(h => !h.requirements_met);

  return (
    <div data-cy="hands-page">
      <div className="page-header">
        <h1>Hands</h1>
        <div className="flex items-center gap-2">
          <span className="badge badge-success">{ready.length} ready</span>
          {notReady.length > 0 && (
            <span className="badge badge-muted">{notReady.length} needs setup</span>
          )}
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div data-cy="hands-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}
        {!error && hands.length === 0 && (
          <div data-cy="hands-empty" className="empty-state">No hands available.</div>
        )}
        <div data-cy="hands-grid" className="grid grid-auto" style={{ gap: 16 }}>
          {hands.map(hand => (
            <div
              key={hand.id}
              data-cy="hand-card"
              className="card"
              style={{
                display: 'flex',
                flexDirection: 'column',
                gap: 10,
                opacity: hand.requirements_met ? 1 : 0.85,
              }}
            >
              {/* Header */}
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
                <span style={{ fontSize: 28, lineHeight: 1, flexShrink: 0 }}>{hand.icon || '🤖'}</span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
                    <span style={{ fontWeight: 700, fontSize: 14 }}>{hand.name}</span>
                    {hand.requirements_met
                      ? <span className="badge badge-success">Ready</span>
                      : <span className="badge badge-warn">Needs setup</span>}
                  </div>
                  <div style={{ marginTop: 2 }}>{categoryBadge(hand.category)}</div>
                </div>
              </div>

              {/* Description */}
              {hand.description && (
                <p style={{ margin: 0, fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5 }}>
                  {hand.description}
                </p>
              )}

              {/* Stats */}
              <div style={{ display: 'flex', gap: 16, fontSize: 11, color: 'var(--text-dim)' }}>
                {hand.tools?.length > 0 && <span>{hand.tools.length} tools</span>}
                {hand.settings_count > 0 && <span>{hand.settings_count} settings</span>}
              </div>

              {/* Requirements */}
              {!hand.requirements_met && hand.requirements?.length > 0 && (
                <div>
                  <div
                    data-cy="hand-requirements-toggle"
                    style={{ cursor: 'pointer', fontSize: 12, color: 'var(--text-dim)', marginBottom: 6 }}
                    onClick={() => toggle(hand.id)}
                  >
                    ⚠ Missing requirements {expanded[hand.id] ? '▲' : '▼'}
                  </div>
                  {expanded[hand.id] && (
                    <ul data-cy="hand-requirements-section" style={{ margin: 0, padding: '0 0 0 16px', listStyle: 'disc' }}>
                      {hand.requirements.map(req => (
                        <li
                          key={req.key}
                          style={{
                            fontSize: 12,
                            color: req.satisfied ? 'var(--success)' : 'var(--error)',
                            marginBottom: 3,
                          }}
                        >
                          {req.label}
                        </li>
                      ))}
                    </ul>
                  )}
                </div>
              )}

              {/* Tools preview */}
              {hand.requirements_met && hand.tools?.length > 0 && expanded[hand.id] && (
                <div data-cy="hand-tools-section" style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                  {hand.tools.slice(0, 8).map(t => (
                    <code key={t} style={{ marginRight: 6, background: 'var(--surface2)', padding: '1px 5px', borderRadius: 3 }}>{t}</code>
                  ))}
                  {hand.tools.length > 8 && <span>+{hand.tools.length - 8} more</span>}
                </div>
              )}

              {hand.requirements_met && hand.tools?.length > 0 && (
                <button
                  data-cy="hand-expand-btn"
                  className="btn btn-ghost btn-xs"
                  style={{ alignSelf: 'flex-start', fontSize: 11 }}
                  onClick={() => toggle(hand.id)}
                >
                  {expanded[hand.id] ? 'Hide tools' : `Show ${hand.tools.length} tools`}
                </button>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
