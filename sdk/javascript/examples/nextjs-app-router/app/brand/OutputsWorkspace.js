'use client';

import { useState } from 'react';

const OUTPUT_TYPE_LABELS = {
  brand_brief: 'Brand Brief',
  competitor_matrix: 'Competitor Matrix',
  voice_guide: 'Voice Guide',
  customer_avatar: 'Customer Avatar',
  email_sequence: 'Email Sequence',
  homepage_messaging: 'Homepage Messaging',
  content_plan: 'Content Plan',
  proof_stack: 'Proof Stack',
  offer_map: 'Offer Map',
  founder_bio: 'Founder Bio',
};

const STATUS_CONFIG = {
  draft: { label: 'Draft', color: 'var(--text-muted)', bg: 'var(--surface3)' },
  ready_for_review: { label: 'Ready for Review', color: 'var(--warning)', bg: 'var(--warning-subtle)' },
  approved: { label: 'Approved', color: 'var(--success)', bg: 'var(--success-subtle)' },
  archived: { label: 'Archived', color: 'var(--text-muted)', bg: 'var(--surface3)' },
};

// ── OutputCard ─────────────────────────────────────────────────────────────

function OutputCard({ output, isActive, onSelect, onApprove, onRequestRevision, onDuplicate, onExport }) {
  const [expanded, setExpanded] = useState(false);
  const statusCfg = STATUS_CONFIG[output.status] || STATUS_CONFIG.draft;
  const typeLabel = OUTPUT_TYPE_LABELS[output.output_type] || output.output_type;
  const createdDate = new Date(output.created_at).toLocaleString([], {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  });
  const durationSec = output.duration_ms ? `${(output.duration_ms / 1000).toFixed(1)}s` : null;

  return (
    <div
      style={{
        borderRadius: 10,
        border: `1px solid ${isActive ? 'var(--accent)' : 'var(--border)'}`,
        background: isActive ? 'var(--accent-subtle)' : 'var(--bg-elevated)',
        marginBottom: 10,
        overflow: 'hidden',
        transition: 'border-color 0.15s',
      }}
    >
      {/* Card header */}
      <div
        style={{ padding: '12px 14px', cursor: 'pointer' }}
        onClick={() => { onSelect(output.id); setExpanded(v => !v); }}
      >
        <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 700, fontSize: 13, color: 'var(--text)', marginBottom: 3 }}>
              {output.title}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
              <span style={{
                fontSize: 10, padding: '2px 6px', borderRadius: 4, fontWeight: 600,
                background: 'var(--surface3)', color: 'var(--text-dim)',
              }}>{typeLabel}</span>
              <span style={{
                fontSize: 10, padding: '2px 6px', borderRadius: 4, fontWeight: 600,
                background: statusCfg.bg, color: statusCfg.color,
              }}>{statusCfg.label}</span>
              <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{createdDate}</span>
              {durationSec && (
                <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>· {durationSec}</span>
              )}
            </div>
          </div>
          <span style={{
            fontSize: 14, color: 'var(--text-muted)',
            transform: expanded ? 'rotate(90deg)' : 'none',
            transition: 'transform 0.15s',
            flexShrink: 0,
          }}>›</span>
        </div>
      </div>

      {/* Expanded content */}
      {expanded && (
        <>
          {/* Content preview */}
          <div style={{
            borderTop: '1px solid var(--border)',
            padding: '14px 14px',
            maxHeight: 480,
            overflowY: 'auto',
          }}>
            <pre style={{
              margin: 0,
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
              fontSize: 12,
              lineHeight: 1.7,
              color: 'var(--text-secondary)',
              fontFamily: 'var(--font-sans)',
            }}>
              {output.content}
            </pre>
          </div>

          {/* Actions */}
          <div style={{
            borderTop: '1px solid var(--border)',
            padding: '10px 14px',
            display: 'flex',
            gap: 7,
            flexWrap: 'wrap',
            background: 'var(--surface2)',
          }}>
            {output.status !== 'approved' && (
              <ActionBtn
                onClick={() => onApprove(output.id)}
                color="var(--success)"
                bg="var(--success-subtle)"
                border="var(--success)"
              >
                ✓ Approve
              </ActionBtn>
            )}
            {output.status === 'approved' && (
              <ActionBtn
                onClick={() => onRequestRevision(output.id)}
                color="var(--warning)"
                bg="var(--warning-subtle)"
                border="var(--warning)"
              >
                ↺ Revise
              </ActionBtn>
            )}
            {output.status !== 'approved' && (
              <ActionBtn
                onClick={() => onRequestRevision(output.id)}
                color="var(--text-dim)"
                bg="var(--surface3)"
                border="var(--border)"
              >
                ↺ Revise
              </ActionBtn>
            )}
            <ActionBtn
              onClick={() => onDuplicate(output.id)}
              color="var(--text-dim)"
              bg="var(--surface3)"
              border="var(--border)"
            >
              ⧉ Duplicate
            </ActionBtn>
            <ActionBtn
              onClick={() => onExport(output.id)}
              color="var(--accent)"
              bg="var(--accent-subtle)"
              border="var(--accent)"
            >
              ↓ Export
            </ActionBtn>
          </div>
        </>
      )}
    </div>
  );
}

function ActionBtn({ onClick, color, bg, border, children }) {
  return (
    <button
      onClick={onClick}
      style={{
        padding: '5px 10px', borderRadius: 6, fontSize: 11, fontWeight: 600,
        border: `1px solid ${border}`, background: bg, color, cursor: 'pointer',
      }}
    >
      {children}
    </button>
  );
}

// ── Empty state ────────────────────────────────────────────────────────────

function EmptyState() {
  return (
    <div style={{
      display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
      padding: '60px 24px', textAlign: 'center',
    }}>
      <div style={{ fontSize: 40, marginBottom: 16 }}>🖊</div>
      <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', marginBottom: 8 }}>
        No outputs yet
      </div>
      <div style={{ fontSize: 13, color: 'var(--text-dim)', maxWidth: 340, lineHeight: 1.6 }}>
        Fill in your brand context on the left, then click a task in the Agent Launchpad to generate your first deliverable.
      </div>
      <div style={{ marginTop: 20, fontSize: 12, color: 'var(--text-muted)' }}>
        Start with <strong style={{ color: 'var(--accent)' }}>Analyze My Business</strong> — it only needs 5 fields.
      </div>
    </div>
  );
}

// ── OutputsWorkspace ───────────────────────────────────────────────────────

export default function OutputsWorkspace({
  outputs,
  activeOutputId,
  onSelect,
  onApprove,
  onRequestRevision,
  onDuplicate,
  onExport,
}) {
  const [activeTab, setActiveTab] = useState('all');

  const visibleOutputs = outputs.filter(o => {
    if (activeTab === 'all') return o.status !== 'archived';
    return o.status === activeTab;
  });

  const counts = {
    all: outputs.filter(o => o.status !== 'archived').length,
    ready_for_review: outputs.filter(o => o.status === 'ready_for_review').length,
    approved: outputs.filter(o => o.status === 'approved').length,
    archived: outputs.filter(o => o.status === 'archived').length,
  };

  const TABS = [
    { id: 'all', label: 'All' },
    { id: 'ready_for_review', label: 'To Review' },
    { id: 'approved', label: 'Approved' },
    { id: 'archived', label: 'Archived' },
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', minHeight: '100%', padding: '16px 20px' }}>
      {/* Header */}
      <div style={{
        display: 'flex', alignItems: 'center', justifyContent: 'space-between',
        marginBottom: 14, gap: 12,
      }}>
        <div>
          <h2 style={{ margin: 0, fontSize: 16, fontWeight: 700, color: 'var(--text)' }}>
            Outputs Workspace
          </h2>
          <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 2 }}>
            {counts.all === 0 ? 'Your deliverables will appear here' : `${counts.all} deliverable${counts.all === 1 ? '' : 's'}`}
          </div>
        </div>

        {/* Tab filters */}
        {counts.all > 0 && (
          <div style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
            {TABS.map(tab => {
              const count = counts[tab.id];
              if (tab.id !== 'all' && count === 0) return null;
              return (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id)}
                  style={{
                    padding: '4px 10px', borderRadius: 6, fontSize: 11, fontWeight: 600,
                    border: `1px solid ${activeTab === tab.id ? 'var(--accent)' : 'var(--border)'}`,
                    background: activeTab === tab.id ? 'var(--accent-subtle)' : 'var(--surface)',
                    color: activeTab === tab.id ? 'var(--accent)' : 'var(--text-dim)',
                    cursor: 'pointer',
                  }}
                >
                  {tab.label}
                  {count > 0 && (
                    <span style={{
                      marginLeft: 4, fontSize: 10,
                      background: activeTab === tab.id ? 'var(--accent)' : 'var(--surface3)',
                      color: activeTab === tab.id ? '#fff' : 'var(--text-muted)',
                      padding: '0 4px', borderRadius: 3,
                    }}>{count}</span>
                  )}
                </button>
              );
            })}
          </div>
        )}
      </div>

      {/* Content */}
      {visibleOutputs.length === 0 ? (
        <EmptyState />
      ) : (
        <div>
          {visibleOutputs.map(output => (
            <OutputCard
              key={output.id}
              output={output}
              isActive={output.id === activeOutputId}
              onSelect={onSelect}
              onApprove={onApprove}
              onRequestRevision={onRequestRevision}
              onDuplicate={onDuplicate}
              onExport={onExport}
            />
          ))}
        </div>
      )}
    </div>
  );
}
