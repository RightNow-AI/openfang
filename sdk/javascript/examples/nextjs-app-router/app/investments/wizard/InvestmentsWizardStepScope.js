'use client';

import { WATCH_SCOPE_LABELS, WATCH_SCOPE_DESCRIPTIONS, TIME_HORIZON_LABELS, TIME_HORIZON_DESCRIPTIONS, RISK_COMFORT_LABELS, RISK_COMFORT_DESCRIPTIONS } from '../lib/investments-copy';

const SCOPE_OPTIONS = ['stock', 'etf', 'crypto', 'sectors', 'themes', 'mixed'];
const HORIZON_OPTIONS = ['short', 'medium', 'long'];
const RISK_OPTIONS = ['low', 'medium', 'high'];

export default function InvestmentsWizardStepScope({ value, onChange, onNext }) {
  const { watchScope, symbols, timeHorizon, riskComfort } = value;
  const canNext = watchScope && symbols && symbols.trim().length > 0 && timeHorizon && riskComfort;

  return (
    <div>
      <h3 style={{ fontSize: 16, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
        What do you want to watch?
      </h3>
      <p style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 20 }}>
        Pick what you want to track. You can add more later.
      </p>

      {/* Watch type grid */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 8, marginBottom: 20 }}>
        {SCOPE_OPTIONS.map((opt) => {
          const active = watchScope === opt;
          return (
            <button
              key={opt}
              type="button"
              onClick={() => onChange({ ...value, watchScope: opt })}
              style={{
                padding: '10px 8px',
                borderRadius: 8,
                border: `2px solid ${active ? 'var(--accent)' : 'var(--border)'}`,
                background: active ? 'rgba(255,106,26,0.07)' : 'var(--surface)',
                cursor: 'pointer',
                textAlign: 'center',
              }}
            >
              <div style={{ fontSize: 12, fontWeight: 700, color: active ? 'var(--accent)' : 'var(--text)', marginBottom: 2 }}>
                {WATCH_SCOPE_LABELS[opt]}
              </div>
              <div style={{ fontSize: 10, color: 'var(--text-dim)' }}>
                {WATCH_SCOPE_DESCRIPTIONS[opt]}
              </div>
            </button>
          );
        })}
      </div>

      {/* Symbols */}
      <div style={{ marginBottom: 20 }}>
        <label style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)', display: 'block', marginBottom: 6 }}>
          What symbols, sectors, or themes?
        </label>
        <textarea
          placeholder="AAPL, NVDA, BTC, energy sector, AI themes..."
          value={symbols || ''}
          onChange={(e) => onChange({ ...value, symbols: e.target.value })}
          rows={3}
          style={{
            width: '100%',
            padding: '9px 12px',
            borderRadius: 8,
            border: '1.5px solid var(--border)',
            background: 'var(--surface)',
            color: 'var(--text)',
            fontSize: 12,
            resize: 'vertical',
            boxSizing: 'border-box',
          }}
        />
        <div style={{ fontSize: 10, color: 'var(--text-dim)', marginTop: 3 }}>
          Comma-separated. Type anything — agent will figure out the rest.
        </div>
      </div>

      {/* Time horizon */}
      <div style={{ marginBottom: 20 }}>
        <label style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)', display: 'block', marginBottom: 6 }}>
          How long is your time horizon?
        </label>
        <div style={{ display: 'flex', gap: 8 }}>
          {HORIZON_OPTIONS.map((opt) => {
            const active = timeHorizon === opt;
            return (
              <button
                key={opt}
                type="button"
                onClick={() => onChange({ ...value, timeHorizon: opt })}
                style={{
                  flex: 1,
                  padding: '9px 6px',
                  borderRadius: 8,
                  border: `2px solid ${active ? 'var(--accent)' : 'var(--border)'}`,
                  background: active ? 'rgba(255,106,26,0.07)' : 'var(--surface)',
                  cursor: 'pointer',
                  textAlign: 'center',
                }}
              >
                <div style={{ fontSize: 12, fontWeight: 700, color: active ? 'var(--accent)' : 'var(--text)', marginBottom: 1 }}>
                  {TIME_HORIZON_LABELS[opt]}
                </div>
                <div style={{ fontSize: 10, color: 'var(--text-dim)' }}>{TIME_HORIZON_DESCRIPTIONS[opt]}</div>
              </button>
            );
          })}
        </div>
      </div>

      {/* Risk comfort */}
      <div style={{ marginBottom: 24 }}>
        <label style={{ fontSize: 12, fontWeight: 600, color: 'var(--text)', display: 'block', marginBottom: 6 }}>
          How much risk are you comfortable watching?
        </label>
        <div style={{ display: 'flex', gap: 8 }}>
          {RISK_OPTIONS.map((opt) => {
            const active = riskComfort === opt;
            const colors = { low: '#22c55e', medium: '#f59e0b', high: '#ef4444' };
            return (
              <button
                key={opt}
                type="button"
                onClick={() => onChange({ ...value, riskComfort: opt })}
                style={{
                  flex: 1,
                  padding: '9px 6px',
                  borderRadius: 8,
                  border: `2px solid ${active ? colors[opt] : 'var(--border)'}`,
                  background: active ? `${colors[opt]}11` : 'var(--surface)',
                  cursor: 'pointer',
                  textAlign: 'center',
                }}
              >
                <div style={{ fontSize: 12, fontWeight: 700, color: active ? colors[opt] : 'var(--text)', marginBottom: 1 }}>
                  {RISK_COMFORT_LABELS[opt]}
                </div>
                <div style={{ fontSize: 10, color: 'var(--text-dim)' }}>{RISK_COMFORT_DESCRIPTIONS[opt]}</div>
              </button>
            );
          })}
        </div>
      </div>

      <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
        <button className="btn btn-primary" onClick={onNext} disabled={!canNext}>
          Next →
        </button>
      </div>
    </div>
  );
}
