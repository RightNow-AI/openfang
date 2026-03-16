'use client';

import { useState, useEffect, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

// ── Provider metadata ───────────────────────────────────────────────────────
const PROVIDERS = [
  {
    id: 'openai',
    name: 'OpenAI',
    icon: '🟢',
    description: 'GPT-4o, GPT-4.1 and o-series reasoning models',
    keyUrl: 'https://platform.openai.com/api-keys',
    keyPrefix: 'sk-',
    defaultModel: 'gpt-4o',
    defaultUrl: 'https://api.openai.com/v1',
    needsUrl: false,
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    icon: '🔴',
    description: 'Claude 3.5 / 4 — strongest reasoning & safety',
    keyUrl: 'https://console.anthropic.com/settings/keys',
    keyPrefix: 'sk-ant-',
    defaultModel: 'claude-sonnet-4-20250514',
    defaultUrl: 'https://api.anthropic.com',
    needsUrl: false,
  },
  {
    id: 'gemini',
    name: 'Google Gemini',
    icon: '🔵',
    description: 'Gemini 2.5 Flash and Pro — long context, multimodal',
    keyUrl: 'https://aistudio.google.com/app/apikey',
    keyPrefix: 'AIza',
    defaultModel: 'gemini-2.5-flash',
    defaultUrl: 'https://generativelanguage.googleapis.com',
    needsUrl: false,
  },
  {
    id: 'groq',
    name: 'Groq',
    icon: '⚡',
    description: 'Llama 3.3/4 at ~800 tok/s — fastest inference',
    keyUrl: 'https://console.groq.com/keys',
    keyPrefix: 'gsk_',
    defaultModel: 'llama-3.3-70b-versatile',
    defaultUrl: 'https://api.groq.com/openai/v1',
    needsUrl: false,
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    icon: '🌐',
    description: '100+ models via one API — free tier available',
    keyUrl: 'https://openrouter.ai/keys',
    keyPrefix: 'sk-or-',
    defaultModel: 'openrouter/auto',
    defaultUrl: 'https://openrouter.ai/api/v1',
    needsUrl: false,
  },
  {
    id: 'xai',
    name: 'xAI Grok',
    icon: '🤖',
    description: 'Grok 3 — powerful reasoning from xAI',
    keyUrl: 'https://console.x.ai/',
    keyPrefix: 'xai-',
    defaultModel: 'grok-3-fast',
    defaultUrl: 'https://api.x.ai/v1',
    needsUrl: false,
  },
  {
    id: 'minimax',
    name: 'MiniMax',
    icon: '🧠',
    description: 'MiniMax M2.5 — multimodal, 1M token context',
    keyUrl: 'https://www.minimaxi.com/',
    keyPrefix: null,
    defaultModel: 'MiniMax-M2.5',
    defaultUrl: 'https://api.minimax.io/v1',
    needsUrl: false,
  },
  {
    id: 'ollama',
    name: 'Ollama (local)',
    icon: '🏠',
    description: 'Run models locally — no API key needed',
    keyUrl: null,
    keyPrefix: null,
    defaultModel: 'llama3.2',
    defaultUrl: 'http://127.0.0.1:11434/v1',
    needsUrl: true,
  },
];

// ── Field group style helpers ───────────────────────────────────────────────
const fieldLabel = {
  fontSize: 12,
  fontWeight: 600,
  color: 'var(--text-dim)',
  marginBottom: 5,
  textTransform: 'uppercase',
  letterSpacing: '0.5px',
  display: 'block',
};

const fieldInput = {
  width: '100%',
  padding: '9px 12px',
  background: 'var(--surface)',
  border: '1px solid var(--border)',
  borderRadius: 'var(--radius-sm)',
  color: 'var(--text)',
  fontSize: 13,
  fontFamily: 'inherit',
  boxSizing: 'border-box',
};

function StatusBadge({ status }) {
  const map = {
    configured:   { label: 'Connected',      color: 'var(--success)',  bg: 'var(--success-subtle)' },
    not_required: { label: 'Local / no key', color: 'var(--accent)',   bg: 'var(--accent-subtle)'  },
    missing:      { label: 'Not configured', color: 'var(--text-dim)', bg: 'var(--surface2)'        },
    error:        { label: 'Error',          color: 'var(--error)',    bg: 'var(--error-subtle)'    },
    ok:           { label: 'Connection OK',  color: 'var(--success)',  bg: 'var(--success-subtle)' },
    testing:      { label: 'Testing…',       color: 'var(--warning)',  bg: 'var(--warning-subtle)' },
  };
  const s = map[status] ?? map.missing;
  return (
    <span style={{
      fontSize: 11, fontWeight: 700, padding: '3px 8px',
      borderRadius: 20, background: s.bg, color: s.color,
      textTransform: 'uppercase', letterSpacing: '0.5px',
    }}>
      {s.label}
    </span>
  );
}

// ── Provider Settings Panel ─────────────────────────────────────────────────
function ProviderPanel({ current, allProviders, onSaved }) {
  const pMeta = PROVIDERS.find(p => p.id === current?.provider) ?? PROVIDERS[0];

  const [selectedId, setSelectedId] = useState(current?.provider ?? 'anthropic');
  const [apiKey, setApiKey] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [baseUrl, setBaseUrl] = useState('');
  const [defaultModel, setDefaultModel] = useState('');
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [saveMsg, setSaveMsg] = useState(null);
  const [testResult, setTestResult] = useState(null);

  // Populate fields when provider selection changes
  useEffect(() => {
    const meta = PROVIDERS.find(p => p.id === selectedId);
    if (!meta) return;
    setApiKey('');
    setTestResult(null);
    setSaveMsg(null);
    // Pre-fill URL from catalog list or fallback
    const fromCatalog = allProviders?.find(p => p.id === selectedId);
    setBaseUrl(fromCatalog?.base_url ?? meta.defaultUrl);
    // Pre-fill model: use current if same provider, else use catalog default
    if (selectedId === current?.provider && current?.model) {
      setDefaultModel(current.model);
    } else {
      setDefaultModel(meta.defaultModel);
    }
  }, [selectedId, current, allProviders]);

  const meta = PROVIDERS.find(p => p.id === selectedId) ?? pMeta;
  const catalogEntry = allProviders?.find(p => p.id === selectedId);
  const authStatus = selectedId === current?.provider
    ? (current?.api_key_configured ? 'configured' : (meta.keyPrefix ? 'missing' : 'not_required'))
    : (catalogEntry?.auth_status ?? 'missing');

  async function handleSave() {
    setSaving(true);
    setSaveMsg(null);
    try {
      const body = { provider: selectedId, default_model: defaultModel, base_url: baseUrl };
      if (apiKey.trim()) body.api_key = apiKey.trim();
      const res = await apiClient.put('/api/settings/providers/current', body);
      setSaveMsg({ type: 'success', text: `Saved. Restart the daemon to apply the new provider.` });
      onSaved?.();
    } catch (e) {
      setSaveMsg({ type: 'error', text: `Save failed: ${e.message}` });
    }
    setSaving(false);
  }

  async function handleTest() {
    setTesting(true);
    setTestResult(null);
    // Save first (with key if provided) then test
    try {
      const body = { provider: selectedId, default_model: defaultModel, base_url: baseUrl };
      if (apiKey.trim()) body.api_key = apiKey.trim();
      await apiClient.put('/api/settings/providers/current', body);
    } catch (e) {
      setTestResult({ status: 'error', error: `Could not save before test: ${e.message}` });
      setTesting(false);
      return;
    }
    try {
      const res = await apiClient.post(`/api/providers/${selectedId}/test`, {});
      setTestResult(res);
    } catch (e) {
      setTestResult({ status: 'error', error: e.message });
    }
    setTesting(false);
  }

  return (
    <div>
      {/* Provider selector */}
      <div style={{ marginBottom: 20 }}>
        <label style={fieldLabel}>AI Provider</label>
        <select
          value={selectedId}
          onChange={e => setSelectedId(e.target.value)}
          style={{ ...fieldInput, cursor: 'pointer' }}
        >
          {PROVIDERS.map(p => (
            <option key={p.id} value={p.id}>
              {p.icon}  {p.name}
            </option>
          ))}
        </select>
        <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 5, lineHeight: 1.5 }}>
          {meta.description}
        </div>
      </div>

      {/* Status badge row */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 20 }}>
        <span style={{ fontSize: 12, color: 'var(--text-dim)' }}>Current status:</span>
        <StatusBadge status={selectedId === current?.provider
          ? (current.api_key_configured ? 'configured' : (meta.keyPrefix ? 'missing' : 'not_required'))
          : (catalogEntry?.auth_status ?? 'missing')} />
        {selectedId === current?.provider && (
          <span style={{ fontSize: 11, color: 'var(--text-muted)', marginLeft: 'auto' }}>
            Active provider
          </span>
        )}
      </div>

      {/* API key field (hidden for local providers) */}
      {meta.keyPrefix && (
        <div style={{ marginBottom: 18 }}>
          <label style={fieldLabel}>
            API Key
            {meta.keyUrl && (
              <a
                href={meta.keyUrl}
                target="_blank"
                rel="noreferrer"
                style={{ marginLeft: 8, fontWeight: 400, color: 'var(--accent)', textTransform: 'none', letterSpacing: 0 }}
              >
                Get a key ↗
              </a>
            )}
          </label>
          <div style={{ position: 'relative' }}>
            <input
              type={showKey ? 'text' : 'password'}
              value={apiKey}
              onChange={e => setApiKey(e.target.value)}
              placeholder={
                selectedId === current?.provider && current?.api_key_configured
                  ? '••••••••  (key saved — paste new key to replace)'
                  : `Paste your ${meta.name} key (${meta.keyPrefix}…)`
              }
              style={{ ...fieldInput, paddingRight: 56 }}
              autoComplete="off"
              spellCheck={false}
            />
            <button
              onClick={() => setShowKey(v => !v)}
              style={{
                position: 'absolute', right: 10, top: '50%', transform: 'translateY(-50%)',
                background: 'none', border: 'none', cursor: 'pointer',
                fontSize: 16, color: 'var(--text-dim)', padding: 4,
              }}
              title={showKey ? 'Hide key' : 'Show key'}
              type="button"
            >
              {showKey ? '🙈' : '👁'}
            </button>
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4, lineHeight: 1.5 }}>
            Stored in <code>~/.openfang/secrets.env</code> — never committed to the repo.
          </div>
        </div>
      )}

      {/* Base URL (always shown for local providers, collapsible for cloud) */}
      {(meta.needsUrl || true) && (
        <div style={{ marginBottom: 18 }}>
          <label style={fieldLabel}>Base URL</label>
          <input
            type="url"
            value={baseUrl}
            onChange={e => setBaseUrl(e.target.value)}
            placeholder={meta.defaultUrl}
            style={fieldInput}
          />
          {meta.needsUrl && (
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>
              Update this if Ollama is running on a different host or port.
            </div>
          )}
        </div>
      )}

      {/* Default model */}
      <div style={{ marginBottom: 24 }}>
        <label style={fieldLabel}>Default Model</label>
        <input
          type="text"
          value={defaultModel}
          onChange={e => setDefaultModel(e.target.value)}
          placeholder={meta.defaultModel}
          style={fieldInput}
        />
        <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>
          Used when an agent doesn't specify a model explicitly.
        </div>
      </div>

      {/* Action buttons */}
      <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', alignItems: 'center' }}>
        <button
          className="btn btn-primary btn-sm"
          onClick={handleSave}
          disabled={saving || testing}
        >
          {saving ? 'Saving…' : '💾 Save settings'}
        </button>
        <button
          className="btn btn-ghost btn-sm"
          onClick={handleTest}
          disabled={testing || saving}
        >
          {testing ? '⏳ Testing…' : '🔌 Test connection'}
        </button>
      </div>

      {/* Feedback messages */}
      {saveMsg && (
        <div style={{
          marginTop: 14,
          padding: '10px 14px',
          borderRadius: 'var(--radius-sm)',
          background: saveMsg.type === 'success' ? 'var(--success-subtle)' : 'var(--error-subtle)',
          border: `1px solid ${saveMsg.type === 'success' ? 'var(--success)' : 'var(--error)'}`,
          fontSize: 13, color: 'var(--text)', lineHeight: 1.6,
        }}>
          {saveMsg.type === 'success' ? '✅ ' : '❌ '}{saveMsg.text}
        </div>
      )}

      {testResult && (
        <div style={{
          marginTop: 10,
          padding: '10px 14px',
          borderRadius: 'var(--radius-sm)',
          background: testResult.status === 'ok' ? 'var(--success-subtle)' : 'var(--error-subtle)',
          border: `1px solid ${testResult.status === 'ok' ? 'var(--success)' : 'var(--error)'}`,
          fontSize: 13, lineHeight: 1.6,
        }}>
          {testResult.status === 'ok'
            ? `✅ Connection OK — latency ${testResult.latency_ms}ms`
            : `❌ Test failed: ${testResult.error ?? 'Unknown error'}`}
        </div>
      )}
    </div>
  );
}

// ── Main page ───────────────────────────────────────────────────────────────
export default function SettingsPage() {
  const [current, setCurrent] = useState(null);
  const [allProviders, setAllProviders] = useState([]);
  const [config, setConfig] = useState(null);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState('providers');

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [cur, rawProviders, cfg] = await Promise.all([
        apiClient.get('/api/settings/providers/current').catch(() => null),
        apiClient.get('/api/providers').catch(() => []),
        apiClient.get('/api/config').catch(() => null),
      ]);
      setCurrent(cur);
      const providers = Array.isArray(rawProviders) ? rawProviders : (rawProviders?.providers ?? []);
      setAllProviders(providers);
      setConfig(cfg);
    } catch {
      // handled per-request above
    }
    setLoading(false);
  }, []);

  useEffect(() => { load(); }, [load]);

  const tabs = [
    { id: 'providers', label: '🔌 Providers' },
    { id: 'config',    label: '📄 Config file' },
    { id: 'links',     label: '🔗 Quick links' },
  ];

  return (
    <div data-cy="settings-page">
      <div className="page-header">
        <h1>Settings</h1>
        <button className="btn btn-ghost btn-sm" onClick={load} disabled={loading}>
          {loading ? '⏳' : '↺'} Refresh
        </button>
      </div>

      {/* Tab bar */}
      <div style={{ display: 'flex', gap: 4, padding: '0 24px', borderBottom: '1px solid var(--border)', marginBottom: 24 }}>
        {tabs.map(t => (
          <button
            key={t.id}
            onClick={() => setActiveTab(t.id)}
            style={{
              padding: '10px 16px',
              fontSize: 13, fontWeight: activeTab === t.id ? 700 : 400,
              color: activeTab === t.id ? 'var(--accent)' : 'var(--text-dim)',
              background: 'none', border: 'none', cursor: 'pointer',
              borderBottom: activeTab === t.id ? '2px solid var(--accent)' : '2px solid transparent',
              marginBottom: -1,
            }}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div className="page-body">
        {loading && <div className="loading-state"><div className="spinner" /></div>}

        {!loading && activeTab === 'providers' && (
          <div className="card" style={{ maxWidth: 600 }}>
            <div className="card-header" style={{ marginBottom: 20 }}>
              LLM Provider
              {current && (
                <span style={{ marginLeft: 10, fontSize: 12, fontWeight: 400, color: 'var(--text-dim)' }}>
                  Active: <strong style={{ color: 'var(--text)' }}>{current.display_name ?? current.provider}</strong>
                  {' · '}{current.model}
                </span>
              )}
            </div>
            <ProviderPanel
              current={current}
              allProviders={allProviders}
              onSaved={load}
            />
          </div>
        )}

        {!loading && activeTab === 'config' && (
          <div className="card">
            <div className="card-header" style={{ marginBottom: 8 }}>Active configuration</div>
            <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '0 0 12px' }}>
              Loaded from <code>~/.openfang/config.toml</code>. Secrets are redacted.
            </p>
            {config ? (
              <pre style={{ fontSize: 12, overflow: 'auto', margin: 0, color: 'var(--text-dim)', background: 'var(--surface2)', padding: 12, borderRadius: 6 }}>
                {JSON.stringify(config, null, 2)}
              </pre>
            ) : (
              <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>Config could not be loaded.</p>
            )}
          </div>
        )}

        {!loading && activeTab === 'links' && (
          <div>
            <div className="grid grid-3" style={{ gap: 16 }}>
              <a href="/agent-catalog" style={{ textDecoration: 'none' }} className="card">
                <div className="card-header">Agent catalog</div>
                <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: 0 }}>Browse and configure agents</p>
              </a>
              <a href="/channels" style={{ textDecoration: 'none' }} className="card">
                <div className="card-header">Channels</div>
                <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: 0 }}>Connect WhatsApp, Slack, and more</p>
              </a>
              <a href="/skills" style={{ textDecoration: 'none' }} className="card">
                <div className="card-header">Skills</div>
                <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: 0 }}>View available agent skills</p>
              </a>
              <a href="/onboarding" style={{ textDecoration: 'none' }} className="card">
                <div className="card-header">Onboarding wizard</div>
                <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: 0 }}>Walk through first-time setup again</p>
              </a>
              <a
                href="https://github.com/RightNow-AI/openfang/blob/main/docs/configuration.md"
                target="_blank" rel="noreferrer"
                style={{ textDecoration: 'none' }}
                className="card"
              >
                <div className="card-header">Config docs ↗</div>
                <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: 0 }}>Full configuration reference</p>
              </a>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
