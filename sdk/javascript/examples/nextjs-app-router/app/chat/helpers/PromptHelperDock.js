'use client';

import { useState, useCallback } from 'react';
import { PROMPT_LIBRARY, QUICK_START_CARDS, CATEGORIES } from './prompt-library';
import PromptFillForm from './PromptFillForm';
import PromptDictionaryPanel from './PromptDictionaryPanel';

const RECENT_KEY = 'of-recent-helpers';
const MAX_RECENT = 5;

// ─── Sub-components ──────────────────────────────────────────────────────────

function QuickCard({ card, onSelect }) {
  const template = PROMPT_LIBRARY.find((t) => t.id === card.templateId);
  return (
    <button
      onClick={() => template && onSelect(template)}
      data-cy={`quick-card-${card.templateId}`}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        padding: '10px 12px',
        borderRadius: 10,
        border: '1px solid var(--border, #333)',
        background: 'var(--bg-elevated, #111)',
        color: 'var(--text-primary, #fff)',
        cursor: 'pointer',
        textAlign: 'left',
        transition: 'border-color 0.15s, background 0.15s',
        width: '100%',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = 'var(--accent, #7c3aed)';
        e.currentTarget.style.background = 'var(--surface3, #2a2a3e)';
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = 'var(--border, #333)';
        e.currentTarget.style.background = 'var(--bg-elevated, #111)';
      }}
    >
      <span style={{ fontSize: 22, flexShrink: 0 }}>{card.icon}</span>
      <span style={{ fontSize: 13, fontWeight: 600, lineHeight: 1.3 }}>{card.label}</span>
    </button>
  );
}

function TemplateCard({ template, onSelect }) {
  return (
    <div
      data-cy={`template-card-${template.id}`}
      style={{
        padding: '12px 14px',
        borderRadius: 10,
        border: '1px solid var(--border, #333)',
        background: 'var(--bg-elevated, #111)',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8, marginBottom: 6 }}>
        <span style={{ fontSize: 20, flexShrink: 0 }}>{template.icon}</span>
        <div>
          <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text-primary, #fff)' }}>
            {template.title}
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-dim, #888)', marginTop: 2 }}>
            {template.description}
          </div>
        </div>
      </div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <span
          style={{
            fontSize: 10,
            fontWeight: 600,
            padding: '2px 7px',
            borderRadius: 20,
            background:
              template.difficulty === 'beginner'
                ? 'rgba(16,185,129,0.15)'
                : template.difficulty === 'intermediate'
                ? 'rgba(124,58,237,0.15)'
                : 'rgba(248,113,113,0.15)',
            color:
              template.difficulty === 'beginner'
                ? 'var(--success, #10b981)'
                : template.difficulty === 'intermediate'
                ? 'var(--accent, #7c3aed)'
                : 'var(--error, #f87171)',
          }}
        >
          {template.difficulty}
        </span>
        <button
          onClick={() => onSelect(template)}
          data-cy={`use-template-${template.id}`}
          style={{
            fontSize: 12,
            fontWeight: 600,
            padding: '5px 12px',
            borderRadius: 7,
            border: 'none',
            background: 'var(--accent, #7c3aed)',
            color: '#fff',
            cursor: 'pointer',
          }}
        >
          Fill this in for me
        </button>
      </div>
    </div>
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export default function PromptHelperDock({ open, onClose, onUseTemplate }) {
  const [activeCategory, setActiveCategory] = useState('');
  const [activeTemplate, setActiveTemplate] = useState(null);
  const [search, setSearch] = useState('');
  const [recentIds, setRecentIds] = useState(() => {
    if (typeof window === 'undefined') return [];
    try {
      const raw = localStorage.getItem(RECENT_KEY);
      return raw ? JSON.parse(raw) : [];
    } catch {
      return [];
    }
  });

  const handleSelectTemplate = useCallback((template) => {
    setActiveTemplate(template);
  }, []);

  const handleFormSubmit = useCallback(
    async (filledPrompt) => {
      // Update recent helpers
      setRecentIds((prev) => {
        const next = [activeTemplate.id, ...prev.filter((id) => id !== activeTemplate.id)].slice(
          0,
          MAX_RECENT,
        );
        try {
          localStorage.setItem(RECENT_KEY, JSON.stringify(next));
        } catch {
          // ignore
        }
        return next;
      });
      setActiveTemplate(null);
      setSearch('');
      setActiveCategory('');
      onUseTemplate(filledPrompt);
    },
    [activeTemplate, onUseTemplate],
  );

  const handleFormCancel = useCallback(() => {
    setActiveTemplate(null);
  }, []);

  const handleClose = useCallback(() => {
    setActiveTemplate(null);
    setSearch('');
    setActiveCategory('');
    onClose();
  }, [onClose]);

  // Filtered templates
  const filteredTemplates = PROMPT_LIBRARY.filter((t) => {
    const matchCat = activeCategory === '' || t.category === activeCategory;
    const q = search.trim().toLowerCase();
    const matchSearch =
      q === '' ||
      t.title.toLowerCase().includes(q) ||
      t.description.toLowerCase().includes(q);
    return matchCat && matchSearch;
  });

  const recentTemplates = recentIds
    .map((id) => PROMPT_LIBRARY.find((t) => t.id === id))
    .filter(Boolean);

  if (!open) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        onClick={handleClose}
        aria-hidden="true"
        style={{
          position: 'fixed',
          inset: 0,
          background: 'rgba(0,0,0,0.5)',
          zIndex: 199,
        }}
      />

      {/* Dock panel */}
      <div
        data-cy="prompt-helper-dock"
        role="dialog"
        aria-label="Prompt helpers"
        style={{
          position: 'fixed',
          right: 0,
          top: 0,
          height: '100vh',
          width: 420,
          maxWidth: '100vw',
          zIndex: 200,
          background: 'var(--surface2, #1a1a2e)',
          borderLeft: '1px solid var(--border, #333)',
          display: 'flex',
          flexDirection: 'column',
          overflow: 'hidden',
        }}
      >
        {/* Header */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '14px 16px',
            borderBottom: '1px solid var(--border, #333)',
            flexShrink: 0,
          }}
        >
          <div>
            <span style={{ fontSize: 15, fontWeight: 700, color: 'var(--text-primary, #fff)' }}>
              ✨ Prompt helpers
            </span>
            <div style={{ fontSize: 12, color: 'var(--text-dim, #888)', marginTop: 2 }}>
              We build it. You just answer a few questions.
            </div>
          </div>
          <button
            onClick={handleClose}
            aria-label="Close prompt helpers"
            data-cy="close-prompt-dock"
            style={{
              background: 'transparent',
              border: 'none',
              color: 'var(--text-dim, #888)',
              fontSize: 20,
              cursor: 'pointer',
              lineHeight: 1,
              padding: 4,
            }}
          >
            ×
          </button>
        </div>

        {/* Body — scrollable */}
        <div style={{ flex: 1, overflowY: 'auto', padding: '14px 16px' }}>
          {activeTemplate ? (
            // ── Fill-in form ──────────────────────────────────────────
            <PromptFillForm
              template={activeTemplate}
              onSubmit={handleFormSubmit}
              onCancel={handleFormCancel}
            />
          ) : (
            // ── Browse layers ─────────────────────────────────────────
            <>
              {/* Search */}
              <div style={{ marginBottom: 14 }}>
                <input
                  type="search"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                  placeholder="Search helpers…"
                  data-cy="dock-search"
                  style={{
                    width: '100%',
                    padding: '9px 12px',
                    borderRadius: 9,
                    border: '1px solid var(--border, #333)',
                    background: 'var(--bg-elevated, #111)',
                    color: 'var(--text-primary, #fff)',
                    fontSize: 13,
                    boxSizing: 'border-box',
                    outline: 'none',
                  }}
                />
              </div>

              {/* ── Recent (only when no search/filter active and have recents) */}
              {search === '' && activeCategory === '' && recentTemplates.length > 0 && (
                <section style={{ marginBottom: 20 }}>
                  <div
                    style={{
                      fontSize: 11,
                      fontWeight: 700,
                      color: 'var(--text-dim, #888)',
                      letterSpacing: '0.08em',
                      textTransform: 'uppercase',
                      marginBottom: 8,
                    }}
                  >
                    Recently used
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
                    {recentTemplates.map((t) => (
                      <TemplateCard key={t.id} template={t} onSelect={handleSelectTemplate} />
                    ))}
                  </div>
                </section>
              )}

              {/* ── Quick-start cards (when no search) */}
              {search === '' && activeCategory === '' && (
                <section style={{ marginBottom: 20 }}>
                  <div
                    style={{
                      fontSize: 11,
                      fontWeight: 700,
                      color: 'var(--text-dim, #888)',
                      letterSpacing: '0.08em',
                      textTransform: 'uppercase',
                      marginBottom: 8,
                    }}
                  >
                    Quick start
                  </div>
                  <div
                    style={{
                      display: 'grid',
                      gridTemplateColumns: '1fr 1fr',
                      gap: 8,
                    }}
                  >
                    {QUICK_START_CARDS.map((card) => (
                      <QuickCard key={card.templateId} card={card} onSelect={handleSelectTemplate} />
                    ))}
                  </div>
                </section>
              )}

              {/* ── Category filter */}
              <section style={{ marginBottom: 14 }}>
                <div
                  style={{
                    fontSize: 11,
                    fontWeight: 700,
                    color: 'var(--text-dim, #888)',
                    letterSpacing: '0.08em',
                    textTransform: 'uppercase',
                    marginBottom: 8,
                  }}
                >
                  Browse by topic
                </div>
                <select
                  value={activeCategory}
                  onChange={(e) => setActiveCategory(e.target.value)}
                  data-cy="category-select"
                  style={{
                    width: '100%',
                    padding: '9px 12px',
                    borderRadius: 9,
                    border: '1px solid var(--border, #333)',
                    background: 'var(--bg-elevated, #111)',
                    color: 'var(--text-primary, #fff)',
                    fontSize: 13,
                    cursor: 'pointer',
                    outline: 'none',
                    appearance: 'none',
                    backgroundImage:
                      'url("data:image/svg+xml,%3Csvg xmlns=\'http://www.w3.org/2000/svg\' width=\'10\' height=\'6\'%3E%3Cpath d=\'M0 0l5 6 5-6z\' fill=\'%23888\'/%3E%3C/svg%3E")',
                    backgroundRepeat: 'no-repeat',
                    backgroundPosition: 'right 12px center',
                    paddingRight: 32,
                  }}
                >
                  <option value="">All topics</option>
                  {CATEGORIES.map((cat) => (
                    <option key={cat.id} value={cat.id}>
                      {cat.label}
                    </option>
                  ))}
                </select>
              </section>

              {/* ── Template list */}
              <section style={{ marginBottom: 8 }}>
                {(search !== '' || activeCategory !== '') && (
                  <div
                    style={{
                      fontSize: 11,
                      fontWeight: 700,
                      color: 'var(--text-dim, #888)',
                      letterSpacing: '0.08em',
                      textTransform: 'uppercase',
                      marginBottom: 8,
                    }}
                  >
                    {filteredTemplates.length} helper{filteredTemplates.length !== 1 ? 's' : ''} found
                  </div>
                )}
                {(search !== '' || activeCategory !== '') && filteredTemplates.length === 0 && (
                  <div
                    style={{
                      padding: '24px 0',
                      textAlign: 'center',
                      color: 'var(--text-dim, #888)',
                      fontSize: 13,
                    }}
                  >
                    No helpers match your search.
                    <br />
                    <button
                      onClick={() => {
                        setSearch('');
                        setActiveCategory('');
                      }}
                      style={{
                        marginTop: 8,
                        background: 'transparent',
                        border: 'none',
                        color: 'var(--accent, #7c3aed)',
                        fontSize: 13,
                        cursor: 'pointer',
                        textDecoration: 'underline',
                      }}
                    >
                      Clear filters
                    </button>
                  </div>
                )}
                {(search !== '' || activeCategory !== '') && filteredTemplates.length > 0 && (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                    {filteredTemplates.map((t) => (
                      <TemplateCard key={t.id} template={t} onSelect={handleSelectTemplate} />
                    ))}
                  </div>
                )}

                {/* Full library shown when no filter */}
                {search === '' && activeCategory === '' && (
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                    <div
                      style={{
                        fontSize: 11,
                        fontWeight: 700,
                        color: 'var(--text-dim, #888)',
                        letterSpacing: '0.08em',
                        textTransform: 'uppercase',
                        marginBottom: 4,
                      }}
                    >
                      All helpers
                    </div>
                    {PROMPT_LIBRARY.map((t) => (
                      <TemplateCard key={t.id} template={t} onSelect={handleSelectTemplate} />
                    ))}
                  </div>
                )}
              </section>

              {/* ── Dictionary */}
              <PromptDictionaryPanel />
            </>
          )}
        </div>
      </div>
    </>
  );
}
