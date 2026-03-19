'use client';
import { useState } from 'react';

export default function CreativeVisualBoard({ references, selectedReferenceIds, onToggleReference, onAskDirectorAboutSelection, onUploadReference, onAddReferenceUrl }) {
  const [urlInput, setUrlInput] = useState('');

  const handleUpload = (e) => {
    const files = Array.from(e.target.files ?? []);
    if (files.length > 0) onUploadReference(files);
    e.target.value = '';
  };

  const handleAddUrl = () => {
    if (!urlInput.trim()) return;
    onAddReferenceUrl(urlInput.trim());
    setUrlInput('');
  };

  return (
    <div data-cy="visual-board" style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      {/* Toolbar */}
      <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', alignItems: 'center' }}>
        <label style={{ padding: '8px 16px', borderRadius: 8, background: 'var(--accent,#7c3aed)', color: '#fff', cursor: 'pointer', fontSize: 13, fontWeight: 600, flexShrink: 0 }}>
          Upload references
          <input type="file" multiple accept="image/*,video/*" onChange={handleUpload} style={{ display: 'none' }} />
        </label>
        <div style={{ display: 'flex', gap: 6, flex: 1, minWidth: 200 }}>
          <input
            value={urlInput}
            onChange={e => setUrlInput(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleAddUrl()}
            placeholder="Paste image or reference URL…"
            style={{ flex: 1, padding: '8px 12px', borderRadius: 8, background: 'var(--bg-elevated,#111)', border: '1px solid var(--border,#333)', color: 'var(--text-primary,#f1f1f1)', fontSize: 13, outline: 'none' }}
          />
          <button
            onClick={handleAddUrl}
            style={{ padding: '8px 14px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-primary,#f1f1f1)', cursor: 'pointer', fontSize: 13 }}
          >
            Add
          </button>
        </div>
        {selectedReferenceIds.length > 0 && (
          <button
            onClick={onAskDirectorAboutSelection}
            style={{ padding: '8px 14px', borderRadius: 8, background: 'transparent', border: '1px solid var(--accent,#7c3aed)', color: 'var(--accent,#7c3aed)', cursor: 'pointer', fontSize: 13, fontWeight: 600, flexShrink: 0 }}
          >
            Ask director ({selectedReferenceIds.length})
          </button>
        )}
      </div>

      {/* Empty state */}
      {references.length === 0 && (
        <div style={{ padding: '56px 0', textAlign: 'center', color: 'var(--text-dim,#888)', fontSize: 14, border: '2px dashed var(--border,#333)', borderRadius: 12 }}>
          <div style={{ fontSize: 36, marginBottom: 10 }}>📌</div>
          <div>No references yet. Upload images or paste URLs above.</div>
        </div>
      )}

      {/* Grid */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(150px, 1fr))', gap: 10 }}>
        {references.map(ref => {
          const selected = selectedReferenceIds.includes(ref.id);
          return (
            <div
              key={ref.id}
              data-cy="reference-card"
              onClick={() => onToggleReference(ref.id)}
              style={{ borderRadius: 10, overflow: 'hidden', border: `2px solid ${selected ? 'var(--accent,#7c3aed)' : 'var(--border,#333)'}`, cursor: 'pointer', position: 'relative', transition: 'border-color .15s' }}
            >
              {ref.url ? (
                <img src={ref.url} alt={ref.label ?? 'Reference'} style={{ width: '100%', height: 110, objectFit: 'cover', display: 'block' }} />
              ) : (
                <div style={{ width: '100%', height: 110, display: 'flex', alignItems: 'center', justifyContent: 'center', background: 'var(--surface2,#1a1a2e)', fontSize: 28 }}>🔗</div>
              )}
              {ref.label && (
                <div style={{ padding: '5px 8px', fontSize: 11, color: 'var(--text-dim,#888)', background: 'var(--bg-elevated,#111)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{ref.label}</div>
              )}
              {selected && (
                <div style={{ position: 'absolute', top: 6, right: 6, background: 'var(--accent,#7c3aed)', borderRadius: '50%', width: 20, height: 20, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 11, color: '#fff' }}>✓</div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
