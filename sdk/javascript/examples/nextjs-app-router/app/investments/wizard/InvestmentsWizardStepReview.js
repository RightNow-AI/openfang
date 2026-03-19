'use client';

function Row({ label, value }) {
  return (
    <div style={{ display: 'flex', gap: 10, paddingBottom: 10, borderBottom: '1px solid var(--border)', marginBottom: 10 }}>
      <div style={{ width: 110, flexShrink: 0, fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', paddingTop: 2, textTransform: 'uppercase', letterSpacing: 0.4 }}>
        {label}
      </div>
      <div style={{ flex: 1, fontSize: 12, color: 'var(--text)', fontWeight: 500 }}>{value}</div>
    </div>
  );
}

function Chips({ items }) {
  return (
    <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap' }}>
      {items.length > 0
        ? items.map((item) => (
            <span key={item} className="badge badge-dim" style={{ fontSize: 11 }}>{item}</span>
          ))
        : <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>None selected</span>
      }
    </div>
  );
}

export default function InvestmentsWizardStepReview({ summary, creating, onBack, onCreate }) {
  const {
    scopeLabel,
    symbolsLabel,
    horizonLabel,
    riskLabel,
    signalLabels = [],
    patternLabels = [],
    approvalLabels = [],
    providerLabels = [],
  } = summary || {};

  return (
    <div>
      <h3 style={{ fontSize: 16, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
        Does this look right?
      </h3>
      <p style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 20 }}>
        Read this slowly. If something looks wrong, go back and change it.
      </p>

      <div style={{ marginBottom: 24 }}>
        {scopeLabel && <Row label="Watching" value={scopeLabel} />}
        {symbolsLabel && <Row label="Symbols" value={symbolsLabel} />}
        {horizonLabel && <Row label="Horizon" value={horizonLabel} />}
        {riskLabel && <Row label="Risk" value={riskLabel} />}
        <div style={{ paddingBottom: 10, borderBottom: '1px solid var(--border)', marginBottom: 10 }}>
          <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.4, marginBottom: 6 }}>Signals</div>
          <Chips items={signalLabels} />
        </div>
        <div style={{ paddingBottom: 10, borderBottom: '1px solid var(--border)', marginBottom: 10 }}>
          <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.4, marginBottom: 6 }}>Patterns</div>
          <Chips items={patternLabels} />
        </div>
        <div style={{ paddingBottom: 10, borderBottom: '1px solid var(--border)', marginBottom: 10 }}>
          <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.4, marginBottom: 6 }}>Approval rules</div>
          <Chips items={approvalLabels} />
        </div>
        <div style={{ paddingBottom: 10 }}>
          <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.4, marginBottom: 6 }}>Data sources</div>
          <Chips items={providerLabels.length > 0 ? providerLabels : ['Manual entry']} />
        </div>
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <button className="btn btn-ghost" onClick={onBack} disabled={creating}>← Back</button>
        <button className="btn btn-primary" onClick={onCreate} disabled={creating}>
          {creating ? (
            <>
              <span className="spinner" style={{ width: 13, height: 13, marginRight: 6 }} />
              Creating…
            </>
          ) : 'Approve setup'}
        </button>
      </div>
    </div>
  );
}
