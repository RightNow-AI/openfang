'use client';

import { useState, useEffect, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';
import { track } from '../../lib/telemetry';
import AgentTemplateDetailDrawer from './AgentTemplateDetailDrawer';

// ──────────────────────────────────────────────────────────────────────────────
// Static config
// ──────────────────────────────────────────────────────────────────────────────

const AGENT_STARTERS = [
  {
    id: 'agency_core_agents',
    title: 'Agency starter agents',
    subtitle: 'Manage clients, research, plan tasks and get approvals.',
    icon: '🏢',
    color: '#7C3AED',
    includesAgents: ['Client Manager', 'Research Agent', 'Task Planner', 'Approval Agent'],
    template_names: ['client_manager', 'researcher', 'task_planner', 'approval_agent'],
    mode: 'agency',
  },
  {
    id: 'growth_core_agents',
    title: 'Growth starter agents',
    subtitle: 'Research ads, write hooks, scripts and follow-up emails.',
    icon: '🚀',
    color: '#059669',
    includesAgents: ['Ad Researcher', 'Hook Writer', 'Script Writer', 'Email Follow-up Agent'],
    template_names: ['ad_researcher', 'hook_writer', 'script_writer', 'email_followup'],
    mode: 'growth',
  },
  {
    id: 'school_core_agents',
    title: 'School starter agents',
    subtitle: 'Build curricula, lessons, support students and grow community.',
    icon: '🎓',
    color: '#D97706',
    includesAgents: ['Curriculum Architect', 'Lesson Builder', 'Student Success Agent', 'Community Agent'],
    template_names: ['curriculum_architect', 'lesson_builder', 'student_success', 'community_agent'],
    mode: 'school',
  },
];

const WIZARD_GOALS = [
  { id: 'client-management', label: 'Manage clients', icon: '🤝' },
  { id: 'campaign-building', label: 'Run campaigns', icon: '📣' },
  { id: 'research', label: 'Do research', icon: '🔍' },
  { id: 'content', label: 'Create content', icon: '✍️' },
  { id: 'course-building', label: 'Build courses', icon: '📚' },
  { id: 'student-support', label: 'Support students', icon: '🎓' },
  { id: 'not-sure', label: "I'm not sure", icon: '🤔' },
];

const WIZARD_MODES = [
  { id: 'agency', label: 'Agency', icon: '🏢', desc: 'Client services & delivery' },
  { id: 'growth', label: 'Growth', icon: '📈', desc: 'Marketing & acquisition' },
  { id: 'school', label: 'School / Course', icon: '🏫', desc: 'Education & coaching' },
  { id: 'general', label: 'General', icon: '⚡', desc: 'Works for anything' },
];

const WIZARD_STYLES = [
  { id: 'simple', label: 'Just show me starter packs', icon: '📦', desc: 'Pick a bundle and go' },
  { id: 'recommended', label: 'Show me curated picks', icon: '⭐', desc: 'Filtered for my goal' },
  { id: 'show-all', label: 'Show me everything', icon: '🗂️', desc: 'Browse full catalog' },
];

const GOAL_MODE_MAP = {
  'client-management': 'agency',
  'campaign-building': 'growth',
  research: 'agency',
  content: 'growth',
  'course-building': 'school',
  'student-support': 'school',
  'not-sure': 'general',
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

function normalizeEntry(raw) {
  return {
    catalog_id: raw.catalog_id || raw.id || `cat-${Math.random().toString(36).slice(2)}`,
    agent_id: raw.agent_id || null,
    name: raw.name || 'Unnamed Agent',
    description: raw.description || '',
    division: raw.division || 'general',
    source: raw.source || 'builtin',
    tags: Array.isArray(raw.tags) ? raw.tags : [],
    enabled: raw.enabled ?? true,
    best_for: raw.best_for || '',
    avoid_for: raw.avoid_for || '',
    example: raw.example || '',
    purpose: raw.purpose || '',
    role: raw.role || '',
  };
}

function divisionColor(div) {
  const map = {
    agency: '#7C3AED',
    growth: '#059669',
    school: '#D97706',
    general: '#0EA5E9',
  };
  return map[div] || '#64748B';
}

// ──────────────────────────────────────────────────────────────────────────────
// Shared UI primitives
// ──────────────────────────────────────────────────────────────────────────────

function WizardStep({ step, total, title, children }) {
  return (
    <div style={{ padding: '0 0 8px 0' }}>
      <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 6 }}>
        Step {step} of {total}
      </div>
      <div
        style={{
          fontSize: 18,
          fontWeight: 700,
          color: 'var(--text)',
          marginBottom: 20,
        }}
      >
        {title}
      </div>
      {children}
    </div>
  );
}

function ChoiceBtn({ selected, onClick, icon, label, desc }) {
  return (
    <button
      onClick={onClick}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 12,
        padding: '12px 16px',
        borderRadius: 10,
        border: `2px solid ${selected ? 'var(--accent)' : 'var(--border)'}` ,
        background: selected ? 'var(--accent-subtle)' : 'var(--bg-elevated)',
        color: 'var(--text)',
        width: '100%',
        cursor: 'pointer',
        marginBottom: 8,
        textAlign: 'left',
        transition: 'border-color 0.15s',
      }}
    >
      <span style={{ fontSize: 22 }}>{icon}</span>
      <span>
        <span style={{ fontSize: 14, fontWeight: 600, display: 'block' }}>{label}</span>
        {desc && (
          <span style={{ fontSize: 12, color: 'var(--text-dim)', display: 'block', marginTop: 2 }}>
            {desc}
          </span>
        )}
      </span>
      {selected && (
        <span style={{ marginLeft: 'auto', color: 'var(--accent)', fontSize: 18 }}>✓</span>
      )}
    </button>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Wizard steps
// ──────────────────────────────────────────────────────────────────────────────

function StepGoal({ value, onChange }) {
  return (
    <WizardStep step={1} total={5} title="What are you trying to do?">
      {WIZARD_GOALS.map((g) => (
        <ChoiceBtn
          key={g.id}
          selected={value === g.id}
          onClick={() => onChange(g.id)}
          icon={g.icon}
          label={g.label}
        />
      ))}
    </WizardStep>
  );
}

function StepMode({ value, onChange }) {
  return (
    <WizardStep step={2} total={5} title="What kind of work do you do?">
      {WIZARD_MODES.map((m) => (
        <ChoiceBtn
          key={m.id}
          selected={value === m.id}
          onClick={() => onChange(m.id)}
          icon={m.icon}
          label={m.label}
          desc={m.desc}
        />
      ))}
    </WizardStep>
  );
}

function StepStyle({ value, onChange }) {
  return (
    <WizardStep step={3} total={5} title="How would you like to browse agents?">
      {WIZARD_STYLES.map((s) => (
        <ChoiceBtn
          key={s.id}
          selected={value === s.id}
          onClick={() => onChange(s.id)}
          icon={s.icon}
          label={s.label}
          desc={s.desc}
        />
      ))}
    </WizardStep>
  );
}

function StepRecommendations({ goal, mode, style, entries, onSpawnPack, spawnStatus }) {
  const pack = AGENT_STARTERS.find((s) => s.mode === mode || (mode === 'general' && s.id === 'agency_core_agents'));
  const filtered =
    style === 'show-all'
      ? entries
      : style === 'recommended'
      ? entries.filter((e) => e.division === mode)
      : [];

  return (
    <WizardStep step={4} total={5} title="Here's what we recommend">
      {style === 'simple' && pack && (
        <div
          style={{
            background: 'var(--bg-elevated)',
            border: `2px solid ${pack.color}40`,
            borderRadius: 12,
            padding: '16px 20px',
            marginBottom: 16,
          }}
        >
          <div style={{ fontSize: 28, marginBottom: 8 }}>{pack.icon}</div>
          <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', marginBottom: 4 }}>
            {pack.title}
          </div>
          <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 12 }}>{pack.subtitle}</div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginBottom: 14 }}>
            {pack.includesAgents.map((a) => (
              <span
                key={a}
                style={{
                  fontSize: 12,
                  padding: '2px 10px',
                  borderRadius: 100,
                  background: `${pack.color}22`,
                  color: pack.color,
                  border: `1px solid ${pack.color}44`,
                }}
              >
                {a}
              </span>
            ))}
          </div>
          {spawnStatus[pack.id] === 'done' ? (
            <div style={{ color: 'var(--success)', fontSize: 14, fontWeight: 600 }}>
              ✓ Agents spawned
            </div>
          ) : (
            <button
              onClick={() => onSpawnPack(pack)}
              disabled={spawnStatus[pack.id] === 'loading'}
              style={{
                padding: '8px 18px',
                borderRadius: 8,
                border: 'none',
                background: pack.color,
                color: '#fff',
                fontWeight: 600,
                cursor: spawnStatus[pack.id] === 'loading' ? 'not-allowed' : 'pointer',
                fontSize: 14,
                opacity: spawnStatus[pack.id] === 'loading' ? 0.7 : 1,
              }}
            >
              {spawnStatus[pack.id] === 'loading' ? 'Spawning…' : 'Spawn this pack'}
            </button>
          )}
          {spawnStatus[pack.id] === 'error' && (
            <div style={{ color: 'var(--danger)', fontSize: 12, marginTop: 6 }}>
              Some agents failed to spawn. Continue to see what was created.
            </div>
          )}
        </div>
      )}
      {(style === 'recommended' || style === 'show-all') && (
        <>
          <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 12 }}>
            {filtered.length} agent{filtered.length !== 1 ? 's' : ''} found
          </div>
          <div
            style={{
              maxHeight: 260,
              overflowY: 'auto',
              display: 'flex',
              flexDirection: 'column',
              gap: 8,
            }}
          >
            {filtered.map((e) => (
              <div
                key={e.catalog_id}
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border)',
                  borderRadius: 8,
                  padding: '10px 14px',
                }}
              >
                <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--text)' }}>{e.name}</div>
                <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{e.description}</div>
              </div>
            ))}
            {filtered.length === 0 && (
              <div style={{ color: 'var(--text-dim)', fontSize: 13 }}>None in this category.</div>
            )}
          </div>
        </>
      )}
    </WizardStep>
  );
}

function StepFinish({ mode, style, onClose }) {
  return (
    <WizardStep step={5} total={5} title="You're ready to go">
      <div
        style={{
          textAlign: 'center',
          padding: '20px 0 4px',
        }}
      >
        <div style={{ fontSize: 52, marginBottom: 12 }}>🎉</div>
        <div style={{ fontSize: 15, color: 'var(--text-soft)', marginBottom: 20 }}>
          Your agents are being set up.
          {style === 'simple'
            ? " They'll appear in My Agents once ready."
            : ' Head to My Agents to configure them.'}
        </div>
        <button
          onClick={onClose}
          style={{
            padding: '10px 28px',
            borderRadius: 8,
            border: 'none',
            background: 'var(--accent)',
            color: '#fff',
            fontWeight: 700,
            cursor: 'pointer',
            fontSize: 15,
          }}
        >
          Go to My Agents
        </button>
      </div>
    </WizardStep>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Wizard modal
// ──────────────────────────────────────────────────────────────────────────────

function AgentCatalogWizard({ entries, onClose, onSpawned }) {
  const [step, setStep] = useState(1);
  const [goal, setGoal] = useState('');
  const [mode, setMode] = useState('');
  const [style, setStyle] = useState('');
  const [spawnStatus, setSpawnStatus] = useState({});

  const suggestedMode = goal ? GOAL_MODE_MAP[goal] : '';

  function nextStep() {
    if (step === 1 && !goal) return;
    if (step === 2 && !mode) {
      // auto-advance with suggestion if needed
    }
    if (step === 3 && !style) return;
    setStep((s) => s + 1);
  }

  function prevStep() {
    setStep((s) => Math.max(1, s - 1));
  }

  async function handleSpawnPack(pack) {
    setSpawnStatus((prev) => ({ ...prev, [pack.id]: 'loading' }));
    let hasError = false;
    for (const templateName of pack.template_names) {
      try {
        await apiClient.post('/api/agents/spawn', {
          template_name: templateName,
          name: templateName.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase()),
        });
      } catch {
        hasError = true;
      }
    }
    setSpawnStatus((prev) => ({ ...prev, [pack.id]: hasError ? 'error' : 'done' }));
    track('catalog_wizard_pack_spawned', { pack_id: pack.id });
    if (onSpawned) onSpawned();
  }

  function handleGoalChange(v) {
    setGoal(v);
    if (GOAL_MODE_MAP[v] && GOAL_MODE_MAP[v] !== 'general') {
      setMode(GOAL_MODE_MAP[v]);
    }
  }

  const canNext =
    (step === 1 && !!goal) ||
    (step === 2 && !!mode) ||
    (step === 3 && !!style) ||
    step === 4;

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0,0,0,0.72)',
        zIndex: 9999,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        style={{
          background: 'var(--bg-card)',
          borderRadius: 16,
          width: '100%',
          maxWidth: 520,
          maxHeight: '92vh',
          overflowY: 'auto',
          padding: '28px 32px',
          boxShadow: '0 24px 64px rgba(0,0,0,0.6)',
          position: 'relative',
        }}
      >
        <button
          onClick={onClose}
          style={{
            position: 'absolute',
            top: 16,
            right: 16,
            background: 'none',
            border: 'none',
            color: 'var(--text-dim)',
            cursor: 'pointer',
            fontSize: 20,
            padding: '2px 6px',
          }}
        >
          ✕
        </button>

        {step === 1 && <StepGoal value={goal} onChange={handleGoalChange} />}
        {step === 2 && (
          <StepMode
            value={mode}
            onChange={setMode}
          />
        )}
        {step === 3 && <StepStyle value={style} onChange={setStyle} />}
        {step === 4 && (
          <StepRecommendations
            goal={goal}
            mode={mode}
            style={style}
            entries={entries}
            onSpawnPack={handleSpawnPack}
            spawnStatus={spawnStatus}
          />
        )}
        {step === 5 && (
          <StepFinish mode={mode} style={style} onClose={() => { onClose(); }} />
        )}

        {step < 5 && (
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              marginTop: 24,
              gap: 10,
            }}
          >
            {step > 1 ? (
              <button
                onClick={prevStep}
                style={{
                  padding: '8px 20px',
                  borderRadius: 8,
                  border: '1px solid var(--border-strong)',
                  background: 'transparent',
                  color: 'var(--text-dim)',
                  cursor: 'pointer',
                  fontSize: 14,
                }}
              >
                ← Back
              </button>
            ) : (
              <div />
            )}
            <button
              onClick={nextStep}
              disabled={!canNext}
              style={{
                padding: '8px 24px',
                borderRadius: 8,
                border: 'none',
                background: canNext ? 'var(--accent)' : 'var(--border-strong)',
                color: canNext ? 'var(--text-inverse)' : 'var(--text-dim)',
                fontWeight: 700,
                cursor: canNext ? 'pointer' : 'not-allowed',
                fontSize: 14,
              }}
            >
              {step === 4 ? 'Finish →' : 'Next →'}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Cards
// ──────────────────────────────────────────────────────────────────────────────

function AgentStarterPackCard({ pack, onSpawn, spawnStatus }) {
  const status = spawnStatus[pack.id];
  return (
    <div
      style={{
        background: 'var(--bg-card)',
        border: `2px solid ${pack.color}33`,
        borderRadius: 14,
        padding: '20px 22px',
        display: 'flex',
        flexDirection: 'column',
        gap: 10,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <span
          style={{
            fontSize: 28,
            background: `${pack.color}22`,
            borderRadius: 10,
            padding: '6px 10px',
          }}
        >
          {pack.icon}
        </span>
        <div>
          <div style={{ fontSize: 15, fontWeight: 700, color: 'var(--text)' }}>{pack.title}</div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{pack.subtitle}</div>
        </div>
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
        {pack.includesAgents.map((a) => (
          <span
            key={a}
            style={{
              fontSize: 11,
              padding: '2px 9px',
              borderRadius: 100,
              background: `${pack.color}18`,
              color: pack.color,
              border: `1px solid ${pack.color}40`,
            }}
          >
            {a}
          </span>
        ))}
      </div>
      {status === 'done' ? (
        <div style={{ color: 'var(--success)', fontSize: 13, fontWeight: 600 }}>✓ Agents spawned</div>
      ) : (
        <button
          onClick={() => onSpawn(pack)}
          disabled={status === 'loading'}
          style={{
            marginTop: 4,
            padding: '8px 16px',
            borderRadius: 8,
            border: 'none',
            background: pack.color,
            color: '#fff',
            fontWeight: 700,
            cursor: status === 'loading' ? 'not-allowed' : 'pointer',
            fontSize: 13,
            opacity: status === 'loading' ? 0.7 : 1,
            alignSelf: 'flex-start',
          }}
        >
          {status === 'loading' ? 'Spawning…' : 'Set up this pack →'}
        </button>
      )}
      {status === 'error' && (
        <div style={{ color: 'var(--danger)', fontSize: 12 }}>Some agents failed — check My Agents.</div>
      )}
    </div>
  );
}

function AgentCardSimple({ entry }) {
  const color = divisionColor(entry.division);
  return (
    <div
      style={{
        background: 'var(--bg-elevated)',
        border: '1px solid var(--border)',
        borderRadius: 10,
        padding: '14px 16px',
        display: 'flex',
        alignItems: 'flex-start',
        gap: 12,
      }}
    >
      <span
        style={{
          fontSize: 22,
          background: `${color}22`,
          borderRadius: 8,
          padding: '4px 8px',
          flexShrink: 0,
        }}
      >
        🤖
      </span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)' }}>{entry.name}</div>
        <div
          style={{
            fontSize: 12,
            color: 'var(--text-dim)',
            marginTop: 3,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {entry.description}
        </div>
        <div style={{ display: 'flex', gap: 6, marginTop: 6 }}>
          <span
            style={{
              fontSize: 10,
              padding: '2px 8px',
              borderRadius: 100,
              background: `${color}18`,
              color: color,
              border: `1px solid ${color}40`,
            }}
          >
            {entry.division}
          </span>
          {entry.tags.slice(0, 2).map((t) => (
            <span
              key={t}
              style={{
                fontSize: 10,
                padding: '2px 8px',
                borderRadius: 100,
                background: 'var(--bg-card)',
                color: 'var(--text-dim)',
              }}
            >
              {t}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Tabs
// ──────────────────────────────────────────────────────────────────────────────

function RecommendedAgentsTab({ entries, onOpenWizard }) {
  return (
    <div>
      {/* Hero CTA */}
      <div
        style={{
          background: 'linear-gradient(135deg, var(--accent-dim) 0%, var(--accent) 100%)',
          borderRadius: 16,
          padding: '28px 32px',
          marginBottom: 28,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: 16,
        }}
      >
        <div>
          <div style={{ fontSize: 22, fontWeight: 800, color: '#fff', marginBottom: 6 }}>
            Set up your AI agents
          </div>
          <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.8)' }}>
            {"Tell us what you're working on and we'll pick the right agents."}
          </div>
        </div>
        <button
          onClick={onOpenWizard}
          style={{
            padding: '12px 24px',
            borderRadius: 10,
            border: 'none',
            background: '#fff',
            color: 'var(--accent)',
            fontWeight: 800,
            cursor: 'pointer',
            fontSize: 15,
            whiteSpace: 'nowrap',
          }}
        >
          Set up for me →
        </button>
      </div>

      {/* Starter packs */}
      <div style={{ fontSize: 15, fontWeight: 700, color: 'var(--text-soft)', marginBottom: 14 }}>
        Starter packs
      </div>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))',
          gap: 14,
        }}
      >
        {AGENT_STARTERS.map((pack) => (
          <AgentStarterPackCard
            key={pack.id}
            pack={pack}
            onSpawn={() => {}}
            spawnStatus={{}}
          />
        ))}
      </div>
    </div>
  );
}

function MyAgentsTab({ entries, loading, onOpenWizard }) {
  if (loading) {
    return (
      <div style={{ color: 'var(--text-dim)', fontSize: 14, padding: '40px 0', textAlign: 'center' }}>
        Loading agents…
      </div>
    );
  }

  const myAgents = entries.filter((e) => e.agent_id != null);

  if (myAgents.length === 0) {
    return (
      <div
        style={{
          textAlign: 'center',
          padding: '64px 32px',
          background: 'var(--bg-elevated)',
          borderRadius: 16,
          border: '1px dashed var(--border)',
        }}
      >
        <div style={{ fontSize: 40, marginBottom: 12 }}>🤖</div>
        <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text-soft)', marginBottom: 8 }}>
          No agents yet
        </div>
        <div style={{ fontSize: 14, color: 'var(--text-dim)', marginBottom: 20 }}>
          Use the setup wizard to get your first agents running.
        </div>
        <button
          onClick={onOpenWizard}
          style={{
            padding: '10px 24px',
            borderRadius: 8,
            border: 'none',
            background: 'var(--accent)',
            color: '#fff',
            fontWeight: 700,
            cursor: 'pointer',
            fontSize: 14,
          }}
        >
          Set up agents →
        </button>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      {myAgents.map((e) => (
        <AgentCardSimple key={e.catalog_id} entry={e} />
      ))}
    </div>
  );
}

function TemplatesAgentsTab({ onSpawn, spawnStatus }) {
  return (
    <div>
      <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 16 }}>
        Spawn a full starter pack in one click.
      </div>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))',
          gap: 14,
        }}
      >
        {AGENT_STARTERS.map((pack) => (
          <AgentStarterPackCard
            key={pack.id}
            pack={pack}
            onSpawn={onSpawn}
            spawnStatus={spawnStatus}
          />
        ))}
      </div>
    </div>
  );
}

function AdvancedAgentsTab({ entries, loading, error, onRefresh }) {
  const [filter, setFilter] = useState('');
  const [expanded, setExpanded] = useState(null);
  const [spawnName, setSpawnName] = useState('');
  const [spawnError, setSpawnError] = useState('');
  const [preflightError, setPreflightError] = useState('');
  const [preflightWarning, setPreflightWarning] = useState('');
  const [spawning, setSpawning] = useState(false);

  const filtered = entries.filter(
    (e) =>
      !filter ||
      e.name.toLowerCase().includes(filter.toLowerCase()) ||
      e.description.toLowerCase().includes(filter.toLowerCase()) ||
      e.tags.some((t) => t.toLowerCase().includes(filter.toLowerCase()))
  );

  async function handleSpawn(entry) {
    setSpawnError('');
    setPreflightError('');
    setPreflightWarning('');
    if (!spawnName.trim()) {
      setSpawnError('Name is required');
      return;
    }
    setSpawning(true);
    try {
      await apiClient.post('/api/agents/spawn', {
        template_name: entry.catalog_id,
        name: spawnName.trim(),
      });
      setSpawnName('');
      setExpanded(null);
      onRefresh();
      track('catalog_agent_spawned', { catalog_id: entry.catalog_id });
    } catch (err) {
      const msg = err?.message || 'Failed to spawn';
      if (msg.includes('preflight')) {
        setPreflightError(msg);
      } else {
        setSpawnError(msg);
      }
    } finally {
      setSpawning(false);
    }
  }

  if (error) {
    return (
      <div data-cy="catalog-error" style={{ color: 'var(--danger)', padding: '16px 0' }}>
        {error}
      </div>
    );
  }

  return (
    <div>
      <div style={{ marginBottom: 16 }}>
        <input
          data-cy="catalog-filter"
          type="text"
          placeholder="Filter agents…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          style={{
            width: '100%',
            padding: '10px 14px',
            borderRadius: 8,
            border: '1px solid var(--border)',
            background: 'var(--bg-elevated)',
            color: 'var(--text)',
            fontSize: 14,
            outline: 'none',
          }}
        />
      </div>

      {loading ? (
        <div style={{ color: 'var(--text-dim)', fontSize: 14, textAlign: 'center', padding: '40px 0' }}>
          Loading…
        </div>
      ) : filtered.length === 0 ? (
        filter ? (
          <div data-cy="catalog-filter-empty" style={{ color: 'var(--text-dim)', fontSize: 14 }}>
            {`No agents match "${filter}".`}
          </div>
        ) : (
          <div data-cy="catalog-empty" style={{ color: 'var(--text-dim)', fontSize: 14 }}>
            No agents in catalog.
          </div>
        )
      ) : (
        <div
          data-cy="catalog-grid"
          style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}
        >
          {filtered.map((e) => (
            <div
              key={e.catalog_id}
              data-cy="catalog-card"
              style={{
                background: 'var(--bg-elevated)',
                border: `1px solid ${expanded === e.catalog_id ? 'var(--accent)' : 'var(--border)'}` ,
                borderRadius: 10,
                padding: '14px 16px',
              }}
            >
              <div
                style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)', marginBottom: 4 }}
              >
                {e.name}
              </div>
              <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 10 }}>
                {e.description}
              </div>
              <div style={{ display: 'flex', gap: 8, justifyContent: 'space-between' }}>
                <button
                  data-cy="catalog-details-btn"
                  onClick={() => setExpanded(expanded === e.catalog_id ? null : e.catalog_id)}
                  style={{
                    fontSize: 12,
                    padding: '4px 12px',
                    borderRadius: 6,
                    border: '1px solid var(--border-strong)',
                    background: 'transparent',
                    color: 'var(--text-dim)',
                    cursor: 'pointer',
                  }}
                >
                  {expanded === e.catalog_id ? 'Hide details' : 'Details'}
                </button>
                <button
                  data-cy="catalog-toggle-btn"
                  onClick={() => setExpanded(expanded === e.catalog_id ? null : e.catalog_id)}
                  style={{
                    fontSize: 12,
                    padding: '4px 12px',
                    borderRadius: 6,
                    border: 'none',
                    background: 'var(--accent)',
                    color: '#fff',
                    cursor: 'pointer',
                  }}
                >
                  Spawn
                </button>
              </div>

              {expanded === e.catalog_id && (
                <div
                  data-cy="agent-detail-overlay"
                  style={{ marginTop: 12, borderTop: '1px solid var(--border)', paddingTop: 12 }}
                >
                  <div data-cy="agent-detail-panel">
                    {e.best_for && (
                      <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 4 }}>
                        <strong style={{ color: 'var(--text-soft)' }}>Best for:</strong> {e.best_for}
                      </div>
                    )}
                    {e.example && (
                      <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 10 }}>
                        <strong style={{ color: 'var(--text-soft)' }}>Example:</strong> {e.example}
                      </div>
                    )}
                    {preflightWarning && (
                      <div data-cy="preflight-warning" style={{ color: 'var(--warning)', fontSize: 12, marginBottom: 6 }}>
                        ⚠ {preflightWarning}
                        <button
                          data-cy="spawn-anyway-btn"
                          onClick={() => handleSpawn(e)}
                          style={{
                            marginLeft: 8,
                            fontSize: 11,
                            padding: '2px 8px',
                            borderRadius: 4,
                            border: '1px solid var(--warning)',
                            background: 'transparent',
                            color: 'var(--warning)',
                            cursor: 'pointer',
                          }}
                        >
                          Spawn anyway
                        </button>
                      </div>
                    )}
                    {preflightError && (
                      <div data-cy="preflight-error" style={{ color: 'var(--danger)', fontSize: 12, marginBottom: 6 }}>
                        {preflightError}
                      </div>
                    )}
                    {spawnError && (
                      <div data-cy="spawn-error" style={{ color: 'var(--danger)', fontSize: 12, marginBottom: 6 }}>
                        {spawnError}
                      </div>
                    )}
                    <div style={{ display: 'flex', gap: 8, marginTop: 6 }}>
                      <input
                        data-cy="spawn-name-input"
                        type="text"
                        placeholder="Agent name…"
                        value={spawnName}
                        onChange={(ev) => setSpawnName(ev.target.value)}
                        style={{
                          flex: 1,
                          padding: '6px 10px',
                          borderRadius: 6,
                          border: '1px solid var(--border)',
                          background: 'var(--bg-elevated)',
                          color: 'var(--text)',
                          fontSize: 13,
                          outline: 'none',
                        }}
                      />
                      <button
                        data-cy="spawn-btn"
                        onClick={() => handleSpawn(e)}
                        disabled={spawning}
                        style={{
                          padding: '6px 14px',
                          borderRadius: 6,
                          border: 'none',
                          background: 'var(--accent)',
                          color: '#fff',
                          fontWeight: 700,
                          cursor: spawning ? 'not-allowed' : 'pointer',
                          fontSize: 13,
                          opacity: spawning ? 0.7 : 1,
                        }}
                      >
                        {spawning ? '…' : 'Spawn'}
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Main export
// ──────────────────────────────────────────────────────────────────────────────

export default function AgentCatalogPageV2({ initialEntries = [] }) {
  const [tab, setTab] = useState('recommended');
  const [entries, setEntries] = useState(initialEntries.map(normalizeEntry));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [showWizard, setShowWizard] = useState(false);
  const [packSpawnStatus, setPackSpawnStatus] = useState({});
  const [drawerTemplateId, setDrawerTemplateId] = useState(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/agents/catalog');
      const raw = data?.agents ?? (Array.isArray(data) ? data : []);
      setEntries(raw.map(normalizeEntry));
    } catch (err) {
      setError(err?.message || 'Failed to load catalog');
    } finally {
      setLoading(false);
    }
  }, []);

  async function handlePackSpawn(pack) {
    setPackSpawnStatus((prev) => ({ ...prev, [pack.id]: 'loading' }));
    let hasError = false;
    for (const templateName of pack.template_names) {
      try {
        await apiClient.post('/api/agents/spawn', {
          template_name: templateName,
          name: templateName.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase()),
        });
      } catch {
        hasError = true;
      }
    }
    setPackSpawnStatus((prev) => ({ ...prev, [pack.id]: hasError ? 'error' : 'done' }));
    track('catalog_pack_spawned', { pack_id: pack.id });
    refresh();
  }

  const TABS = [
    { id: 'recommended', label: 'Recommended' },
    { id: 'my', label: 'My Agents' },
    { id: 'templates', label: 'Templates' },
    { id: 'advanced', label: 'Advanced' },
  ];

  return (
    <div data-cy="catalog-page" style={{ fontFamily: 'system-ui, sans-serif', color: 'var(--text)' }}>
      {/* Header */}
      <div style={{ marginBottom: 28 }}>
        <h1 style={{ fontSize: 28, fontWeight: 800, margin: 0, color: 'var(--text)' }}>
          Agent Catalog
        </h1>
        <p style={{ fontSize: 14, color: 'var(--text-dim)', margin: '6px 0 0' }}>
          Browse and spawn AI agents for your workflows.
        </p>
      </div>

      {/* Tabs */}
      <div
        style={{
          display: 'flex',
          gap: 4,
          borderBottom: '1px solid var(--border)',
          marginBottom: 24,
        }}
      >
        {TABS.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            style={{
              padding: '8px 18px',
              background: 'none',
              border: 'none',
              borderBottom: tab === t.id ? '2px solid var(--accent)' : '2px solid transparent',
              color: tab === t.id ? 'var(--accent)' : 'var(--text-dim)',
              fontWeight: tab === t.id ? 700 : 500,
              cursor: 'pointer',
              fontSize: 14,
              marginBottom: -1,
            }}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      {tab === 'recommended' && (
        <RecommendedAgentsTab entries={entries} onOpenWizard={() => setShowWizard(true)} onOpenDetail={id => setDrawerTemplateId(id)} />
      )}
      {tab === 'my' && (
        <MyAgentsTab entries={entries} loading={loading} onOpenWizard={() => setShowWizard(true)} onOpenDetail={id => setDrawerTemplateId(id)} />
      )}
      {tab === 'templates' && (
        <TemplatesAgentsTab onSpawn={handlePackSpawn} spawnStatus={packSpawnStatus} />
      )}
      {tab === 'advanced' && (
        <AdvancedAgentsTab
          entries={entries}
          loading={loading}
          error={error}
          onRefresh={refresh}
        />
      )}

      {/* Wizard */}
      {showWizard && (
        <AgentCatalogWizard
          entries={entries}
          onClose={() => setShowWizard(false)}
          onSpawned={refresh}
        />
      )}
      <AgentTemplateDetailDrawer
        open={!!drawerTemplateId}
        templateId={drawerTemplateId}
        onClose={() => setDrawerTemplateId(null)}
        onSpawn={async (id) => {
          await apiClient.post('/api/agents/spawn', { template_id: id });
          await refresh();
        }}
      />
    </div>
  );
}
