'use client';
import { CREATIVE_STARTERS } from './config/creative-starters';
import CreativeStarterCard from './cards/CreativeStarterCard';

export default function RecommendedCreativeTab({ onStartBlank, onStartTemplate }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 32 }}>
      {/* Primary CTA */}
      <div style={{
        padding: '28px 32px',
        borderRadius: 'var(--radius-lg)',
        background: 'linear-gradient(135deg, var(--accent-subtle) 0%, var(--surface2) 100%)',
        border: '1px solid var(--accent-glow)',
        display: 'flex',
        flexDirection: 'column',
        gap: 12,
        alignItems: 'flex-start',
      }}>
        <div style={{ fontWeight: 800, fontSize: 22, color: 'var(--text)' }}>
          🎨 Creative Studio
        </div>
        <div style={{ fontSize: 14, color: 'var(--text-secondary)', maxWidth: 520, lineHeight: 1.5 }}>
          Answer a few simple questions and OpenFang will build your creative project — prompts, scripts, images, voice, and video — step by step with your approval at every important moment.
        </div>
        <button
          data-cy="start-creative-cta"
          className="btn"
          style={{ background: 'var(--accent)', color: '#fff', border: 'none', padding: '10px 22px', fontSize: 14, fontWeight: 700 }}
          onClick={onStartBlank}
        >
          Set up a creative project for me
        </button>
      </div>

      {/* Starter packs */}
      <div>
        <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 16 }}>
          Or start with a template
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))', gap: 14 }}>
          {CREATIVE_STARTERS.map(s => (
            <CreativeStarterCard key={s.id} starter={s} onStart={onStartTemplate} />
          ))}
        </div>
      </div>
    </div>
  );
}
