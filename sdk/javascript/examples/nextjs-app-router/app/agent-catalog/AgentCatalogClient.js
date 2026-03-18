'use client';
import { useState, useCallback, useEffect, useRef } from 'react';
import { useRouter } from 'next/navigation';
import { apiClient } from '../../lib/api-client';
import { track } from '../../lib/telemetry';
import { validateSpawnName, AGENT_NAME_MAX_LENGTH } from '../../lib/spawn-validation';
import { deriveSuggestedSkills } from '../../lib/agent-skills';
import { extractTomlField, extractTomlMultiline, patchTomlName } from '../../lib/toml-helpers';
import BoundSkillsSection from './BoundSkillsSection';
import SpawnSuccessBanner from './SpawnSuccessBanner';

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

// ─── Detail Modal ──────────────────────────────────────────────────────────
function DetailModal({ entry, onClose, onSpawnSuccess }) {
  const [templateData, setTemplateData] = useState(null);
  const [loadError, setLoadError] = useState('');
  const [loadingTemplate, setLoadingTemplate] = useState(true);
  const [spawnName, setSpawnName] = useState(entry.name);
  const [spawning, setSpawning] = useState(false);
  const [spawnError, setSpawnError] = useState('');
  const [promptExpanded, setPromptExpanded] = useState(false);
  const [preflightResult, setPreflightResult] = useState(null);     // null | PreflightResult
  const [preflightPending, setPreflightPending] = useState(false);
  const [preflightWarningsAck, setPreflightWarningsAck] = useState(false);
  const nameRef = useRef(null);

  useEffect(() => {
    nameRef.current?.focus();
    const controller = new AbortController();
    const templateKey = entry.agent_id || entry.name;
    setLoadingTemplate(true);
    setLoadError('');
    fetch(`/api/templates/${encodeURIComponent(templateKey)}`, { signal: controller.signal })
      .then(r => r.json())
      .then(data => {
        setTemplateData(data);
        setLoadingTemplate(false);
      })
      .catch(e => {
        if (e.name === 'AbortError') return;
        setLoadError(e.message || 'Could not load template details.');
        setLoadingTemplate(false);
      });
    return () => controller.abort();
  }, [entry.agent_id, entry.name]);

  // Close on Escape
  useEffect(() => {
    const handler = (e) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onClose]);

  const handleSpawn = async () => {
    if (spawning) return; // guard against double-click
    const validation = validateSpawnName(spawnName);
    if (validation.error) {
      setSpawnError(validation.error);
      nameRef.current?.focus();
      return;
    }
    const name = validation.name;
    if (!templateData?.manifest_toml) {
      setSpawnError('Template TOML not available. Cannot spawn.');
      return;
    }

    const toml = patchTomlName(templateData.manifest_toml, name);

    // ── Preflight ─────────────────────────────────────────────────────────
    // Run preflight unless the user has already acknowledged warnings once.
    if (!preflightWarningsAck) {
      setPreflightPending(true);
      setPreflightResult(null);
      track('agent_preflight_started', { template: entry.name, name });
      try {
        const pf = await apiClient.post('/api/agents/preflight', { manifest_toml: toml });
        setPreflightResult(pf);
        setPreflightPending(false);

        if (!pf.ok) {
          track('agent_preflight_failed', {
            template: entry.name,
            name,
            error_codes: pf.errors?.map(e => e.code),
          });
          return; // block spawn — user sees error list below
        }

        if (pf.warnings?.length > 0) {
          track('agent_preflight_warned', {
            template: entry.name,
            name,
            warning_codes: pf.warnings.map(w => w.code),
          });
          return; // show warnings — user must click "Spawn Anyway"
        }

        track('agent_preflight_passed', { template: entry.name, name });
      } catch {
        // Preflight endpoint unreachable — fail open and continue spawning.
        // The spawn route itself also enforces preflight server-side.
        setPreflightPending(false);
      }
    }

    // ── Spawn ─────────────────────────────────────────────────────────────
    setSpawning(true);
    setSpawnError('');
    track('spawn_started', { template: entry.name, name });
    try {
      const data = await apiClient.post('/api/agents/spawn', { manifest_toml: toml });
      const spawnedId = data?.agent_id ?? data?.id ?? '';
      track('spawn_succeeded', { template: entry.name, name, agentId: spawnedId });
      onSpawnSuccess({ agentId: spawnedId, name });
    } catch (e) {
      const msg = e.message || 'Spawn failed. Is the daemon running?';
      track('spawn_failed', { template: entry.name, name, error: msg });
      setSpawnError(msg);
      setSpawning(false);
    }
  };

  const manifest = templateData?.manifest ?? {};
  const systemPrompt = extractTomlMultiline(templateData?.manifest_toml, 'system_prompt');
  const provider = manifest?.model?.provider ?? entry.division ?? '—';
  const model = manifest?.model?.model ?? '—';
  const temperature = manifest?.model?.temperature ?? '—';
  const maxTokens = manifest?.model?.max_tokens ?? '—';
  const tools = manifest?.capabilities?.tools ?? [];
  const boundSkills = manifest?.skills?.length
    ? manifest.skills
    : null;
  const suggestedSkills = !boundSkills && templateData?.suggested_skills?.length
    ? templateData.suggested_skills
    : (!boundSkills && tools.length ? deriveSuggestedSkills(manifest) : null);

  return (
    <div
      data-cy="agent-detail-overlay"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
      style={{
        position: 'fixed', inset: 0, zIndex: 1000,
        background: 'rgba(0,0,0,0.55)', backdropFilter: 'blur(2px)',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        padding: 24,
      }}
    >
      <div
        data-cy="agent-detail-panel"
        style={{
          background: 'var(--bg-elevated)',
          border: '1px solid var(--border)',
          borderRadius: 'var(--radius)',
          boxShadow: 'var(--shadow-lg, 0 20px 60px rgba(0,0,0,.4))',
          width: '100%', maxWidth: 580,
          maxHeight: '90vh',
          display: 'flex', flexDirection: 'column',
          overflow: 'hidden',
        }}
      >
        {/* Header */}
        <div style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          padding: '16px 20px', borderBottom: '1px solid var(--border)',
          flexShrink: 0,
        }}>
          <div style={{ minWidth: 0 }}>
            <div style={{ fontWeight: 700, fontSize: 15 }}>{entry.name}</div>
            <div style={{ display: 'flex', gap: 6, marginTop: 4, flexWrap: 'wrap' }}>
              {entry.division && <span className="badge badge-info" style={{ fontSize: 10 }}>{entry.division}</span>}
              <span className={`badge ${entry.source === 'imported' ? 'badge-warning' : 'badge-success'}`} style={{ fontSize: 10 }}>
                {entry.source === 'imported' ? 'Imported' : 'Native'}
              </span>
              {entry.tags.slice(0, 3).map(t => (
                <span key={t} className="badge badge-dim" style={{ fontSize: 10 }}>{t}</span>
              ))}
            </div>
          </div>
          <button
            className="btn btn-ghost btn-sm"
            onClick={onClose}
            style={{ flexShrink: 0, marginLeft: 12, fontSize: 16, padding: '2px 8px' }}
            aria-label="Close"
          >✕</button>
        </div>

        {/* Scrollable body */}
        <div style={{ overflowY: 'auto', flex: 1, padding: '16px 20px', display: 'flex', flexDirection: 'column', gap: 16 }}>

          {/* Description */}
          {entry.description && (
            <p style={{ margin: 0, fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.6 }}>
              {entry.description}
            </p>
          )}

          {loadingTemplate && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: 'var(--text-dim)', fontSize: 13 }}>
              <div className="spinner" style={{ width: 14, height: 14 }} />
              Loading template…
            </div>
          )}

          {loadError && (
            <div className="error-state" style={{ fontSize: 12 }}>⚠ {loadError}</div>
          )}

          {!loadingTemplate && !loadError && (
            <>
              {/* Model config */}
              <section>
                <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em', marginBottom: 8 }}>
                  Model Config
                </div>
                <div style={{
                  display: 'grid', gridTemplateColumns: '1fr 1fr',
                  gap: '6px 16px', fontSize: 12,
                  background: 'var(--surface2)', borderRadius: 'var(--radius-sm)',
                  padding: '10px 14px',
                }}>
                  <div><span style={{ color: 'var(--text-dim)' }}>Provider</span></div>
                  <div style={{ fontFamily: 'var(--font-mono,monospace)', color: 'var(--accent)' }}>{provider}</div>
                  <div><span style={{ color: 'var(--text-dim)' }}>Model</span></div>
                  <div style={{ fontFamily: 'var(--font-mono,monospace)' }}>{model}</div>
                  <div><span style={{ color: 'var(--text-dim)' }}>Temperature</span></div>
                  <div>{temperature}</div>
                  <div><span style={{ color: 'var(--text-dim)' }}>Max tokens</span></div>
                  <div>{maxTokens}</div>
                </div>
              </section>

              {/* Capabilities */}
              {tools.length > 0 && (
                <section>
                  <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em', marginBottom: 8 }}>
                    Tools ({tools.length})
                  </div>
                  <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
                    {tools.map(t => (
                      <span key={t} className="badge badge-dim" style={{ fontFamily: 'var(--font-mono,monospace)', fontSize: 11 }}>{t}</span>
                    ))}
                  </div>
                </section>
              )}

              {/* Bound Skills (Phase 4) */}
              <BoundSkillsSection bindings={boundSkills} suggested={suggestedSkills} />

              {/* System prompt */}
              {systemPrompt && (
                <section>
                  <button
                    onClick={() => setPromptExpanded(v => !v)}
                    style={{
                      background: 'none', border: 'none', cursor: 'pointer',
                      padding: 0, display: 'flex', alignItems: 'center', gap: 6,
                      fontSize: 11, fontWeight: 700, color: 'var(--text-dim)',
                      textTransform: 'uppercase', letterSpacing: '.05em',
                    }}
                  >
                    <span style={{ transition: 'transform .15s', display: 'inline-block', transform: promptExpanded ? 'rotate(90deg)' : 'rotate(0deg)' }}>▶</span>
                    System Prompt
                    <span className="badge badge-muted" style={{ fontSize: 10, textTransform: 'none', letterSpacing: 0 }}>
                      {systemPrompt.length} chars
                    </span>
                  </button>
                  {promptExpanded && (
                    <pre style={{
                      marginTop: 8, padding: '10px 12px',
                      background: 'var(--surface2)',
                      border: '1px solid var(--border)',
                      borderRadius: 'var(--radius-sm)',
                      fontSize: 11, lineHeight: 1.6,
                      color: 'var(--text-secondary)',
                      fontFamily: 'var(--font-mono,monospace)',
                      whiteSpace: 'pre-wrap', wordBreak: 'break-word',
                      maxHeight: 220, overflowY: 'auto',
                    }}>
                      {systemPrompt}
                    </pre>
                  )}
                </section>
              )}
            </>
          )}

          {/* Best for / example */}
          {entry.best_for && (
            <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
              <span style={{ fontWeight: 600 }}>Best for:</span> {entry.best_for}
            </div>
          )}
          {entry.example && (
            <div style={{
              fontSize: 12, background: 'var(--surface2)',
              borderRadius: 'var(--radius-sm)', padding: '7px 11px',
              color: 'var(--text-dim)', fontStyle: 'italic',
            }}>
              "{entry.example}"
            </div>
          )}
        </div>

        {/* Spawn footer */}
        <div style={{
          borderTop: '1px solid var(--border)', padding: '14px 20px',
          display: 'flex', flexDirection: 'column', gap: 10, flexShrink: 0,
          background: 'var(--bg-elevated)',
        }}>
          {spawnError && (
            <div data-cy="spawn-error" style={{ fontSize: 12, color: 'var(--error, #f87171)' }}>
              ⚠ {spawnError}
            </div>
          )}

          {/* Preflight pending */}
          {preflightPending && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12, color: 'var(--text-dim)' }}>
              <div className="spinner" style={{ width: 12, height: 12 }} />
              Checking skill dependencies…
            </div>
          )}

          {/* Preflight blocking errors */}
          {preflightResult && !preflightResult.ok && (
            <div data-cy="preflight-error" style={{ fontSize: 12, color: 'var(--error, #f87171)', display: 'flex', flexDirection: 'column', gap: 3 }}>
              <div style={{ fontWeight: 600 }}>⛔ Preflight failed — resolve these before spawning:</div>
              {preflightResult.errors?.map((e, i) => (
                <div key={i} style={{ paddingLeft: 12 }}>• {e.skill}: {e.message}</div>
              ))}
            </div>
          )}

          {/* Preflight warnings with acknowledgement */}
          {preflightResult?.ok && preflightResult.warnings?.length > 0 && !preflightWarningsAck && (
            <div data-cy="preflight-warning" style={{ fontSize: 12, display: 'flex', flexDirection: 'column', gap: 6 }}>
              <div style={{ fontWeight: 600, color: 'var(--warning, #fbbf24)' }}>⚠ Skill warnings:</div>
              {preflightResult.warnings.map((w, i) => (
                <div key={i} style={{ paddingLeft: 12, color: 'var(--text-secondary)' }}>• {w.skill}: {w.message}</div>
              ))}
              <button
                data-cy="spawn-anyway-btn"
                className="btn btn-warning btn-sm"
                onClick={() => { setPreflightWarningsAck(true); handleSpawn(); }}
                style={{ alignSelf: 'flex-start', marginTop: 2 }}
              >
                Spawn Anyway
              </button>
            </div>
          )}

          {/* Preflight pass indicator */}
          {preflightResult?.ok && !preflightResult.warnings?.length && (
            <div style={{ fontSize: 12, color: 'var(--success, #4ade80)' }}>✓ Skill preflight passed</div>
          )}

          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <input
              ref={nameRef}
              data-cy="spawn-name-input"
              type="text"
              value={spawnName}
              onChange={e => { setSpawnName(e.target.value); setSpawnError(''); }}
              onKeyDown={e => { if (e.key === 'Enter' && !spawning) handleSpawn(); }}
              placeholder="Agent name…"
              maxLength={AGENT_NAME_MAX_LENGTH}
              disabled={spawning}
              style={{
                flex: 1, padding: '7px 11px',
                borderRadius: 'var(--radius-sm)',
                border: '1px solid var(--border)',
                background: 'var(--bg)',
                color: 'var(--text)', fontSize: 13,
                outline: 'none',
                fontFamily: 'var(--font-mono, monospace)',
              }}
            />
            <button
              data-cy="spawn-btn"
              className="btn btn-primary btn-sm"
              onClick={handleSpawn}
              disabled={spawning || preflightPending || loadingTemplate || !spawnName.trim() || (preflightResult && !preflightResult.ok)}
              style={{ whiteSpace: 'nowrap', minWidth: 110 }}
            >
              {spawning ? (
                <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                  <div className="spinner" style={{ width: 12, height: 12 }} /> Spawning…
                </span>
              ) : '▶ Spawn Agent'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export default function AgentCatalogClient({ initialEntries }) {
  const router = useRouter();
  const [entries, setEntries] = useState(initialEntries ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [toggling, setToggling] = useState({});
  const [filter, setFilter] = useState('');
  const [detailEntry, setDetailEntry] = useState(null);     // open modal
  const [spawnResult, setSpawnResult] = useState(null);     // { agentId, name }

  const openDetail = useCallback((entry) => {
    setSpawnResult(null);
    setDetailEntry(entry);
    track('detail_opened', { agent: entry.name });
  }, []);

  const closeDetail = useCallback(() => setDetailEntry(null), []);

  const handleSpawnSuccess = useCallback(({ agentId, name }) => {
    setDetailEntry(null);
    setSpawnResult({ agentId, name });
  }, []);

  const openChat = useCallback((agentId, agentName) => {
    const params = new URLSearchParams();
    if (agentId) params.set('agentId', agentId);
    if (agentName) params.set('agentName', agentName);
    router.push(`/chat?${params.toString()}`);
  }, [router]);

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
      {/* Detail modal */}
      {detailEntry && (
        <DetailModal
          entry={detailEntry}
          onClose={closeDetail}
          onSpawnSuccess={handleSpawnSuccess}
        />
      )}

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

        {/* Spawn success banner */}
        {spawnResult && (
          <SpawnSuccessBanner
            agentId={spawnResult.agentId}
            agentName={spawnResult.name}
            onDismiss={() => setSpawnResult(null)}
            onOpenChat={openChat}
          />
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
                <div style={{ minWidth: 0, flex: 1 }}>
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
                <div style={{ display: 'flex', flexDirection: 'column', gap: 5, flexShrink: 0 }}>
                  <button
                    data-cy="catalog-details-btn"
                    className="btn btn-primary btn-sm"
                    style={{ whiteSpace: 'nowrap', fontSize: 11 }}
                    onClick={() => openDetail(entry)}
                  >
                    Details / Spawn
                  </button>
                  <button
                    data-cy="catalog-toggle-btn"
                    className={`btn btn-sm ${entry.enabled ? 'btn-ghost' : 'btn-ghost'}`}
                    style={{
                      fontSize: 11,
                      color: entry.enabled ? 'var(--success)' : 'var(--text-dim)',
                      borderColor: entry.enabled ? 'var(--success)44' : 'var(--border)',
                    }}
                    onClick={() => toggleEnabled(entry.catalog_id, entry.enabled)}
                    disabled={!!toggling[entry.catalog_id]}
                  >
                    {toggling[entry.catalog_id] ? '…' : entry.enabled ? '● Enabled' : '○ Disabled'}
                  </button>
                </div>
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
