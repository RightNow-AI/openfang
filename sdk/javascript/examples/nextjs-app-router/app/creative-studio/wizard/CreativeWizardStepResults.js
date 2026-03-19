'use client';

const ASSET_ICONS = {
  prompt: '💡',
  script: '📝',
  image: '🖼️',
  video: '🎬',
  voice: '🎙️',
  final: '✅',
};

function AssetCard({ asset, onApprove, onRevise }) {
  return (
    <div style={{
      padding: '14px 16px',
      borderRadius: 'var(--radius)',
      border: '1px solid var(--border)',
      background: 'var(--bg-elevated)',
      display: 'flex',
      flexDirection: 'column',
      gap: 8,
    }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 }}>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <span style={{ fontSize: 20 }}>{ASSET_ICONS[asset.type] ?? '📄'}</span>
          <span style={{ fontWeight: 600, fontSize: 13 }}>{asset.label}</span>
        </div>
        <ApprovalBadge state={asset.approval_state} />
      </div>
      {asset.content && (
        <div style={{
          padding: '10px 12px',
          borderRadius: 'var(--radius-sm)',
          background: 'var(--surface2)',
          fontSize: 12,
          color: 'var(--text-secondary)',
          lineHeight: 1.6,
          whiteSpace: 'pre-wrap',
          maxHeight: 120,
          overflow: 'auto',
        }}>
          {asset.content}
        </div>
      )}
      {asset.url && (
        <a href={asset.url} target="_blank" rel="noopener noreferrer"
          style={{ fontSize: 12, color: 'var(--accent)' }}>
          View result →
        </a>
      )}
      {asset.approval_state === 'pending' && (
        <div style={{ display: 'flex', gap: 8, marginTop: 4 }}>
          <button
            className="btn btn-sm"
            style={{ background: 'var(--success)', color: '#fff', border: 'none' }}
            onClick={() => onApprove(asset.id)}
          >
            Approve
          </button>
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => onRevise(asset.id)}
          >
            Revise
          </button>
        </div>
      )}
    </div>
  );
}

function ApprovalBadge({ state }) {
  if (state === 'approved') return <span className="badge badge-success">Approved</span>;
  if (state === 'rejected')  return <span className="badge badge-error">Needs revision</span>;
  return <span className="badge badge-warn">Waiting for review</span>;
}

export default function CreativeWizardStepResults({
  project,
  assets,
  running,
  error,
  onApprove,
  onRevise,
  onExport,
  onSendToWorkflow,
}) {
  const grouped = {
    prompt: assets.filter(a => a.type === 'prompt'),
    script: assets.filter(a => a.type === 'script'),
    image:  assets.filter(a => a.type === 'image'),
    video:  assets.filter(a => a.type === 'video'),
    voice:  assets.filter(a => a.type === 'voice'),
    final:  assets.filter(a => a.type === 'final'),
  };

  const hasAssets = assets.length > 0;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 24 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 8 }}>
        <div style={{ fontWeight: 700, fontSize: 15 }}>Results</div>
        <div style={{ display: 'flex', gap: 8 }}>
          {hasAssets && (
            <>
              <button className="btn btn-ghost btn-sm" onClick={onExport}>
                Export
              </button>
              <button className="btn btn-ghost btn-sm" onClick={onSendToWorkflow}>
                Send to next workflow
              </button>
            </>
          )}
        </div>
      </div>

      {error && (
        <div className="error-state">⚠ {error}</div>
      )}

      {running && (
        <div style={{
          padding: '16px',
          borderRadius: 'var(--radius)',
          background: 'var(--accent-subtle)',
          border: '1px solid var(--accent-glow)',
          fontSize: 13,
          color: 'var(--accent)',
          display: 'flex',
          gap: 10,
          alignItems: 'center',
        }}>
          <span style={{ animation: 'spin 1s linear infinite', display: 'inline-block' }}>⏳</span>
          Your creative project is running. Results will appear here as each step finishes.
        </div>
      )}

      {!hasAssets && !running && (
        <div className="empty-state">
          <div style={{ textAlign: 'center' }}>
            <div style={{ fontSize: 36, marginBottom: 8 }}>🎨</div>
            <div style={{ color: 'var(--text-dim)', fontSize: 14 }}>
              Approve the plan to start generating results.
            </div>
          </div>
        </div>
      )}

      {Object.entries(grouped).map(([type, items]) => {
        if (!items.length) return null;
        return (
          <div key={type}>
            <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--text-secondary)', marginBottom: 8, textTransform: 'uppercase', letterSpacing: '0.04em' }}>
              {ASSET_ICONS[type]} {type === 'final' ? 'Final outputs' : type.charAt(0).toUpperCase() + type.slice(1) + 's'}
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 10 }}>
              {items.map(a => (
                <AssetCard
                  key={a.id}
                  asset={a}
                  onApprove={onApprove}
                  onRevise={onRevise}
                />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
