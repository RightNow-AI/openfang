'use client';
import { AI_CHOICE_CATALOG, COST_TIER_LABELS, SPEED_LABELS } from './config/ai-choice-catalog';

const CATEGORY_LABELS = {
  prompt: 'Prompting / idea generation',
  image:  'Image AI',
  video:  'Video AI',
  voice:  'Voice / sound',
  script: 'Script writing',
};

export default function AdvancedCreativeTab() {
  const grouped = {};
  for (const c of AI_CHOICE_CATALOG) {
    if (!grouped[c.category]) grouped[c.category] = [];
    grouped[c.category].push(c);
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
      <div>
        <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 4 }}>Advanced options</div>
        <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>
          Full list of all AI tools available in Creative Studio.
          These are selectable inside the wizard on the AI tools step.
        </div>
      </div>

      {Object.entries(grouped).map(([cat, choices]) => (
        <div key={cat}>
          <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--text-secondary)', marginBottom: 10, textTransform: 'uppercase', letterSpacing: '0.05em' }}>
            {CATEGORY_LABELS[cat] ?? cat}
          </div>
          <div style={{ overflowX: 'auto' }}>
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: 12 }}>
              <thead>
                <tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-muted)' }}>
                  <th style={{ textAlign: 'left', padding: '6px 10px', fontWeight: 600 }}>Tool</th>
                  <th style={{ textAlign: 'left', padding: '6px 10px', fontWeight: 600 }}>Best for</th>
                  <th style={{ textAlign: 'left', padding: '6px 10px', fontWeight: 600 }}>Cost</th>
                  <th style={{ textAlign: 'left', padding: '6px 10px', fontWeight: 600 }}>Speed</th>
                  <th style={{ textAlign: 'left', padding: '6px 10px', fontWeight: 600 }}>Approval needed</th>
                </tr>
              </thead>
              <tbody>
                {choices.map(c => (
                  <tr key={c.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <td style={{ padding: '8px 10px', fontWeight: 600, color: 'var(--text)' }}>{c.label}</td>
                    <td style={{ padding: '8px 10px', color: 'var(--text-secondary)' }}>{c.best_for}</td>
                    <td style={{ padding: '8px 10px' }}>{COST_TIER_LABELS[c.cost_tier]}</td>
                    <td style={{ padding: '8px 10px' }}>{SPEED_LABELS[c.speed_label]}</td>
                    <td style={{ padding: '8px 10px', color: c.requires_approval ? 'var(--warning)' : 'var(--success)' }}>
                      {c.requires_approval ? 'Yes' : 'No'}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ))}
    </div>
  );
}
