'use client';

import { useEffect, useState } from 'react';

export default function FounderPlaybookChips({ value, onChange, disabled = false }) {
  const [playbooks, setPlaybooks] = useState([]);

  useEffect(() => {
    let cancelled = false;

    fetch('/api/playbooks')
      .then((response) => response.ok ? response.json() : { playbooks: [] })
      .then((data) => {
        if (cancelled) return;
        setPlaybooks(Array.isArray(data?.playbooks) ? data.playbooks : []);
      })
      .catch(() => {
        if (cancelled) return;
        setPlaybooks([]);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  if (playbooks.length === 0) return null;

  return (
    <div style={{ marginTop: 12 }}>
      <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 8 }}>
        Founder Playbooks
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
        {playbooks.map((playbook) => {
          const selected = value === playbook.id;
          return (
            <button
              key={playbook.id}
              type="button"
              disabled={disabled}
              onClick={() => onChange(selected ? null : playbook)}
              title={playbook.description}
              style={{
                border: `1px solid ${selected ? 'var(--accent)' : 'var(--border-light)'}`,
                background: selected ? 'var(--accent-subtle)' : 'var(--surface2)',
                color: selected ? 'var(--accent)' : 'var(--text)',
                borderRadius: 999,
                padding: '7px 12px',
                fontSize: 12,
                cursor: disabled ? 'not-allowed' : 'pointer',
                opacity: disabled ? 0.6 : 1,
                display: 'inline-flex',
                alignItems: 'center',
                gap: 6,
              }}
            >
              <span>{playbook.icon}</span>
              <span>{playbook.title}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}