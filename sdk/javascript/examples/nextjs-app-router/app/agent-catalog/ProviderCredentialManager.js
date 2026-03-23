'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { apiClient } from '../../lib/api-client';
import { getProviderMeta, PROVIDER_DIRECTORY } from '../lib/provider-directory';

export default function ProviderCredentialManager({
  workspaceId,
  providerId: fixedProviderId,
  allowProviderSelect = false,
  title = 'Provider Vault',
  description = '',
  compact = false,
  connectedLabel = 'Connected',
  onChange = undefined,
  statusEntries = undefined,
  onStatusRefresh = undefined,
}) {
  const remoteProviders = useMemo(
    () => PROVIDER_DIRECTORY.filter((provider) => provider.requiresApiKey),
    [],
  );
  const [selectedId, setSelectedId] = useState(fixedProviderId || remoteProviders[0]?.id || 'openai');
  const [apiKey, setApiKey] = useState('');
  const [credentials, setCredentials] = useState([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const [testMetadata, setTestMetadata] = useState(null);

  useEffect(() => {
    if (fixedProviderId) {
      setSelectedId(fixedProviderId);
    }
  }, [fixedProviderId]);

  useEffect(() => {
    setTestMetadata(null);
    setMessage('');
    setError('');
  }, [selectedId, workspaceId]);

  const refreshCredentials = useCallback(async () => {
    if (Array.isArray(statusEntries)) {
      setCredentials(statusEntries);
      setLoading(false);
      return;
    }

    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get(`/api/setup/agent-keys?workspaceId=${encodeURIComponent(workspaceId)}`);
      setCredentials(Array.isArray(data) ? data : []);
    } catch (err) {
      setError(err?.message || 'Could not load provider credentials.');
    } finally {
      setLoading(false);
    }
  }, [statusEntries, workspaceId]);

  useEffect(() => {
    refreshCredentials();
  }, [refreshCredentials]);

  const provider = getProviderMeta(selectedId) ?? remoteProviders[0] ?? null;
  const currentCredential = credentials.find((entry) => entry.providerId === selectedId) ?? null;

  async function notifyChange() {
    await onStatusRefresh?.();
    await refreshCredentials();
    await onChange?.();
  }

  async function handleSave(event) {
    event?.preventDefault?.();
    setSaving(true);
    setError('');
    setMessage('');
    setTestMetadata(null);
    try {
      await apiClient.post('/api/setup/agent-keys', {
        workspaceId,
        providerId: selectedId,
        apiKey,
      });
      setApiKey('');
      setMessage(`${provider?.name ?? selectedId} key saved.`);
      await notifyChange();
    } catch (err) {
      setError(err?.message || 'Could not save provider key.');
    } finally {
      setSaving(false);
    }
  }

  async function handleTest() {
    setTesting(true);
    setError('');
    setMessage('');
    setTestMetadata(null);
    try {
      const result = await apiClient.post('/api/setup/agent-keys/test', {
        workspaceId,
        providerId: selectedId,
      });
      setMessage(result?.message || `${provider?.name ?? selectedId} connection succeeded.`);
      setTestMetadata(result?.metadata || null);
    } catch (err) {
      setError(err?.message || 'Connection test failed.');
    } finally {
      setTesting(false);
    }
  }

  async function handleDelete() {
    setDeleting(true);
    setError('');
    setMessage('');
    setTestMetadata(null);
    try {
      await apiClient.del('/api/setup/agent-keys', {
        workspaceId,
        providerId: selectedId,
      });
      setMessage(`${provider?.name ?? selectedId} key deleted.`);
      await notifyChange();
    } catch (err) {
      setError(err?.message || 'Could not delete provider key.');
    } finally {
      setDeleting(false);
    }
  }

  return (
    <section
      style={{
        padding: compact ? '14px 16px' : '18px 20px',
        borderRadius: compact ? 12 : 16,
        border: '1px solid var(--border)',
        background: 'var(--bg-elevated)',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, alignItems: 'flex-start', marginBottom: 14, flexWrap: 'wrap' }}>
        <div>
          <div style={{ fontSize: compact ? 15 : 17, fontWeight: 800, marginBottom: 4 }}>{title}</div>
          {description ? <div style={{ fontSize: 13, color: 'var(--text-dim)', maxWidth: 720 }}>{description}</div> : null}
        </div>
        <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
          Scope: <span style={{ fontFamily: 'var(--font-mono,monospace)', color: 'var(--accent)' }}>{workspaceId}</span>
        </div>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: compact ? '1fr' : 'minmax(260px, 360px) minmax(0, 1fr)', gap: 16 }}>
        <form onSubmit={handleSave} style={{ display: 'grid', gap: 12 }}>
          {allowProviderSelect ? (
            <label style={{ display: 'grid', gap: 6 }}>
              <span style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em' }}>Provider</span>
              <select
                value={selectedId}
                onChange={(event) => setSelectedId(event.target.value)}
                style={{ padding: '10px 12px', borderRadius: 10, border: '1px solid var(--border)', background: 'var(--surface2)', color: 'var(--text)', fontSize: 14 }}
              >
                {remoteProviders.map((item) => (
                  <option key={item.id} value={item.id}>{item.icon} {item.name}</option>
                ))}
              </select>
            </label>
          ) : (
            <div style={{ fontSize: 13, color: 'var(--text)' }}>
              <strong>{provider?.icon ? `${provider.icon} ${provider.name}` : selectedId}</strong>
            </div>
          )}

          <label style={{ display: 'grid', gap: 6 }}>
            <span style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em' }}>API key</span>
            <input
              type="password"
              value={apiKey}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder={provider?.keyPrefix ? `Starts with ${provider.keyPrefix}` : 'Paste provider API key'}
              style={{ padding: '10px 12px', borderRadius: 10, border: '1px solid var(--border)', background: 'var(--surface2)', color: 'var(--text)', fontSize: 14 }}
            />
          </label>

          <div style={{ fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.6 }}>
            {provider?.description}
            {provider?.keyUrl ? (
              <>
                {' '}
                <a href={provider.keyUrl} target="_blank" rel="noreferrer" style={{ color: 'var(--accent)' }}>Get key</a>
              </>
            ) : null}
          </div>

          {currentCredential ? (
            <div style={{ fontSize: 12, color: '#34d399' }}>
              {connectedLabel}: ••••{currentCredential.last4} updated {new Date(currentCredential.updatedAt).toLocaleString()}
            </div>
          ) : (
            <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>No saved key for this provider yet.</div>
          )}

          {error ? <div style={{ fontSize: 12, color: 'var(--error,#ef4444)' }}>{error}</div> : null}
          {message ? <div style={{ fontSize: 12, color: '#34d399' }}>{message}</div> : null}
          {testMetadata ? (
            <div style={{ padding: '10px 12px', borderRadius: 10, background: 'rgba(52,211,153,.08)', border: '1px solid rgba(52,211,153,.18)', fontSize: 12, color: 'var(--text)' }}>
              <div style={{ fontWeight: 700, marginBottom: 6 }}>Connection details</div>
              <div style={{ display: 'grid', gap: 4, color: 'var(--text-dim)' }}>
                {typeof testMetadata.endpoint === 'string' ? <div>Endpoint: {testMetadata.endpoint}</div> : null}
                {typeof testMetadata.modelCount === 'number' ? <div>Models visible: {testMetadata.modelCount}</div> : null}
                {typeof testMetadata.taskCount === 'number' ? <div>Recent tasks visible: {testMetadata.taskCount}</div> : null}
                {Array.isArray(testMetadata.sampleModels) && testMetadata.sampleModels.length > 0 ? <div>Examples: {testMetadata.sampleModels.join(', ')}</div> : null}
              </div>
            </div>
          ) : null}

          <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
            <button
              type="submit"
              disabled={saving || !apiKey.trim()}
              style={{
                padding: '10px 16px',
                borderRadius: 10,
                border: 'none',
                background: 'var(--accent)',
                color: '#fff',
                fontWeight: 700,
                cursor: saving ? 'progress' : 'pointer',
                opacity: saving || !apiKey.trim() ? 0.7 : 1,
              }}
            >
              {saving ? 'Saving…' : currentCredential ? 'Update key' : 'Save key'}
            </button>
            <button
              type="button"
              onClick={handleTest}
              disabled={!currentCredential || testing}
              style={{
                padding: '10px 16px',
                borderRadius: 10,
                border: '1px solid var(--border)',
                background: 'transparent',
                color: 'var(--text)',
                fontWeight: 700,
                cursor: !currentCredential || testing ? 'not-allowed' : 'pointer',
                opacity: !currentCredential || testing ? 0.7 : 1,
              }}
            >
              {testing ? 'Testing…' : 'Test connection'}
            </button>
            <button
              type="button"
              onClick={handleDelete}
              disabled={!currentCredential || deleting}
              style={{
                padding: '10px 16px',
                borderRadius: 10,
                border: '1px solid rgba(239,68,68,.28)',
                background: 'transparent',
                color: 'var(--error,#ef4444)',
                fontWeight: 700,
                cursor: !currentCredential || deleting ? 'not-allowed' : 'pointer',
                opacity: !currentCredential || deleting ? 0.7 : 1,
              }}
            >
              {deleting ? 'Deleting…' : 'Delete key'}
            </button>
          </div>
        </form>

        <div style={{ display: 'grid', gap: 10, alignContent: 'start' }}>
          <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em' }}>Masked provider status</div>
          {loading ? (
            <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>Loading provider status…</div>
          ) : credentials.length === 0 ? (
            <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>No provider keys saved yet.</div>
          ) : (
            credentials.map((entry) => {
              const meta = getProviderMeta(entry.providerId);
              return (
                <div
                  key={entry.providerId}
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    gap: 12,
                    alignItems: 'center',
                    padding: '10px 12px',
                    borderRadius: 10,
                    border: '1px solid var(--border)',
                    background: 'var(--surface2)',
                  }}
                >
                  <div>
                    <div style={{ fontSize: 13, fontWeight: 700 }}>{meta?.icon ? `${meta.icon} ${meta.name}` : entry.providerId}</div>
                    <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>Updated {new Date(entry.updatedAt).toLocaleString()}</div>
                  </div>
                  <div style={{ fontSize: 12, color: '#34d399', fontFamily: 'var(--font-mono,monospace)' }}>••••{entry.last4}</div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </section>
  );
}
