'use client';

import { MARKET_DATA_PROVIDERS, POLLING_CADENCE_OPTIONS } from '../config/market-data-providers';
import InvestmentDataInputChooser from './InvestmentDataInputChooser';

export default function InvestmentsWizardStepDataSources({
  value,
  onChange,
  onTestConnection,
  testingConnection,
  testResult,
  onBack,
  onNext,
}) {
  const { inputMethod, providers = [], apiKeyName, defaultMarket, pollingCadence, fallbackSource } = value;
  const selectedProviders = providers.filter((p) => p !== 'manual_csv');

  function toggleProvider(id) {
    const next = providers.includes(id) ? providers.filter((p) => p !== id) : [...providers, id];
    onChange({ ...value, providers: next });
  }

  const activeProvider = MARKET_DATA_PROVIDERS.find((p) => selectedProviders.includes(p.id));

  return (
    <div>
      <h3 style={{ fontSize: 16, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
        Where does the data come from?
      </h3>
      <p style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 20 }}>
        You can type in data yourself, import a CSV, or plug into a live market data source.
      </p>

      <InvestmentDataInputChooser
        value={inputMethod || 'manual'}
        onChange={(v) => onChange({ ...value, inputMethod: v })}
      />

      {inputMethod === 'csv' && (
        <div style={{ marginTop: 16, padding: '12px 14px', background: 'var(--surface)', borderRadius: 8, border: '1.5px solid var(--border)', fontSize: 12, color: 'var(--text-dim)' }}>
          Upload a CSV with columns: <code>symbol, name, sector, cost_basis, shares</code>. You&apos;ll be prompted on the next step.
        </div>
      )}

      {inputMethod === 'provider' && (
        <div style={{ marginTop: 16 }}>
          <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text)', marginBottom: 10 }}>Available providers</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 16 }}>
            {MARKET_DATA_PROVIDERS.filter((p) => p.id !== 'manual_csv').map((prov) => {
              const active = selectedProviders.includes(prov.id);
              return (
                <label
                  key={prov.id}
                  style={{
                    display: 'flex',
                    alignItems: 'flex-start',
                    gap: 10,
                    cursor: 'pointer',
                    padding: '9px 12px',
                    borderRadius: 8,
                    border: `1.5px solid ${active ? 'var(--accent)' : 'var(--border)'}`,
                    background: active ? 'rgba(255,106,26,0.05)' : 'var(--surface)',
                  }}
                >
                  <input
                    type="checkbox"
                    checked={active}
                    onChange={() => toggleProvider(prov.id)}
                    style={{ marginTop: 2, accentColor: 'var(--accent)', flexShrink: 0 }}
                  />
                  <div style={{ flex: 1 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <span style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>{prov.label}</span>
                      {prov.official && (
                        <span className="badge badge-success" style={{ fontSize: 9 }}>Official API</span>
                      )}
                      {!prov.official && (
                        <span className="badge badge-warn" style={{ fontSize: 9 }}>Unofficial</span>
                      )}
                    </div>
                    <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 2 }}>{prov.hint}</div>
                    {prov.note && (
                      <div style={{ fontSize: 10, color: 'var(--text-dim)', marginTop: 2, opacity: 0.7 }}>{prov.note}</div>
                    )}
                    {prov.docsUrl && (
                      <a href={prov.docsUrl} target="_blank" rel="noopener noreferrer" style={{ fontSize: 10, color: 'var(--accent)', marginTop: 2, display: 'inline-block' }}>
                        View docs →
                      </a>
                    )}
                  </div>
                </label>
              );
            })}
          </div>

          {activeProvider?.requiresApiKey && (
            <div style={{ marginBottom: 14 }}>
              <label style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)', display: 'block', marginBottom: 5 }}>
                API Key Name (as stored in your config)
              </label>
              <input
                type="text"
                placeholder="e.g. ALPHA_VANTAGE_API_KEY"
                value={apiKeyName || ''}
                onChange={(e) => onChange({ ...value, apiKeyName: e.target.value })}
                style={{ width: '100%', padding: '8px 11px', borderRadius: 8, border: '1.5px solid var(--border)', background: 'var(--surface)', color: 'var(--text)', fontSize: 12, boxSizing: 'border-box' }}
              />
              <div style={{ fontSize: 10, color: 'var(--text-dim)', marginTop: 3 }}>
                The environment variable key stored in your OpenFang config.
              </div>
            </div>
          )}

          <div style={{ marginBottom: 14 }}>
            <label style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)', display: 'block', marginBottom: 5 }}>
              Default market
            </label>
            <input
              type="text"
              placeholder="e.g. US, crypto, all"
              value={defaultMarket || ''}
              onChange={(e) => onChange({ ...value, defaultMarket: e.target.value })}
              style={{ width: '100%', padding: '8px 11px', borderRadius: 8, border: '1.5px solid var(--border)', background: 'var(--surface)', color: 'var(--text)', fontSize: 12, boxSizing: 'border-box' }}
            />
          </div>

          <div style={{ marginBottom: 14 }}>
            <label style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)', display: 'block', marginBottom: 5 }}>
              Polling cadence
            </label>
            <select
              value={pollingCadence || 'daily'}
              onChange={(e) => onChange({ ...value, pollingCadence: e.target.value })}
              style={{ width: '100%', padding: '8px 11px', borderRadius: 8, border: '1.5px solid var(--border)', background: 'var(--surface)', color: 'var(--text)', fontSize: 12 }}
            >
              {POLLING_CADENCE_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>

          <div style={{ marginBottom: 16 }}>
            <label style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)', display: 'block', marginBottom: 5 }}>
              Fallback data source
            </label>
            <input
              type="text"
              placeholder="e.g. manual, yahoo_finance"
              value={fallbackSource || ''}
              onChange={(e) => onChange({ ...value, fallbackSource: e.target.value })}
              style={{ width: '100%', padding: '8px 11px', borderRadius: 8, border: '1.5px solid var(--border)', background: 'var(--surface)', color: 'var(--text)', fontSize: 12, boxSizing: 'border-box' }}
            />
            <div style={{ fontSize: 10, color: 'var(--text-dim)', marginTop: 3 }}>
              Used when primary provider is unavailable.
            </div>
          </div>

          <div style={{ display: 'flex', gap: 8, marginBottom: 4 }}>
            <button
              className="btn btn-ghost btn-sm"
              onClick={onTestConnection}
              disabled={testingConnection || selectedProviders.length === 0}
            >
              {testingConnection ? (
                <span className="spinner" style={{ width: 12, height: 12, marginRight: 5 }} />
              ) : null}
              {testingConnection ? 'Testing…' : 'Test connection'}
            </button>
            <button className="btn btn-ghost btn-sm" onClick={onNext} style={{ color: 'var(--text-dim)' }}>
              Skip for now
            </button>
          </div>

          {testResult && (
            <div style={{
              marginTop: 8,
              padding: '8px 12px',
              borderRadius: 6,
              background: testResult.ok ? 'rgba(34,197,94,0.09)' : 'rgba(239,68,68,0.09)',
              fontSize: 12,
              color: testResult.ok ? '#22c55e' : '#ef4444',
            }}>
              {testResult.message}
            </div>
          )}
        </div>
      )}

      <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: 24 }}>
        <button className="btn btn-ghost" onClick={onBack}>← Back</button>
        <button className="btn btn-primary" onClick={onNext}>Next →</button>
      </div>
    </div>
  );
}
