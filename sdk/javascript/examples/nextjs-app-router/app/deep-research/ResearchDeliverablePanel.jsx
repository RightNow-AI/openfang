'use client';

import ResearchDownloadActions from './ResearchDownloadActions';

function summaryCount(label, value) {
  return {
    label,
    value: String(value ?? 0),
  };
}

export default function ResearchDeliverablePanel({
  query,
  report,
  founderWorkspace,
  onRefresh,
  onCopy,
  onDownloadMarkdown,
  onDownloadJson,
}) {
  const metrics = [
    summaryCount('Sources', report?.sourceUrls?.length ?? 0),
    summaryCount('Citations', report?.citationCount ?? report?.citationUrls?.length ?? 0),
    summaryCount('Next steps', report?.nextActionCount ?? 0),
  ];

  return (
    <div style={{
      padding: '18px 20px',
      border: '1px solid var(--border-light)',
      borderRadius: 14,
      background: 'linear-gradient(180deg, rgba(37,99,235,0.09), rgba(15,23,42,0.18))',
      marginBottom: 18,
    }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16, flexWrap: 'wrap' }}>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.08em', marginBottom: 8 }}>
            Research result ready
          </div>
          <div style={{ fontSize: 22, fontWeight: 800, color: 'var(--text)', marginBottom: 8 }}>
            {report?.lead || report?.findings?.split('\n')[0] || query}
          </div>
          <div style={{ fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.6, maxWidth: 760 }}>
            {founderWorkspace
              ? `Saved to ${founderWorkspace.name}. Review the result, download it if needed, and turn the next steps into action.`
              : 'Your research result is ready to review, download, or reuse.'}
          </div>
        </div>
        <ResearchDownloadActions
          onRefresh={onRefresh}
          onCopy={onCopy}
          onDownloadMarkdown={onDownloadMarkdown}
          onDownloadJson={onDownloadJson}
        />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', gap: 12, marginTop: 16 }}>
        {metrics.map((metric) => (
          <div key={metric.label} style={{ padding: '12px 14px', borderRadius: 12, background: 'rgba(15,23,42,0.24)', border: '1px solid rgba(148,163,184,0.14)' }}>
            <div style={{ fontSize: 11, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 6 }}>{metric.label}</div>
            <div style={{ fontSize: 20, fontWeight: 800, color: 'var(--text)' }}>{metric.value}</div>
          </div>
        ))}
      </div>
    </div>
  );
}