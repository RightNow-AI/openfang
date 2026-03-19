'use client';
import { CREATIVE_STARTERS } from './config/creative-starters';
import CreativeStarterCard from './cards/CreativeStarterCard';

export default function CreativeTemplatesTab({ onStartTemplate }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ fontWeight: 700, fontSize: 15 }}>Starter templates</div>
      <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>
        Each template pre-fills the wizard with a recommended setup. You can change anything before running.
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))', gap: 14 }}>
        {CREATIVE_STARTERS.map(s => (
          <CreativeStarterCard key={s.id} starter={s} onStart={onStartTemplate} />
        ))}
      </div>
    </div>
  );
}
