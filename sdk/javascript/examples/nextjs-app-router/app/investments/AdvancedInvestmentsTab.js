'use client';

import { MARKET_DATA_PROVIDERS } from './config/market-data-providers';

export default function AdvancedInvestmentsTab({ onOpenWizard }) {
  return (
    <div>
      <div style={{ marginBottom: 24 }}>
        <h2 style={{ fontSize: 15, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
          Advanced configuration
        </h2>
        <p style={{ fontSize: 12, color: 'var(--text-dim)' }}>
          Connect data providers, update your setup, or review integration options.
        </p>
      </div>

      {/* Provider cards */}
      <div style={{ marginBottom: 24 }}>
        <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text)', marginBottom: 10, textTransform: 'uppercase', letterSpacing: 0.5 }}>
          Market data providers
        </div>
        <div className="grid grid-2" style={{ gap: 10 }}>
          {MARKET_DATA_PROVIDERS.filter((p) => p.id !== 'manual_csv' && p.id !== 'other').map((prov) => (
            <div key={prov.id} className="card" style={{ padding: '12px 14px' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginBottom: 5 }}>
                <span style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>{prov.label}</span>
                {prov.official ? (
                  <span className="badge badge-success" style={{ fontSize: 9 }}>Official</span>
                ) : (
                  <span className="badge badge-warn" style={{ fontSize: 9 }}>Unofficial</span>
                )}
              </div>
              <p style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 6 }}>{prov.hint}</p>
              {prov.note && (
                <p style={{ fontSize: 10, color: 'var(--text-dim)', opacity: 0.7, marginBottom: 6 }}>{prov.note}</p>
              )}
              {prov.docsUrl && (
                <a
                  href={prov.docsUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  style={{ fontSize: 11, color: 'var(--accent)' }}
                >
                  View docs →
                </a>
              )}
            </div>
          ))}
        </div>
      </div>

      {/* Re-run wizard */}
      <div className="card" style={{ padding: '16px 18px', marginBottom: 20 }}>
        <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)', marginBottom: 5 }}>
          Update your investment setup
        </div>
        <p style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 12 }}>
          Change what you track, update approval rules, or switch data providers.
        </p>
        <button className="btn btn-ghost btn-sm" onClick={onOpenWizard}>
          Re-run setup wizard
        </button>
      </div>

      {/* Integration notes */}
      <div style={{ fontSize: 11, color: 'var(--text-dim)', lineHeight: 1.7 }}>
        <strong style={{ color: 'var(--text)' }}>Integrations:</strong> Connect to Scheduler to run daily market briefs automatically. Link to Workflows to trigger approval queues on signal fires. Use Comms to get alerts via email or webhook.
      </div>
    </div>
  );
}
