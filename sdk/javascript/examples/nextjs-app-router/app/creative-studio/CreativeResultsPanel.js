'use client';

const TYPE_LABELS = {
  prompt_pack: 'Prompt pack',
  script:      'Script',
  image:       'Images',
  video:       'Video',
  voice:       'Voice',
  moodboard:   'Moodboard',
};

const STATUS_COLORS = {
  draft:             '#6b7280',
  ready_for_review:  '#f59e0b',
  approved:          '#10b981',
  archived:          '#374151',
};

export default function CreativeResultsPanel({ assets, onOpenAsset, onApproveAsset, onExportAsset, onArchiveAsset }) {
  if (!assets?.length) {
    return (
      <div style={{ padding: '56px 0', textAlign: 'center', color: 'var(--text-dim)', fontSize: 14 }}>
        <div style={{ fontSize: 36, marginBottom: 10 }}>📦</div>
        <div>No results yet. Approve the plan and launch a task to generate assets.</div>
      </div>
    );
  }

  const byType = {};
  assets.forEach(a => {
    if (!byType[a.type]) byType[a.type] = [];
    byType[a.type].push(a);
  });

  return (
    <div data-cy="results-panel" style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
      {Object.entries(byType).map(([type, items]) => (
        <div key={type}>
          <div style={{ fontWeight: 700, fontSize: 11, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 10 }}>
            {TYPE_LABELS[type] ?? type}
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {items.map(asset => (
              <AssetCard
                key={asset.id}
                asset={asset}
                onOpen={() => onOpenAsset?.(asset.id)}
                onApprove={() => onApproveAsset?.(asset.id)}
                onExport={() => onExportAsset?.(asset.id)}
                onArchive={() => onArchiveAsset?.(asset.id)}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function AssetCard({ asset, onOpen, onApprove, onExport, onArchive }) {
  const c = STATUS_COLORS[asset.status] ?? '#6b7280';
  return (
    <div data-cy="asset-card" style={{ padding: '12px 16px', borderRadius: 10, background: 'var(--surface2)', border: `1px solid ${c}44`, display: 'flex', gap: 14, alignItems: 'flex-start' }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 5 }}>
          <div style={{ fontWeight: 600, fontSize: 14 }}>{asset.title}</div>
          <span style={{ fontSize: 10, padding: '2px 7px', borderRadius: 999, background: `${c}22`, color: c, border: `1px solid ${c}44`, flexShrink: 0 }}>
            {(asset.status ?? 'draft').replace(/_/g, ' ')}
          </span>
        </div>
        {asset.content_markdown && (
          <pre style={{ margin: '0 0 4px', fontSize: 12, color: 'var(--text-secondary,#bbb)', whiteSpace: 'pre-wrap', overflow: 'hidden', maxHeight: 72, fontFamily: 'inherit' }}>
            {asset.content_markdown.slice(0, 280)}{asset.content_markdown.length > 280 ? '…' : ''}
          </pre>
        )}
        {asset.url && (
          <a href={asset.url} target="_blank" rel="noopener noreferrer" style={{ fontSize: 12, color: 'var(--accent)' }}>
            View file →
          </a>
        )}
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 5, flexShrink: 0 }}>
        {asset.status === 'ready_for_review' && (
          <button onClick={onApprove} style={{ padding: '5px 10px', borderRadius: 6, background: 'var(--success,#10b981)', color: '#fff', border: 'none', cursor: 'pointer', fontSize: 11, fontWeight: 600 }}>Approve</button>
        )}
        <button onClick={onOpen} style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 11 }}>View</button>
        {asset.status === 'approved' && (
          <button onClick={onExport} style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 11 }}>Export</button>
        )}
        {asset.status !== 'archived' && (
          <button onClick={onArchive} style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 11 }}>Archive</button>
        )}
      </div>
    </div>
  );
}
