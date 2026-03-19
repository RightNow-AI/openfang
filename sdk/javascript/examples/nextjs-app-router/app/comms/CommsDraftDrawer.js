'use client';
import { useState, useEffect } from 'react';

export default function CommsDraftDrawer({ open, draftId, onClose, onApprove, onRequestChanges, onSend }) {
  const [draft, setDraft] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [changeNote, setChangeNote] = useState('');
  const [showChangeInput, setShowChangeInput] = useState(false);
  const [working, setWorking] = useState(null);

  useEffect(() => {
    if (!open || !draftId) return;
    setDraft(null);
    setError('');
    setShowChangeInput(false);
    setLoading(true);
    fetch(`/api/comms/drafts/${draftId}`)
      .then(r => r.ok ? r.json() : Promise.reject(r.statusText))
      .then(d => setDraft(d?.draft ?? d))
      .catch(e => setError(e?.message || 'Could not load draft.'))
      .finally(() => setLoading(false));
  }, [open, draftId]);

  const act = async (fn, key) => {
    setWorking(key);
    try { await fn(); onClose(); } catch {}
    setWorking(null);
  };

  if (!open) return null;

  return (
    <div
      style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,.6)', backdropFilter: 'blur(3px)', display: 'flex', justifyContent: 'flex-end' }}
      onClick={e => e.target === e.currentTarget && onClose()}
    >
      <div data-cy="comms-draft-drawer" style={{ width: 520, background: 'var(--bg-elevated)', borderLeft: '1px solid var(--border)', overflowY: 'auto', padding: '28px 24px', display: 'flex', flexDirection: 'column', gap: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
          <div style={{ fontWeight: 700, fontSize: 18 }}>{draft?.subject ?? 'Draft'}</div>
          <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 22, color: 'var(--text-dim)', lineHeight: 1 }}>✕</button>
        </div>

        {loading && <div style={{ color: 'var(--text-dim)', fontSize: 14 }}>Loading…</div>}
        {error && <div style={{ color: 'var(--error,#ef4444)', fontSize: 13, padding: '10px 14px', borderRadius: 8, background: 'rgba(239,68,68,.08)', border: '1px solid rgba(239,68,68,.2)' }}>{error}</div>}

        {draft && (
          <>
            {/* Meta */}
            <div style={{ display: 'flex', gap: 10, marginBottom: 16, flexWrap: 'wrap', alignItems: 'center' }}>
              <ApprovalBadge status={draft.approval_status} />
              {draft.channel && <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>via {draft.channel}</span>}
            </div>

            {/* Recipients */}
            {draft.recipients?.length > 0 && (
              <div style={{ marginBottom: 14 }}>
                <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 5 }}>Recipients</div>
                <div style={{ fontSize: 13 }}>{draft.recipients.join(', ')}</div>
              </div>
            )}

            {/* Body */}
            <div style={{ marginBottom: 18 }}>
              <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 8 }}>Message</div>
              <div style={{ fontSize: 14, lineHeight: 1.7, color: 'var(--text-primary)', whiteSpace: 'pre-wrap', background: 'var(--surface2)', padding: '12px 14px', borderRadius: 8, border: '1px solid var(--border)' }}>
                {draft.body}
              </div>
            </div>

            {/* Request changes input */}
            {showChangeInput && (
              <div style={{ display: 'flex', gap: 8, marginBottom: 10 }}>
                <input
                  value={changeNote}
                  onChange={e => setChangeNote(e.target.value)}
                  placeholder="Describe what needs changing…"
                  style={{ flex: 1, padding: '8px 12px', borderRadius: 7, background: 'var(--bg-elevated)', border: '1px solid var(--border)', color: 'var(--text-primary)', fontSize: 13, outline: 'none' }}
                />
                <button
                  onClick={() => act(() => onRequestChanges?.(draftId, changeNote), 'change')}
                  disabled={!changeNote.trim() || working === 'change'}
                  style={{ padding: '8px 14px', borderRadius: 7, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 13 }}
                >
                  {working === 'change' ? '…' : 'Send'}
                </button>
              </div>
            )}

            {/* Actions */}
            <div style={{ display: 'flex', gap: 10, marginTop: 'auto', paddingTop: 20, borderTop: '1px solid var(--border)', flexWrap: 'wrap' }}>
              {draft.approval_status !== 'approved' && (
                <button onClick={() => act(() => onApprove?.(draftId), 'approve')} disabled={!!working} style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--success,#10b981)', color: '#fff', border: 'none', cursor: working ? 'not-allowed' : 'pointer', fontWeight: 700, fontSize: 14, opacity: working && working !== 'approve' ? 0.5 : 1 }}>
                  {working === 'approve' ? 'Approving…' : '✓ Approve'}
                </button>
              )}
              {draft.approval_status === 'approved' && (
                <button onClick={() => act(() => onSend?.(draftId), 'send')} disabled={!!working} style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent)', color: '#fff', border: 'none', cursor: working ? 'not-allowed' : 'pointer', fontWeight: 700, fontSize: 14, opacity: working && working !== 'send' ? 0.5 : 1 }}>
                  {working === 'send' ? 'Sending…' : 'Send'}
                </button>
              )}
              <button onClick={() => setShowChangeInput(v => !v)} style={{ padding: '9px 14px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 13 }}>Request changes</button>
              <button onClick={onClose} style={{ padding: '9px 14px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 13 }}>Close</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function ApprovalBadge({ status }) {
  const MAP = { approved: '#10b981', pending_review: '#f59e0b', rejected: '#ef4444', draft: '#6b7280' };
  const c = MAP[status] ?? '#6b7280';
  return <span style={{ fontSize: 11, padding: '3px 9px', borderRadius: 999, background: `${c}22`, color: c, border: `1px solid ${c}44` }}>{(status ?? 'draft').replace(/_/g, ' ')}</span>;
}
