'use client';

export default function ResearchDownloadActions({ onRefresh, onCopy, onDownloadMarkdown, onDownloadJson }) {
  const buttonStyle = {
    padding: '6px 12px',
    background: 'var(--surface2)',
    border: '1px solid var(--border-light)',
    borderRadius: 6,
    fontSize: 12,
    cursor: 'pointer',
    color: 'var(--text)',
  };

  return (
    <div style={{ display: 'flex', gap: 6, flexShrink: 0, flexWrap: 'wrap' }}>
      <button onClick={onRefresh} title="Run this research again" style={buttonStyle}>↻ Refresh</button>
      <button onClick={onCopy} title="Copy report text" style={buttonStyle}>📋 Copy</button>
      <button onClick={onDownloadMarkdown} title="Download markdown report" style={buttonStyle}>⬇ Markdown</button>
      <button onClick={onDownloadJson} title="Download structured report JSON" style={buttonStyle}>⬇ JSON</button>
    </div>
  );
}