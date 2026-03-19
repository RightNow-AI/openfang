'use client';
import { COST_TIER_LABELS, SPEED_LABELS } from '../config/ai-choice-catalog';

const COST_COLORS = { free: '#22c55e', low: '#3b82f6', medium: '#f59e0b', high: '#ef4444' };

export default function AiChoiceCard({ choice, selected, onSelect }) {
  return (
    <button
      type="button"
      data-cy="ai-choice-card"
      onClick={() => onSelect(choice.id)}
      style={{
        textAlign: 'left',
        padding: '14px 16px',
        borderRadius: 'var(--radius)',
        border: selected
          ? '2px solid var(--accent)'
          : '2px solid var(--border)',
        background: selected ? 'var(--accent-subtle)' : 'var(--bg-elevated)',
        cursor: 'pointer',
        transition: 'border-color 0.15s, background 0.15s',
        display: 'flex',
        flexDirection: 'column',
        gap: 6,
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8 }}>
        <span style={{ fontWeight: 600, fontSize: 13, color: 'var(--text)' }}>{choice.label}</span>
        {choice.auto_recommend && (
          <span style={{ fontSize: 10, padding: '2px 7px', borderRadius: 999, background: 'var(--accent-subtle)', color: 'var(--accent)', fontWeight: 600 }}>
            Recommended
          </span>
        )}
      </div>
      <span style={{ fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5 }}>{choice.description}</span>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 4 }}>
        <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>
          Best for: <em>{choice.best_for}</em>
        </span>
      </div>
      <div style={{ display: 'flex', gap: 10, marginTop: 2 }}>
        <span style={{
          fontSize: 10, padding: '2px 7px', borderRadius: 999,
          background: COST_COLORS[choice.cost_tier] + '22',
          color: COST_COLORS[choice.cost_tier],
          fontWeight: 600,
        }}>
          {COST_TIER_LABELS[choice.cost_tier]}
        </span>
        <span style={{
          fontSize: 10, padding: '2px 7px', borderRadius: 999,
          background: 'var(--surface2)',
          color: 'var(--text-dim)',
        }}>
          {SPEED_LABELS[choice.speed_label]}
        </span>
        {choice.requires_approval && (
          <span style={{
            fontSize: 10, padding: '2px 7px', borderRadius: 999,
            background: 'var(--warning-subtle)',
            color: 'var(--warning)',
          }}>
            Needs approval
          </span>
        )}
      </div>
    </button>
  );
}
