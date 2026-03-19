'use client';

import { impactBadgeClass } from '../lib/investments-ui';

export default function ResearchSummaryCard({ research, onOpenThesis }) {
  return (
    <div className="card" data-cy={`research-${research.id}`}>
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 10 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 4, flexWrap: 'wrap' }}>
            {research.symbol && (
              <span style={{ fontFamily: 'monospace', fontWeight: 800, fontSize: 13, color: 'var(--text)' }}>
                {research.symbol}
              </span>
            )}
            <span className={`badge ${impactBadgeClass(research.impact)}`} style={{ fontSize: 10 }}>
              {research.impact?.toUpperCase() || 'UNKNOWN'} impact
            </span>
          </div>
          <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)', marginBottom: 4 }}>
            {research.title}
          </div>
          <p style={{
            fontSize: 12,
            color: 'var(--text-dim)',
            display: '-webkit-box',
            WebkitLineClamp: 2,
            WebkitBoxOrient: 'vertical',
            overflow: 'hidden',
            marginBottom: 8,
          }}>
            {research.summary}
          </p>
          <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginBottom: 6 }}>
            {(research.sources || []).map((s) => (
              <span key={s} className="badge badge-dim" style={{ fontSize: 9, textTransform: 'uppercase' }}>{s}</span>
            ))}
          </div>
          {research.generated_at && (
            <div style={{ fontSize: 10, color: 'var(--text-dim)', opacity: 0.6 }}>
              {new Date(research.generated_at).toLocaleDateString()}
            </div>
          )}
        </div>
      </div>
      <div style={{ marginTop: 10, textAlign: 'right' }}>
        <button className="btn btn-ghost btn-sm" onClick={() => onOpenThesis(research.id)}>
          Open thesis →
        </button>
      </div>
    </div>
  );
}
