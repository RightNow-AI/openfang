'use client';
import { useState, useCallback, useEffect } from 'react';
import { apiClient } from '../../lib/api-client';
import { track } from '../../lib/telemetry';
import SkillDrawer from './SkillDrawer';

// ─── Starter Packs ────────────────────────────────────────────────────────────

const STARTER_PACKS = [
  {
    id: 'agency',
    title: 'Agency Starter Pack',
    description: 'Best for client work, follow-up, and planning.',
    bestFor: 'Agency Mode',
    includes: ['Web Research', 'Email', 'Planning', 'Document Reader'],
    installSkillNames: ['web_search', 'email', 'planning', 'document_reader'],
  },
  {
    id: 'growth',
    title: 'Growth Starter Pack',
    description: 'Best for campaigns, research, writing, and follow-up.',
    bestFor: 'Growth Mode',
    includes: ['Web Research', 'Email', 'Copywriting', 'Creative Planning'],
    installSkillNames: ['web_search', 'email', 'copywriter', 'creative_planning'],
  },
  {
    id: 'school',
    title: 'School Starter Pack',
    description: 'Best for lessons, files, reminders, and planning.',
    bestFor: 'School Mode',
    includes: ['Document Reader', 'Lesson Builder', 'Email', 'Planning'],
    installSkillNames: ['document_reader', 'lesson_builder', 'email', 'planning'],
  },
  {
    id: 'research',
    title: 'Research Pack',
    description: 'Best for competitor tracking, deep research, and analysis.',
    bestFor: 'Research & Analysis',
    includes: ['Web Research', 'Planning', 'Document Reader'],
    installSkillNames: ['web_search', 'planning', 'document_reader'],
  },
  {
    id: 'email',
    title: 'Email & Follow-up Pack',
    description: 'Best for writing, sending, and tracking emails.',
    bestFor: 'Email Workflows',
    includes: ['Email', 'Planning'],
    installSkillNames: ['email', 'planning'],
  },
];

const GOAL_TO_PACK_IDS = {
  'client-work':     ['agency'],
  'ads-campaigns':   ['growth'],
  'email-followup':  ['email'],
  'research':        ['research'],
  'school-course':   ['school'],
  'planning-chores': ['research', 'email'],
  'not-sure':        ['research', 'email', 'agency'],
};

// ─── Data normalizers ─────────────────────────────────────────────────────────

function normalizeSkill(raw, i) {
  return {
    name:          String(raw?.name ?? raw?.id ?? `skill-${i}`),
    description:   String(raw?.description ?? ''),
    runtime:       String(raw?.runtime ?? raw?.language ?? raw?.type ?? ''),
    installed:     raw?.installed !== false,
    enabled:       raw?.enabled !== false,
    bundled:       raw?.bundled ?? raw?.builtin ?? !raw?.custom ?? true,
    version:       String(raw?.version ?? ''),
    tool_count:    Number(raw?.tool_count ?? 0),
    used_by_count: Number(raw?.used_by_count ?? 0),
  };
}

function normalizeRegistryCard(raw) {
  return {
    name:        String(raw?.name ?? ''),
    description: String(raw?.description ?? ''),
    author:      raw?.author ?? null,
    version:     raw?.version ?? null,
    runtime:     String(raw?.runtime ?? raw?.type ?? ''),
    source:      raw?.source ?? 'registry',
    popularity:  raw?.popularity ?? null,
    installed:   !!raw?.installed,
    bundled:     !!raw?.bundled,
    installable: raw?.installable !== false && !raw?.bundled && !raw?.installed,
  };
}

// ─── StarterPackCard ──────────────────────────────────────────────────────────

function StarterPackCard({ pack, installedCount, installing, onInstall }) {
  const total = pack.installSkillNames.length;
  const allDone = installedCount >= total;
  return (
    <div
      data-cy="starter-pack-card"
      style={{
        border: '1px solid var(--border)',
        borderRadius: 10,
        padding: '18px 20px',
        display: 'flex',
        flexDirection: 'column',
        gap: 12,
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>{pack.title}</div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 3 }}>{pack.bestFor}</div>
        </div>
        {installedCount > 0 && (
          <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: '#22c55e20', color: '#22c55e' }}>
            {installedCount}/{total} installed
          </span>
        )}
      </div>
      <div style={{ fontSize: 13, color: 'var(--text-secondary, #ccc)', lineHeight: 1.5 }}>
        {pack.description}
      </div>
      <div>
        <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 5 }}>Includes:</div>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
          {pack.includes.map(inc => (
            <span key={inc} style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: 'var(--surface2)', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)' }}>
              {inc}
            </span>
          ))}
        </div>
      </div>
      <button
        data-cy="install-pack-btn"
        onClick={onInstall}
        disabled={installing || allDone}
        style={{
          padding: '8px 16px',
          borderRadius: 6,
          background: allDone ? 'transparent' : 'var(--accent)',
          color: allDone ? 'var(--text-dim)' : '#fff',
          border: allDone ? '1px solid var(--border)' : 'none',
          cursor: (installing || allDone) ? 'not-allowed' : 'pointer',
          fontWeight: 600,
          fontSize: 13,
        }}
      >
        {installing ? 'Installing…' : allDone ? '✓ Installed' : 'Install starter pack'}
      </button>
    </div>
  );
}

// ─── RecommendedSkillCard ─────────────────────────────────────────────────────

function RecommendedSkillCard({ skill, installing, installed, onInstall }) {
  const isAdded = installed || skill.installed || skill.bundled;
  return (
    <div style={{ border: '1px solid var(--border)', borderRadius: 8, padding: '12px 14px', display: 'flex', gap: 12, alignItems: 'flex-start' }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>{skill.name}</div>
        {skill.description && (
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{skill.description}</div>
        )}
      </div>
      <button
        onClick={onInstall}
        disabled={installing || isAdded || !skill.installable}
        style={{
          padding: '5px 12px',
          borderRadius: 6,
          background: isAdded ? 'transparent' : 'var(--accent)',
          color: isAdded ? 'var(--text-dim)' : '#fff',
          border: isAdded ? '1px solid var(--border)' : 'none',
          cursor: (installing || isAdded || !skill.installable) ? 'not-allowed' : 'pointer',
          fontWeight: 600,
          fontSize: 12,
          whiteSpace: 'nowrap',
          flexShrink: 0,
        }}
      >
        {installing ? 'Installing…' : isAdded ? '✓ Added' : 'Add helper'}
      </button>
    </div>
  );
}

// ─── InstalledSkillCardSimple ─────────────────────────────────────────────────

function InstalledSkillCardSimple({ skill, pending, onToggleEnabled, onOpenDetail }) {
  return (
    <div
      data-cy="installed-skill-card"
      style={{ border: '1px solid var(--border)', borderRadius: 8, padding: '14px 16px', display: 'flex', gap: 12, alignItems: 'center' }}
    >
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>{skill.name}</div>
        <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>
          {skill.description || 'No description available'}
        </div>
        {skill.used_by_count > 0 && (
          <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 3 }}>
            Used by {skill.used_by_count} agent{skill.used_by_count !== 1 ? 's' : ''}
          </div>
        )}
      </div>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexShrink: 0 }}>
        <button
          onClick={() => onToggleEnabled(!skill.enabled)}
          disabled={pending}
          style={{
            padding: '4px 12px',
            borderRadius: 999,
            background: skill.enabled ? '#22c55e20' : 'transparent',
            border: `1px solid ${skill.enabled ? '#22c55e' : 'var(--border)'}`,
            color: skill.enabled ? '#22c55e' : 'var(--text-dim)',
            cursor: pending ? 'wait' : 'pointer',
            fontSize: 12,
            fontWeight: 600,
          }}
        >
          {pending ? '…' : skill.enabled ? '● On' : '○ Off'}
        </button>
        <button
          onClick={onOpenDetail}
          style={{ padding: '4px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontSize: 12 }}
        >
          Details
        </button>
      </div>
    </div>
  );
}

// ─── InstalledSkillCardDetailed ───────────────────────────────────────────────

function InstalledSkillCardDetailed({ skill, pending, onToggleEnabled, onOpenDetail }) {
  return (
    <div data-cy="installed-skill-card" className="card" style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontWeight: 700, fontSize: 14 }}>{skill.name}</div>
          {skill.description && (
            <div className="text-sm text-dim" style={{ marginTop: 3, overflow: 'hidden', display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical' }}>
              {skill.description}
            </div>
          )}
        </div>
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
        {skill.runtime && <span className="badge badge-info" style={{ fontSize: 10 }}>{skill.runtime}</span>}
        <span className={`badge ${skill.bundled ? 'badge-success' : 'badge-warning'}`} style={{ fontSize: 10 }}>
          {skill.bundled ? 'Bundled' : 'Custom'}
        </span>
        {skill.version && <span className="badge badge-muted" style={{ fontSize: 10 }}>v{skill.version}</span>}
        {skill.tool_count > 0 && <span className="badge badge-dim" style={{ fontSize: 10 }}>{skill.tool_count} tool{skill.tool_count !== 1 ? 's' : ''}</span>}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
        <span style={{ fontSize: 12, color: 'var(--text-dim)' }}>
          {skill.used_by_count > 0
            ? `Referenced by ${skill.used_by_count} agent${skill.used_by_count !== 1 ? 's' : ''}`
            : 'Not referenced by any agent'}
        </span>
        <button
          className="btn btn-ghost btn-sm"
          onClick={() => onToggleEnabled(!skill.enabled)}
          disabled={pending}
          style={{ fontSize: 11, minWidth: 80 }}
        >
          {pending
            ? <span style={{ display: 'flex', alignItems: 'center', gap: 5 }}><div className="spinner" style={{ width: 10, height: 10 }} /></span>
            : skill.enabled
              ? <span style={{ color: 'var(--success)' }}>● Enabled</span>
              : <span style={{ color: 'var(--text-dim)' }}>○ Disabled</span>
          }
        </button>
      </div>
      <button className="btn btn-primary btn-sm" onClick={onOpenDetail} style={{ width: '100%' }}>
        Details
      </button>
    </div>
  );
}

// ─── BrowseSkillCard ──────────────────────────────────────────────────────────

function BrowseSkillCard({ card, installing, onInstall }) {
  const { name, description, author, version, runtime, installed, bundled, installable } = card;
  return (
    <div data-cy="browse-skill-card" className="card" style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontWeight: 700, fontSize: 14 }}>{name}</div>
          {description && (
            <div className="text-sm text-dim" style={{ marginTop: 3, overflow: 'hidden', display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical' }}>
              {description}
            </div>
          )}
        </div>
        <button
          data-cy="install-btn"
          data-skill={name}
          className="btn btn-primary btn-sm"
          onClick={() => installable && !installing && onInstall(name)}
          disabled={!installable || installing}
          style={{ whiteSpace: 'nowrap', minWidth: 84, flexShrink: 0 }}
        >
          {installing ? 'Installing…' : bundled ? 'Bundled' : installed ? 'Installed' : 'Add helper'}
        </button>
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
        {bundled && <span className="badge badge-success" style={{ fontSize: 10 }}>Bundled</span>}
        {installed && !bundled && <span className="badge badge-info" style={{ fontSize: 10 }}>Installed</span>}
        {runtime && <span className="badge badge-muted" style={{ fontSize: 10 }}>{runtime}</span>}
        {version && <span className="badge badge-dim" style={{ fontSize: 10 }}>v{version}</span>}
        {author && <span className="badge badge-dim" style={{ fontSize: 10 }}>by {author}</span>}
      </div>
    </div>
  );
}

// ─── Wizard helpers ───────────────────────────────────────────────────────────

const GOALS = [
  { value: 'client-work',    label: 'Run client work',            icon: '💼' },
  { value: 'ads-campaigns',  label: 'Create ads and campaigns',   icon: '📣' },
  { value: 'email-followup', label: 'Write emails and follow up', icon: '📧' },
  { value: 'research',       label: 'Research competitors',       icon: '🔍' },
  { value: 'school-course',  label: 'Build a school or course',   icon: '🎓' },
  { value: 'planning-chores',label: 'Plan tasks and chores',      icon: '📋' },
  { value: 'not-sure',       label: 'Not sure yet',               icon: '✨' },
];

const WORK_STYLES = [
  { value: 'simple',           label: 'Keep it simple',         sub: 'Show only what I need' },
  { value: 'recommended-only', label: 'Recommended tools only', sub: 'Pre-selected for my goal' },
  { value: 'show-everything',  label: 'Show me everything',     sub: 'Full control' },
];

const TOOLS = [
  { value: 'web',      label: 'Web research',       icon: '🌐' },
  { value: 'email',    label: 'Email',               icon: '📧' },
  { value: 'files',    label: 'Files and documents', icon: '📁' },
  { value: 'calendar', label: 'Calendar',            icon: '📅' },
  { value: 'slack',    label: 'Slack or Discord',    icon: '💬' },
  { value: 'none',     label: 'Nothing yet',         icon: '🚫' },
];

function WizardStep({ step, totalSteps = 5, title, subtitle, onBack, backLabel, onNext, nextLabel, nextDisabled, children }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 24 }}>
      <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>Step {step} of {totalSteps}</div>
      <div>
        <h2 style={{ fontSize: 20, fontWeight: 700, margin: 0 }}>{title}</h2>
        {subtitle && <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '6px 0 0', lineHeight: 1.5 }}>{subtitle}</p>}
      </div>
      <div>{children}</div>
      <div style={{ display: 'flex', gap: 10, justifyContent: 'flex-end', marginTop: 4 }}>
        {onBack && (
          <button
            onClick={onBack}
            style={{ padding: '8px 18px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontWeight: 500, fontSize: 13 }}
          >
            {backLabel ?? '← Back'}
          </button>
        )}
        {onNext && (
          <button
            onClick={onNext}
            disabled={nextDisabled}
            style={{ padding: '8px 18px', borderRadius: 6, background: 'var(--accent)', color: '#fff', border: 'none', cursor: nextDisabled ? 'not-allowed' : 'pointer', fontWeight: 600, fontSize: 13, opacity: nextDisabled ? 0.5 : 1 }}
          >
            {nextLabel ?? 'Next →'}
          </button>
        )}
      </div>
    </div>
  );
}

function ChoiceButton({ selected, onClick, children }) {
  return (
    <button
      onClick={onClick}
      style={{
        display: 'block',
        width: '100%',
        textAlign: 'left',
        padding: '12px 16px',
        borderRadius: 8,
        border: `1px solid ${selected ? 'var(--accent)' : 'var(--border)'}`,
        background: selected ? 'var(--accent-subtle)' : 'transparent',
        cursor: 'pointer',
        marginBottom: 8,
      }}
    >
      {children}
    </button>
  );
}

// ─── Wizard Steps ─────────────────────────────────────────────────────────────

function WizardStepGoal({ value, onSelect, onNext }) {
  return (
    <WizardStep step={1} title="What are you trying to do?" subtitle="We'll pick the right helpers based on your goal." onNext={onNext} nextDisabled={!value}>
      {GOALS.map(g => (
        <ChoiceButton key={g.value} selected={value === g.value} onClick={() => onSelect(g.value)}>
          <span style={{ fontSize: 16, marginRight: 10 }}>{g.icon}</span>
          <span style={{ fontSize: 14, fontWeight: value === g.value ? 700 : 400 }}>{g.label}</span>
        </ChoiceButton>
      ))}
    </WizardStep>
  );
}

function WizardStepWorkStyle({ value, onSelect, onBack, onNext }) {
  return (
    <WizardStep step={2} title="How do you want to work?" onBack={onBack} onNext={onNext} nextDisabled={!value}>
      {WORK_STYLES.map(ws => (
        <ChoiceButton key={ws.value} selected={value === ws.value} onClick={() => onSelect(ws.value)}>
          <div style={{ fontWeight: value === ws.value ? 700 : 400, fontSize: 14 }}>{ws.label}</div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{ws.sub}</div>
        </ChoiceButton>
      ))}
    </WizardStep>
  );
}

function WizardStepTools({ value, onToggle, onBack, onNext }) {
  return (
    <WizardStep step={3} title="What tools should be connected?" subtitle="You can always change this later." onBack={onBack} onNext={onNext}>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
        {TOOLS.map(t => {
          const sel = value.includes(t.value);
          return (
            <button
              key={t.value}
              onClick={() => onToggle(t.value)}
              style={{
                padding: '12px 14px',
                borderRadius: 8,
                border: `1px solid ${sel ? 'var(--accent)' : 'var(--border)'}`,
                background: sel ? 'var(--accent-subtle)' : 'transparent',
                cursor: 'pointer',
                textAlign: 'left',
              }}
            >
              <div style={{ fontSize: 18 }}>{t.icon}</div>
              <div style={{ fontSize: 13, fontWeight: sel ? 700 : 400, marginTop: 4 }}>{t.label}</div>
            </button>
          );
        })}
      </div>
    </WizardStep>
  );
}

function WizardStepRecommendations({ packs, skills, installingByName, installedNames, onInstallPack, onInstallSkill, onBack, onNext }) {
  return (
    <WizardStep step={4} title="Here are your recommended skills" subtitle="Install what looks useful. You can always add more later." onBack={onBack} onNext={onNext} nextLabel="Done →">
      {packs.length > 0 && (
        <div style={{ marginBottom: 12 }}>
          {packs.map(pack => {
            const installedCount = pack.installSkillNames.filter(n => installedNames.includes(n)).length;
            return (
              <div key={pack.id} style={{ marginBottom: 10 }}>
                <StarterPackCard
                  pack={pack}
                  installedCount={installedCount}
                  installing={pack.installSkillNames.some(n => !!installingByName[n])}
                  onInstall={() => onInstallPack(pack)}
                />
              </div>
            );
          })}
        </div>
      )}
      {skills.length > 0 && (
        <>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 8, marginTop: 4 }}>Individual helpers</div>
          {skills.map(skill => (
            <div key={skill.name} style={{ marginBottom: 6 }}>
              <RecommendedSkillCard
                skill={skill}
                installing={!!installingByName[skill.name]}
                installed={installedNames.includes(skill.name)}
                onInstall={() => onInstallSkill(skill.name)}
              />
            </div>
          ))}
        </>
      )}
      {packs.length === 0 && skills.length === 0 && (
        <div style={{ padding: 24, textAlign: 'center', color: 'var(--text-dim)', fontSize: 13 }}>
          Browse the registry after setup to find more helpers.
        </div>
      )}
    </WizardStep>
  );
}

function WizardStepFinish({ installedNames, onClose }) {
  const GO_OPTIONS = [
    { label: 'Go to Command Center', href: '/command-center/new' },
    { label: 'Go to Agency Mode',    href: '/agency/new' },
    { label: 'Go to Growth Mode',    href: '/growth/new' },
    { label: 'Go to School Mode',    href: '/school/new' },
  ];
  return (
    <WizardStep step={5} title="You're ready! 🎉" subtitle="Your helpers are installed. Here's what to do next.">
      {installedNames.length > 0 && (
        <div style={{ marginBottom: 20 }}>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 8 }}>Installed this session:</div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
            {installedNames.map(n => (
              <span key={n} style={{ fontSize: 12, padding: '3px 10px', borderRadius: 999, background: '#22c55e20', color: '#22c55e', border: '1px solid #22c55e44' }}>
                ✓ {n}
              </span>
            ))}
          </div>
        </div>
      )}
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {GO_OPTIONS.map(opt => (
          <a
            key={opt.href}
            href={opt.href}
            style={{ display: 'block', padding: '10px 16px', borderRadius: 8, border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', textDecoration: 'none', fontSize: 14, fontWeight: 500 }}
          >
            {opt.label} →
          </a>
        ))}
        <button
          onClick={onClose}
          style={{ padding: '10px 16px', borderRadius: 8, border: '1px solid var(--border)', background: 'transparent', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 14 }}
        >
          Review installed skills
        </button>
      </div>
    </WizardStep>
  );
}

// ─── GuidedSetupWizard ────────────────────────────────────────────────────────

function GuidedSetupWizard({ open, installedSkillNames, registryResults, onClose, onInstallSkill, onInstallPack }) {
  const [step, setStep]                         = useState(1);
  const [goal, setGoal]                         = useState(null);
  const [workStyle, setWorkStyle]               = useState(null);
  const [tools, setTools]                       = useState([]);
  const [sessionInstalled, setSessionInstalled] = useState([]);
  const [installingByName, setInstallingByName] = useState({});

  const toggleTool = (val) => {
    if (val === 'none') { setTools(['none']); return; }
    setTools(prev => {
      const without = prev.filter(t => t !== 'none');
      return without.includes(val) ? without.filter(t => t !== val) : [...without, val];
    });
  };

  const recommendedPacks = goal
    ? (GOAL_TO_PACK_IDS[goal] ?? []).map(id => STARTER_PACKS.find(p => p.id === id)).filter(Boolean)
    : STARTER_PACKS.slice(0, 2);

  const recommendedSkills = workStyle === 'show-everything'
    ? registryResults.filter(s => !s.bundled && !s.installed).slice(0, 6)
    : registryResults.filter(s => !s.bundled && !s.installed).slice(0, 3);

  const doInstallSkill = async (name) => {
    setInstallingByName(prev => ({ ...prev, [name]: true }));
    try {
      await onInstallSkill(name);
      setSessionInstalled(prev => prev.includes(name) ? prev : [...prev, name]);
    } finally {
      setInstallingByName(prev => ({ ...prev, [name]: false }));
    }
  };

  const doInstallPack = async (pack) => {
    const updates = {};
    pack.installSkillNames.forEach(n => { updates[n] = true; });
    setInstallingByName(prev => ({ ...prev, ...updates }));
    try {
      await onInstallPack(pack);
      setSessionInstalled(prev => [...new Set([...prev, ...pack.installSkillNames])]);
    } finally {
      const reset = {};
      pack.installSkillNames.forEach(n => { reset[n] = false; });
      setInstallingByName(prev => ({ ...prev, ...reset }));
    }
  };

  const handleClose = () => {
    setStep(1); setGoal(null); setWorkStyle(null); setTools([]); setSessionInstalled([]);
    onClose();
  };

  if (!open) return null;

  return (
    <div
      style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,0.6)', backdropFilter: 'blur(3px)', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 24 }}
      onClick={e => { if (e.target === e.currentTarget) handleClose(); }}
    >
      <div
        data-cy="guided-wizard"
        style={{ width: '100%', maxWidth: 560, background: 'var(--bg-elevated)', border: '1px solid var(--border)', borderRadius: 12, padding: 32, maxHeight: '90vh', overflowY: 'auto' }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 24 }}>
          <div style={{ fontSize: 13, fontWeight: 600 }}>Let&apos;s choose the right helpers</div>
          <button onClick={handleClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 18, color: 'var(--text-dim)' }}>✕</button>
        </div>

        {step === 1 && <WizardStepGoal value={goal} onSelect={setGoal} onNext={() => setStep(2)} />}
        {step === 2 && <WizardStepWorkStyle value={workStyle} onSelect={setWorkStyle} onBack={() => setStep(1)} onNext={() => setStep(3)} />}
        {step === 3 && <WizardStepTools value={tools} onToggle={toggleTool} onBack={() => setStep(2)} onNext={() => setStep(4)} />}
        {step === 4 && (
          <WizardStepRecommendations
            packs={recommendedPacks}
            skills={recommendedSkills}
            installingByName={installingByName}
            installedNames={[...installedSkillNames, ...sessionInstalled]}
            onInstallPack={doInstallPack}
            onInstallSkill={doInstallSkill}
            onBack={() => setStep(3)}
            onNext={() => setStep(5)}
          />
        )}
        {step === 5 && (
          <WizardStepFinish installedNames={sessionInstalled} onClose={handleClose} />
        )}
      </div>
    </div>
  );
}

// ─── RecommendedTab ───────────────────────────────────────────────────────────

function RecommendedTab({ registryResults, skills, installingByName, onInstallPack, onInstallSkill, onOpenWizard }) {
  const installedNames = skills.map(s => s.name);
  const topNewSkills   = registryResults.filter(s => !s.installed && !s.bundled).slice(0, 4);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 24 }}>
      {/* Guided setup CTA */}
      <div
        style={{
          padding: '20px 24px',
          background: 'rgba(124,58,237,0.08)',
          border: '1px solid rgba(124,58,237,0.3)',
          borderRadius: 10,
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          gap: 12,
          flexWrap: 'wrap',
        }}
      >
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>Not sure where to start?</div>
          <div style={{ fontSize: 13, color: 'var(--text-dim)', marginTop: 3 }}>
            {"Answer a few questions and we'll pick the right helpers for you."}
          </div>
        </div>
        <button
          data-cy="open-wizard-from-rec"
          onClick={onOpenWizard}
          style={{ padding: '8px 18px', borderRadius: 8, background: 'var(--accent)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}
        >
          Set up skills for me
        </button>
      </div>

      {/* Starter packs */}
      <div>
        <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 12 }}>Starter packs</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(260px, 1fr))', gap: 12 }}>
          {STARTER_PACKS.map(pack => (
            <StarterPackCard
              key={pack.id}
              pack={pack}
              installedCount={pack.installSkillNames.filter(n => installedNames.includes(n)).length}
              installing={pack.installSkillNames.some(n => !!installingByName[n])}
              onInstall={() => onInstallPack(pack)}
            />
          ))}
        </div>
      </div>

      {/* Popular helpers from registry */}
      {topNewSkills.length > 0 && (
        <div>
          <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 12 }}>Popular helpers</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {topNewSkills.map(skill => (
              <RecommendedSkillCard
                key={skill.name}
                skill={skill}
                installing={!!installingByName[skill.name]}
                installed={installedNames.includes(skill.name)}
                onInstall={() => onInstallSkill(skill.name)}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ─── InstalledTab ─────────────────────────────────────────────────────────────

function InstalledTab({ skills, view, togglePending, onToggleEnabled, onOpenDetail, onOpenWizard, onSwitchToBrowse }) {
  if (skills.length === 0) {
    return (
      <div
        data-cy="skills-empty"
        style={{ padding: '48px 24px', textAlign: 'center', border: '1px dashed var(--border)', borderRadius: 10 }}
      >
        <div style={{ fontSize: 36, marginBottom: 12 }}>🧰</div>
        <div style={{ fontSize: 17, fontWeight: 700, marginBottom: 6 }}>No skills installed yet</div>
        <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 24, maxWidth: 360, margin: '0 auto 24px' }}>
          Skills are small helpers you add to make OpenFang do more.<br />Start the easy way, or browse what&apos;s available.
        </div>
        <div style={{ display: 'flex', gap: 10, justifyContent: 'center', flexWrap: 'wrap' }}>
          <button
            data-cy="empty-open-wizard"
            onClick={onOpenWizard}
            style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 14 }}
          >
            Start guided setup
          </button>
          <button
            onClick={onSwitchToBrowse}
            style={{ padding: '9px 20px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontSize: 14 }}
          >
            Browse all skills
          </button>
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      {skills.map(skill =>
        view === 'simple' ? (
          <InstalledSkillCardSimple
            key={skill.name}
            skill={skill}
            pending={!!togglePending[skill.name]}
            onToggleEnabled={(enabled) => onToggleEnabled(skill.name, enabled)}
            onOpenDetail={() => onOpenDetail(skill.name)}
          />
        ) : (
          <InstalledSkillCardDetailed
            key={skill.name}
            skill={skill}
            pending={!!togglePending[skill.name]}
            onToggleEnabled={(enabled) => onToggleEnabled(skill.name, enabled)}
            onOpenDetail={() => onOpenDetail(skill.name)}
          />
        )
      )}
    </div>
  );
}

// ─── BrowseTab ────────────────────────────────────────────────────────────────

function BrowseTab({ results, loading, error, query, onChangeQuery, onSearch, onBrowse, installingByName, onInstallSkill }) {
  useEffect(() => { onBrowse(); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <form onSubmit={e => { e.preventDefault(); onSearch(); }} style={{ display: 'flex', gap: 8 }}>
        <input
          type="text"
          value={query}
          onChange={e => onChangeQuery(e.target.value)}
          placeholder="Search for a skill…"
          style={{ flex: 1, padding: '8px 12px', borderRadius: 6, background: 'var(--input-bg)', border: '1px solid var(--border)', color: 'var(--text-primary)', fontSize: 13 }}
        />
        <button
          type="submit"
          disabled={loading || !query.trim()}
          style={{ padding: '8px 16px', borderRadius: 6, background: 'var(--accent)', color: '#fff', border: 'none', cursor: loading ? 'wait' : 'pointer', fontWeight: 600, fontSize: 13 }}
        >
          {loading ? 'Searching…' : 'Search'}
        </button>
        <button
          type="button"
          onClick={onBrowse}
          disabled={loading}
          style={{ padding: '8px 16px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', cursor: loading ? 'wait' : 'pointer', fontSize: 13 }}
        >
          Browse all
        </button>
      </form>

      {error && <div className="error-state" style={{ fontSize: 12 }}>⚠ {error}</div>}

      {loading && (
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', color: 'var(--text-dim)', fontSize: 13 }}>
          <div className="spinner" style={{ width: 14, height: 14 }} /> Loading…
        </div>
      )}

      {!loading && results.length === 0 && (
        <div style={{ padding: 24, textAlign: 'center', color: 'var(--text-dim)', fontSize: 13,border: '1px dashed var(--border)', borderRadius: 8 }}>
          No results yet. Try searching or click Browse all.
        </div>
      )}

      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(260px, 1fr))', gap: 10 }}>
        {results.map(card => (
          <BrowseSkillCard
            key={card.name}
            card={card}
            installing={!!installingByName[card.name]}
            onInstall={onInstallSkill}
          />
        ))}
      </div>
    </div>
  );
}

// ─── SkillsPageV2 — main export ───────────────────────────────────────────────

export default function SkillsPageV2({ initialSkills }) {
  const hasInstalled = (initialSkills ?? []).length > 0;

  const [activeTab, setActiveTab]         = useState(hasInstalled ? 'installed' : 'recommended');
  const [activeView, setActiveView]       = useState('simple');
  const [skills, setSkills]               = useState((initialSkills ?? []).map(normalizeSkill));
  const [refreshing, setRefreshing]       = useState(false);
  const [error, setError]                 = useState('');
  const [drawerSkill, setDrawerSkill]     = useState(null);
  const [drawerOpen, setDrawerOpen]       = useState(false);
  const [wizardOpen, setWizardOpen]       = useState(false);
  const [togglePending, setTogglePending] = useState({});
  const [installingByName, setInstallingByName] = useState({});
  const [browseResults, setBrowseResults] = useState([]);
  const [browseLoading, setBrowseLoading] = useState(false);
  const [browseError, setBrowseError]     = useState('');
  const [browseQuery, setBrowseQuery]     = useState('');

  const refresh = useCallback(async () => {
    setRefreshing(true);
    try {
      const data = await apiClient.get('/api/skills');
      const raw = Array.isArray(data) ? data : data?.skills ?? [];
      setSkills(raw.map(normalizeSkill));
    } catch (e) {
      setError(e.message || 'Could not refresh skills.');
    }
    setRefreshing(false);
  }, []);

  const handleToggle = useCallback(async (skillName, newEnabled) => {
    if (togglePending[skillName]) return;
    const previous = skills.find(s => s.name === skillName)?.enabled ?? true;
    setSkills(prev => prev.map(s => s.name === skillName ? { ...s, enabled: newEnabled } : s));
    setTogglePending(prev => ({ ...prev, [skillName]: true }));
    try {
      await apiClient.put(`/api/skills/${encodeURIComponent(skillName)}/enabled`, { enabled: newEnabled });
      track('skill_toggle_succeeded', { skill: skillName, enabled: newEnabled });
    } catch (e) {
      setSkills(prev => prev.map(s => s.name === skillName ? { ...s, enabled: previous } : s));
      setError(e.message || 'Could not update skill.');
    }
    setTogglePending(prev => ({ ...prev, [skillName]: false }));
  }, [skills, togglePending]);

  const installSkill = useCallback(async (name) => {
    if (installingByName[name]) return;
    setInstallingByName(prev => ({ ...prev, [name]: true }));
    try {
      await apiClient.post('/api/skills/install', { name });
      track('skill_install_succeeded', { skill: name });
      await refresh();
    } catch (e) {
      setError(e.message || `Could not install ${name}.`);
    }
    setInstallingByName(prev => ({ ...prev, [name]: false }));
  }, [installingByName, refresh]);

  const installPack = useCallback(async (pack) => {
    for (const name of pack.installSkillNames) {
      await installSkill(name);
    }
  }, [installSkill]);

  const loadBrowse = useCallback(async () => {
    setBrowseLoading(true);
    setBrowseError('');
    try {
      const data = await apiClient.get('/api/clawhub/browse');
      setBrowseResults((Array.isArray(data) ? data : []).map(normalizeRegistryCard));
    } catch (e) {
      setBrowseError(e.message || 'Could not load registry.');
    }
    setBrowseLoading(false);
  }, []);

  const handleSearch = useCallback(async () => {
    const q = browseQuery.trim();
    if (!q) { return loadBrowse(); }
    setBrowseLoading(true);
    setBrowseError('');
    try {
      const data = await apiClient.get(`/api/clawhub/search?q=${encodeURIComponent(q)}`);
      setBrowseResults((Array.isArray(data) ? data : []).map(normalizeRegistryCard));
    } catch (e) {
      setBrowseError(e.message || 'Search failed.');
    }
    setBrowseLoading(false);
  }, [browseQuery, loadBrowse]);

  const TABS = [
    { key: 'recommended', label: 'Recommended' },
    { key: 'installed',   label: `Installed (${skills.length})` },
    { key: 'browse',      label: 'Browse all' },
  ];

  const openWizard = () => {
    loadBrowse();
    setWizardOpen(true);
  };

  const switchToBrowse = () => {
    setActiveTab('browse');
    loadBrowse();
  };

  return (
    <div data-cy="skills-page">
      {/* Detail drawer */}
      {drawerOpen && drawerSkill && (
        <SkillDrawer
          skillName={drawerSkill}
          onClose={() => setDrawerOpen(false)}
          onToggle={(name, enabled) => handleToggle(name, !enabled)}
          togglePending={!!togglePending[drawerSkill]}
        />
      )}

      {/* Guided setup wizard */}
      <GuidedSetupWizard
        open={wizardOpen}
        installedSkillNames={skills.map(s => s.name)}
        registryResults={browseResults}
        onClose={() => { setWizardOpen(false); setActiveTab('installed'); refresh(); }}
        onInstallSkill={installSkill}
        onInstallPack={installPack}
      />

      {/* Header */}
      <div className="page-header">
        <div>
          <h1 style={{ margin: 0 }}>Skills</h1>
          <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '4px 0 0' }}>
            Skills are small helpers you add to make OpenFang do more.
          </p>
        </div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          <button
            data-cy="open-wizard-btn"
            onClick={openWizard}
            style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}
          >
            Set up skills for me
          </button>
          <button
            data-cy="open-install-modal"
            className="btn btn-ghost btn-sm"
            onClick={switchToBrowse}
          >
            Browse all skills
          </button>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={refreshing}>
            {refreshing ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && (
        <div data-cy="skills-error" className="error-state" style={{ margin: '0 0 16px' }}>
          ⚠ {error}
          <button className="btn btn-ghost btn-sm" onClick={() => setError('')} style={{ marginLeft: 8 }}>Dismiss</button>
        </div>
      )}

      {/* Tab bar + view toggle */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid var(--border)', marginBottom: 20 }}>
        <div style={{ display: 'flex' }}>
          {TABS.map(tab => (
            <button
              key={tab.key}
              data-cy={`tab-${tab.key}`}
              onClick={() => {
                setActiveTab(tab.key);
                if (tab.key === 'browse') loadBrowse();
              }}
              style={{
                padding: '10px 16px',
                background: 'transparent',
                border: 'none',
                borderBottom: `2px solid ${activeTab === tab.key ? 'var(--accent)' : 'transparent'}`,
                color: activeTab === tab.key ? 'var(--text-primary)' : 'var(--text-dim)',
                cursor: 'pointer',
                fontSize: 14,
                fontWeight: activeTab === tab.key ? 700 : 400,
              }}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {activeTab === 'installed' && skills.length > 0 && (
          <div style={{ display: 'flex', gap: 4 }}>
            {['simple', 'detailed'].map(v => (
              <button
                key={v}
                onClick={() => setActiveView(v)}
                style={{
                  padding: '4px 10px',
                  borderRadius: 6,
                  background: activeView === v ? 'var(--accent)' : 'transparent',
                  border: `1px solid ${activeView === v ? 'var(--accent)' : 'var(--border)'}`,
                  color: activeView === v ? '#fff' : 'var(--text-dim)',
                  cursor: 'pointer',
                  fontSize: 12,
                  textTransform: 'capitalize',
                }}
              >
                {v}
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Tab content */}
      <div className="page-body">
        {activeTab === 'recommended' && (
          <RecommendedTab
            registryResults={browseResults}
            skills={skills}
            installingByName={installingByName}
            onInstallPack={installPack}
            onInstallSkill={installSkill}
            onOpenWizard={openWizard}
          />
        )}

        {activeTab === 'installed' && (
          <InstalledTab
            skills={skills}
            view={activeView}
            togglePending={togglePending}
            onToggleEnabled={handleToggle}
            onOpenDetail={(name) => { setDrawerSkill(name); setDrawerOpen(true); }}
            onOpenWizard={openWizard}
            onSwitchToBrowse={switchToBrowse}
          />
        )}

        {activeTab === 'browse' && (
          <BrowseTab
            results={browseResults}
            loading={browseLoading}
            error={browseError}
            query={browseQuery}
            onChangeQuery={setBrowseQuery}
            onSearch={handleSearch}
            onBrowse={loadBrowse}
            installingByName={installingByName}
            onInstallSkill={installSkill}
          />
        )}
      </div>
    </div>
  );
}
