'use client';

import { useState } from 'react';
import { DICTIONARY } from './prompt-library';

export default function PromptDictionaryPanel() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');

  const filtered = DICTIONARY.filter(
    (d) =>
      search === '' ||
      d.term.toLowerCase().includes(search.toLowerCase()) ||
      d.plainEnglish.toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <div
      data-cy="prompt-dictionary"
      style={{
        borderTop: '1px solid var(--border, #333)',
        marginTop: 8,
      }}
    >
      {/* Toggle */}
      <button
        onClick={() => setOpen((v) => !v)}
        style={{
          width: '100%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '10px 0',
          background: 'transparent',
          border: 'none',
          color: 'var(--text-dim, #888)',
          cursor: 'pointer',
          fontSize: 12,
          fontWeight: 600,
        }}
      >
        <span>📖 Plain-English dictionary</span>
        <span style={{ fontSize: 10 }}>{open ? '▲' : '▼'}</span>
      </button>

      {open && (
        <div style={{ paddingBottom: 12 }}>
          {/* Search inside dictionary */}
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Find a word…"
            data-cy="dictionary-search"
            style={{
              width: '100%',
              padding: '7px 10px',
              borderRadius: 7,
              border: '1px solid var(--border, #333)',
              background: 'var(--bg-elevated, #111)',
              color: 'var(--text-primary, #fff)',
              fontSize: 12,
              marginBottom: 10,
              boxSizing: 'border-box',
              outline: 'none',
            }}
          />

          <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
            {filtered.map(({ term, plainEnglish }) => (
              <div key={term} style={{ display: 'flex', gap: 8, alignItems: 'flex-start' }}>
                <span
                  style={{
                    flexShrink: 0,
                    fontSize: 11,
                    fontWeight: 700,
                    fontFamily: 'monospace',
                    color: 'var(--accent, #7c3aed)',
                    minWidth: 110,
                    paddingTop: 1,
                  }}
                >
                  {term}
                </span>
                <span style={{ fontSize: 12, color: 'var(--text-secondary, #bbb)', lineHeight: 1.45 }}>
                  = {plainEnglish}
                </span>
              </div>
            ))}
            {filtered.length === 0 && (
              <div style={{ fontSize: 12, color: 'var(--text-dim, #888)' }}>No matching terms.</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
