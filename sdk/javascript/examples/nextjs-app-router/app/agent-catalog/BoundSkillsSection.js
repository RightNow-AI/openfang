'use client';

export default function BoundSkillsSection({ bindings, suggested }) {
  const items = bindings ?? suggested ?? [];
  if (!items.length) return null;
  const issuggested = !bindings?.length && !!suggested?.length;

  return (
    <section>
      <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em', marginBottom: 8 }}>
        {issuggested ? 'Suggested Skills' : `Bound Skills (${items.length})`}
      </div>
      {issuggested && (
        <div style={{ fontSize: 11, color: 'var(--text-dim)', fontStyle: 'italic', marginBottom: 6 }}>
          Derived from tool references — not yet explicitly bound.
        </div>
      )}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        {items.map(skill => (
          <div
            key={skill.name}
            data-cy="bound-skill-row"
            style={{
              display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap',
              padding: '5px 10px',
              background: 'var(--surface2)',
              borderRadius: 'var(--radius-sm)',
              fontSize: 12,
            }}
          >
            <span style={{ fontFamily: 'var(--font-mono,monospace)', fontWeight: 600 }}>
              {skill.name}
            </span>
            {skill.version && (
              <span className="badge badge-dim" style={{ fontSize: 10, fontFamily: 'var(--font-mono,monospace)' }}>
                v{skill.version}
              </span>
            )}
            <span className={`badge ${skill.required === false ? 'badge-muted' : 'badge-info'}`} style={{ fontSize: 10 }}>
              {skill.required === false ? 'optional' : 'required'}
            </span>
            {skill.suggested && (
              <span className="badge badge-warning" style={{ fontSize: 10 }}>suggested</span>
            )}
            {skill.source && skill.source !== 'unknown' && (
              <span className="badge badge-dim" style={{ fontSize: 10 }}>{skill.source}</span>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
