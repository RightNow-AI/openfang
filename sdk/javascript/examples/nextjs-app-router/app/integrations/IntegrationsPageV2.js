'use client';

import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';
import { track } from '../../lib/telemetry';
import IntegrationDetailDrawer from './IntegrationDetailDrawer';

// ──────────────────────────────────────────────────────────────────────────────
// Static config
// ──────────────────────────────────────────────────────────────────────────────

const INTEGRATION_STARTERS = [
  {
    id: 'agency_integrations',
    title: 'Agency essentials',
    subtitle: 'Email, Files and Calendar — the core agency stack.',
    icon: '🏢',
    color: '#7C3AED',
    includes: ['Email', 'Files', 'Calendar'],
    connectTargets: ['email', 'files', 'calendar'],
    mode: 'agency',
  },
  {
    id: 'growth_integrations',
    title: 'Growth toolkit',
    subtitle: 'Email, Web and Social — reach your audience everywhere.',
    icon: '🚀',
    color: '#059669',
    includes: ['Email', 'Web', 'Social'],
    connectTargets: ['email', 'web', 'social'],
    mode: 'growth',
  },
  {
    id: 'school_integrations',
    title: 'School & Course tools',
    subtitle: 'Files, Calendar and Email — run an organised school.',
    icon: '🎓',
    color: '#D97706',
    includes: ['Files', 'Calendar', 'Email'],
    connectTargets: ['files', 'calendar', 'email'],
    mode: 'school',
  },
];

const WIZARD_GOALS = [
  { id: 'followup', label: 'Client / lead follow-up', icon: '📧' },
  { id: 'research', label: 'Web research', icon: '🔍' },
  { id: 'calendar-work', label: 'Scheduling & meetings', icon: '📅' },
  { id: 'content', label: 'Content & social', icon: '📣' },
  { id: 'student-updates', label: 'Student communications', icon: '🎓' },
  { id: 'not-sure', label: "I'm not sure", icon: '🤔' },
];

const WIZARD_MODES = [
  { id: 'agency', label: 'Agency', icon: '🏢', desc: 'Client services & delivery' },
  { id: 'growth', label: 'Growth', icon: '📈', desc: 'Marketing & acquisition' },
  { id: 'school', label: 'School / Course', icon: '🏫', desc: 'Education & coaching' },
  { id: 'general', label: 'General', icon: '⚡', desc: 'Works for anything' },
];

const TOOL_OPTIONS = [
  { id: 'email', label: 'Email', icon: '📧' },
  { id: 'files', label: 'Files', icon: '📁' },
  { id: 'calendar', label: 'Calendar', icon: '📅' },
  { id: 'social', label: 'Social Media', icon: '📣' },
  { id: 'crm', label: 'CRM', icon: '🤝' },
  { id: 'web', label: 'Web / Browser', icon: '🌐' },
];

const GOAL_MODE_MAP = {
  followup: 'agency',
  research: 'growth',
  'calendar-work': 'agency',
  content: 'growth',
  'student-updates': 'school',
  'not-sure': 'general',
};

const GOAL_TOOLS_MAP = {
  followup: ['email', 'crm'],
  research: ['web'],
  'calendar-work': ['calendar', 'email'],
  content: ['social', 'web'],
  'student-updates': ['email', 'files', 'calendar'],
  'not-sure': [],
};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

function normalizeIntegration(raw) {
  return {
    id: raw.id || `int-${Math.random().toString(36).slice(2)}`,
    name: raw.name || 'Unknown',
    description: raw.description || '',
    status: raw.status || 'not_connected',
    category: raw.category || 'general',
    best_for: raw.best_for || '',
    permissions_summary: raw.permissions_summary || '',
  };
}

function statusColor(s) {
  if (s === 'connected') return '#10B981';
  if (s === 'needs_attention') return '#FBBF24';
  return '#64748B';
}

function statusLabel(s) {
  if (s === 'connected') return 'Connected';
  if (s === 'needs_attention') return 'Needs attention';
  return 'Not connected';
}

function categoryColor(c) {
  const map = {
    email: '#7C3AED',
    files: '#059669',
    calendar: '#D97706',
    social: '#EC4899',
    crm: '#0EA5E9',
    web: '#64748B',
    general: '#334155',
  };
  return map[c] || '#334155';
}

// ──────────────────────────────────────────────────────────────────────────────
// Wizard primitives
// ──────────────────────────────────────────────────────────────────────────────

function WizardStep({ step, total, title, children }) {
  return (
    <div>
      <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 6 }}>
        Step {step} of {total}
      </div>
      <div style={{ fontSize: 18, fontWeight: 700, color: 'var(--text)', marginBottom: 20 }}>
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
        border: `2px solid ${selected ? 'var(--accent)' : 'var(--border)'}`,
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
      {selected && <span style={{ marginLeft: 'auto', color: 'var(--accent)', fontSize: 18 }}>✓</span>}
    </button>
  );
}

function ToggleBtn({ selected, onClick, icon, label }) {
  return (
    <button
      onClick={onClick}
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 6,
        padding: '12px 16px',
        borderRadius: 10,
        border: `2px solid ${selected ? 'var(--accent)' : 'var(--border)'}`,
        background: selected ? 'var(--accent-subtle)' : 'var(--bg-elevated)',
        color: selected ? 'var(--accent)' : 'var(--text-dim)',
        cursor: 'pointer',
        fontSize: 12,
        fontWeight: selected ? 700 : 400,
        transition: 'all 0.15s',
        minWidth: 80,
      }}
    >
      <span style={{ fontSize: 22 }}>{icon}</span>
      {label}
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
    <WizardStep step={2} total={5} title="How do you work?">
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

function StepPickTools({ selected, onChange }) {
  function toggle(id) {
    if (selected.includes(id)) {
      onChange(selected.filter((t) => t !== id));
    } else {
      onChange([...selected, id]);
    }
  }
  return (
    <WizardStep step={3} total={5} title="Which tools do you want to connect?">
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 10 }}>
        {TOOL_OPTIONS.map((t) => (
          <ToggleBtn
            key={t.id}
            selected={selected.includes(t.id)}
            onClick={() => toggle(t.id)}
            icon={t.icon}
            label={t.label}
          />
        ))}
      </div>
      <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 14 }}>
        Select all that apply. You can add more later.
      </div>
    </WizardStep>
  );
}

function StepRecommendations({ goal, mode, tools, allIntegrations, onConnect, connectStatus }) {
  const suggestedIds = tools.length > 0 ? tools : (GOAL_TOOLS_MAP[goal] || []);
  const relevant = allIntegrations.filter((i) => suggestedIds.includes(i.category));
  const fallback = TOOL_OPTIONS.filter((t) => suggestedIds.includes(t.id));

  return (
    <WizardStep step={4} total={5} title="Here's what we recommend connecting">
      {relevant.length === 0 && fallback.length === 0 && (
        <div style={{ color: 'var(--text-dim)', fontSize: 14 }}>
          No specific integrations found. Connect them in the Available tab after setup.
        </div>
      )}
      {fallback.map((tool) => {
        const live = allIntegrations.find((i) => i.category === tool.id);
        const intId = live?.id || tool.id;
        const cs = connectStatus[intId];
        const isConnected = live?.status === 'connected' || cs === 'done';
        return (
          <div
            key={tool.id}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 12,
              padding: '12px 16px',
              background: 'var(--bg-elevated)',
              border: '1px solid var(--border)',
              borderRadius: 10,
              marginBottom: 8,
            }}
          >
            <span style={{ fontSize: 22 }}>{tool.icon}</span>
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--text)' }}>{tool.label}</div>
              {live && (
                <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{live.description}</div>
              )}
            </div>
            {isConnected ? (
              <span style={{ color: 'var(--success)', fontSize: 13, fontWeight: 600 }}>✓ Connected</span>
            ) : (
              <button
                onClick={() => onConnect(intId)}
                disabled={cs === 'loading'}
                style={{
                  padding: '6px 14px',
                  borderRadius: 6,
                  border: 'none',
                  background: 'var(--accent)',
                  color: 'var(--text-inverse)',
                  fontWeight: 700,
                  cursor: cs === 'loading' ? 'not-allowed' : 'pointer',
                  fontSize: 12,
                  opacity: cs === 'loading' ? 0.7 : 1,
                }}
              >
                {cs === 'loading' ? '…' : 'Connect'}
              </button>
            )}
          </div>
        );
      })}
    </WizardStep>
  );
}

function StepFinish({ onClose }) {
  return (
    <WizardStep step={5} total={5} title="You're all connected">
      <div style={{ textAlign: 'center', padding: '20px 0 4px' }}>
        <div style={{ fontSize: 52, marginBottom: 12 }}>🔗</div>
        <div style={{ fontSize: 15, color: 'var(--text-soft)', marginBottom: 20 }}>
          Your integrations are being activated. Head to the Connected tab to see them.
        </div>
        <button
          onClick={onClose}
          style={{
            padding: '10px 28px',
            borderRadius: 8,
            border: 'none',
            background: 'var(--accent)',
            color: 'var(--text-inverse)',
            fontWeight: 700,
            cursor: 'pointer',
            fontSize: 15,
          }}
        >
          View connected →
        </button>
      </div>
    </WizardStep>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Wizard modal
// ──────────────────────────────────────────────────────────────────────────────

function IntegrationsWizard({ allIntegrations, onClose, onConnected }) {
  const [step, setStep] = useState(1);
  const [goal, setGoal] = useState('');
  const [mode, setMode] = useState('');
  const [selectedTools, setSelectedTools] = useState([]);
  const [connectStatus, setConnectStatus] = useState({});

  function handleGoalChange(v) {
    setGoal(v);
    setSelectedTools(GOAL_TOOLS_MAP[v] || []);
    if (GOAL_MODE_MAP[v] && GOAL_MODE_MAP[v] !== 'general') {
      setMode(GOAL_MODE_MAP[v]);
    }
  }

  async function handleConnect(id) {
    setConnectStatus((prev) => ({ ...prev, [id]: 'loading' }));
    try {
      await apiClient.post(`/api/integrations/${id}/connect`, {});
      setConnectStatus((prev) => ({ ...prev, [id]: 'done' }));
      track('integration_connected', { id });
      if (onConnected) onConnected();
    } catch {
      setConnectStatus((prev) => ({ ...prev, [id]: 'error' }));
    }
  }

  const canNext =
    (step === 1 && !!goal) ||
    (step === 2 && !!mode) ||
    step === 3 ||
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
        {step === 2 && <StepMode value={mode} onChange={setMode} />}
        {step === 3 && <StepPickTools selected={selectedTools} onChange={setSelectedTools} />}
        {step === 4 && (
          <StepRecommendations
            goal={goal}
            mode={mode}
            tools={selectedTools}
            allIntegrations={allIntegrations}
            onConnect={handleConnect}
            connectStatus={connectStatus}
          />
        )}
        {step === 5 && <StepFinish onClose={() => { setStep(1); onClose(); }} />}

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
                onClick={() => setStep((s) => s - 1)}
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
              onClick={() => setStep((s) => s + 1)}
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

function IntegrationBundleCard({ bundle, onConnect, connectStatus }) {
  const allConnected = bundle.connectTargets.every((t) => connectStatus[t] === 'done');
  const anyLoading = bundle.connectTargets.some((t) => connectStatus[t] === 'loading');

  async function handleConnectBundle() {
    for (const id of bundle.connectTargets) {
      onConnect(id);
    }
  }

  return (
    <div
      style={{
        background: 'var(--bg-card)',
        border: `2px solid ${bundle.color}33`,
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
            background: `${bundle.color}22`,
            borderRadius: 10,
            padding: '6px 10px',
          }}
        >
          {bundle.icon}
        </span>
        <div>
          <div style={{ fontSize: 15, fontWeight: 700, color: 'var(--text)' }}>{bundle.title}</div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{bundle.subtitle}</div>
        </div>
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
        {bundle.includes.map((t) => (
          <span
            key={t}
            style={{
              fontSize: 11,
              padding: '2px 9px',
              borderRadius: 100,
              background: `${bundle.color}18`,
              color: bundle.color,
              border: `1px solid ${bundle.color}40`,
            }}
          >
            {t}
          </span>
        ))}
      </div>
      {allConnected ? (
        <div style={{ color: 'var(--success)', fontSize: 13, fontWeight: 600 }}>✓ All connected</div>
      ) : (
        <button
          onClick={handleConnectBundle}
          disabled={anyLoading}
          style={{
            marginTop: 4,
            padding: '8px 16px',
            borderRadius: 8,
            border: 'none',
            background: bundle.color,
            color: 'var(--text-inverse)',
            fontWeight: 700,
            cursor: anyLoading ? 'not-allowed' : 'pointer',
            fontSize: 13,
            opacity: anyLoading ? 0.7 : 1,
            alignSelf: 'flex-start',
          }}
        >
          {anyLoading ? 'Connecting…' : 'Connect this bundle →'}
        </button>
      )}
    </div>
  );
}

function IntegrationCard({ integration, onConnect, connectStatus }) {
  const cs = connectStatus[integration.id];
  const color = categoryColor(integration.category);
  const isConnected = integration.status === 'connected' || cs === 'done';

  return (
    <div
      style={{
        background: 'var(--bg-elevated)',
        border: `1px solid ${isConnected ? 'rgba(31,143,85,0.2)' : 'var(--border)'}`,
        borderRadius: 10,
        padding: '14px 16px',
        display: 'flex',
        alignItems: 'center',
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
        {TOOL_OPTIONS.find((t) => t.id === integration.category)?.icon || '🔌'}
      </span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)' }}>{integration.name}</div>
        <div
          style={{
            fontSize: 12,
            color: 'var(--text-dim)',
            marginTop: 2,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        >
          {integration.description}
        </div>
        <div
          style={{
            fontSize: 11,
            marginTop: 4,
            color: statusColor(isConnected ? 'connected' : integration.status),
            fontWeight: 600,
          }}
        >
          ● {statusLabel(isConnected ? 'connected' : integration.status)}
        </div>
      </div>
      {!isConnected && (
        <button
          onClick={() => onConnect(integration.id)}
          disabled={cs === 'loading'}
          style={{
            padding: '6px 14px',
            borderRadius: 6,
            border: 'none',
            background: 'var(--accent)',
            color: 'var(--text-inverse)',
            fontWeight: 700,
            cursor: cs === 'loading' ? 'not-allowed' : 'pointer',
            fontSize: 12,
            opacity: cs === 'loading' ? 0.7 : 1,
            flexShrink: 0,
          }}
        >
          {cs === 'loading' ? '…' : 'Connect'}
        </button>
      )}
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Tab components
// ──────────────────────────────────────────────────────────────────────────────

function RecommendedIntegrationsTab({ allIntegrations, onOpenWizard }) {
  const [connectStatus, setConnectStatus] = useState({});

  async function handleConnect(id) {
    setConnectStatus((prev) => ({ ...prev, [id]: 'loading' }));
    try {
      await apiClient.post(`/api/integrations/${id}/connect`, {});
      setConnectStatus((prev) => ({ ...prev, [id]: 'done' }));
      track('integration_bundle_tool_connected', { id });
    } catch {
      setConnectStatus((prev) => ({ ...prev, [id]: 'error' }));
    }
  }

  return (
    <div>
      <div
        style={{
          background: 'linear-gradient(135deg, #059669 0%, #0D9488 100%)',
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
          <div style={{ fontSize: 22, fontWeight: 800, color: 'var(--text-inverse)', marginBottom: 6 }}>
            Connect your tools
          </div>
          <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.8)' }}>
            {"Tell us your workflow and we'll suggest the right integrations."}
          </div>
        </div>
        <button
          onClick={onOpenWizard}
          style={{
            padding: '12px 24px',
            borderRadius: 10,
            border: 'none',
            background: '#fff',
            color: '#059669',
            fontWeight: 800,
            cursor: 'pointer',
            fontSize: 15,
            whiteSpace: 'nowrap',
          }}
        >
          Set up for me →
        </button>
      </div>

      <div style={{ fontSize: 15, fontWeight: 700, color: 'var(--text-soft)', marginBottom: 14 }}>
        Starter bundles
      </div>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))',
          gap: 14,
        }}
      >
        {INTEGRATION_STARTERS.map((bundle) => (
          <IntegrationBundleCard
            key={bundle.id}
            bundle={bundle}
            onConnect={handleConnect}
            connectStatus={connectStatus}
          />
        ))}
      </div>
    </div>
  );
}

function ConnectedIntegrationsTab({ integrations, loading }) {
  const connected = integrations.filter((i) => i.status === 'connected');

  if (loading) {
    return (
      <div style={{ color: 'var(--text-dim)', fontSize: 14, padding: '40px 0', textAlign: 'center' }}>
        Loading…
      </div>
    );
  }

  if (connected.length === 0) {
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
        <div style={{ fontSize: 40, marginBottom: 12 }}>🔌</div>
        <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text-soft)', marginBottom: 8 }}>
          Nothing connected yet
        </div>
        <div style={{ fontSize: 14, color: 'var(--text-dim)' }}>
          Use the Recommended tab to connect your first tools.
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      {connected.map((i) => (
        <IntegrationCard
          key={i.id}
          integration={i}
          onConnect={() => {}}
          connectStatus={{}}
        />
      ))}
    </div>
  );
}

function AvailableIntegrationsTab({ integrations, loading, onRefresh }) {
  const [connectStatus, setConnectStatus] = useState({});

  async function handleConnect(id) {
    setConnectStatus((prev) => ({ ...prev, [id]: 'loading' }));
    try {
      await apiClient.post(`/api/integrations/${id}/connect`, {});
      setConnectStatus((prev) => ({ ...prev, [id]: 'done' }));
      track('integration_connected', { id });
      onRefresh();
    } catch {
      setConnectStatus((prev) => ({ ...prev, [id]: 'error' }));
    }
  }

  const available = integrations.filter((i) => i.status !== 'connected');

  if (loading) {
    return (
      <div style={{ color: 'var(--text-dim)', fontSize: 14, padding: '40px 0', textAlign: 'center' }}>
        Loading…
      </div>
    );
  }

  if (available.length === 0) {
    const allConnected = integrations.every((i) => i.status === 'connected');
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
        <div style={{ fontSize: 40, marginBottom: 12 }}>{allConnected ? '🎉' : '🔌'}</div>
        <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text-soft)', marginBottom: 8 }}>
          {allConnected ? 'All integrations connected!' : 'No integrations available'}
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      {available.map((i) => (
        <IntegrationCard
          key={i.id}
          integration={i}
          onConnect={handleConnect}
          connectStatus={connectStatus}
        />
      ))}
    </div>
  );
}

function AdvancedIntegrationsTab({ integrations, loading, error, onRefresh }) {
  const [connectStatus, setConnectStatus] = useState({});

  async function handleConnect(id) {
    setConnectStatus((prev) => ({ ...prev, [id]: 'loading' }));
    try {
      await apiClient.post(`/api/integrations/${id}/connect`, {});
      setConnectStatus((prev) => ({ ...prev, [id]: 'done' }));
      track('integration_connected_advanced', { id });
      onRefresh();
    } catch {
      setConnectStatus((prev) => ({ ...prev, [id]: 'error' }));
    }
  }

  if (error) {
    return (
      <div style={{ color: 'var(--danger)', padding: '16px 0' }}>
        {error}
      </div>
    );
  }

  return (
    <div>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginBottom: 16,
        }}
      >
        <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>
          {integrations.length} integration{integrations.length !== 1 ? 's' : ''}
        </div>
        <button
          onClick={onRefresh}
          style={{
            background: 'none',
            border: '1px solid var(--border)',
            borderRadius: 6,
            color: 'var(--text-dim)',
            cursor: 'pointer',
            padding: '4px 12px',
            fontSize: 12,
          }}
        >
          Refresh
        </button>
      </div>

      {loading ? (
        <div style={{ color: 'var(--text-dim)', fontSize: 14, textAlign: 'center', padding: '40px 0' }}>
          Loading…
        </div>
      ) : integrations.length === 0 ? (
        <div style={{ color: 'var(--text-dim)', fontSize: 14 }}>No integrations configured.</div>
      ) : (
        <div
          style={{
            overflowX: 'auto',
            borderRadius: 10,
            border: '1px solid var(--border)',
          }}
        >
          <table
            style={{
              width: '100%',
              borderCollapse: 'collapse',
              fontSize: 13,
              color: 'var(--text-soft)',
            }}
          >
            <thead>
              <tr style={{ background: 'var(--bg-elevated)', borderBottom: '1px solid var(--border)' }}>
                <th style={{ padding: '10px 14px', textAlign: 'left', fontWeight: 600 }}>Name</th>
                <th style={{ padding: '10px 14px', textAlign: 'left', fontWeight: 600 }}>Category</th>
                <th style={{ padding: '10px 14px', textAlign: 'left', fontWeight: 600 }}>Status</th>
                <th style={{ padding: '10px 14px', textAlign: 'left', fontWeight: 600 }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {integrations.map((i) => {
                const cs = connectStatus[i.id];
                const isConnected = i.status === 'connected' || cs === 'done';
                return (
                  <tr
                    key={i.id}
                    style={{ borderBottom: '1px solid var(--border)' }}
                  >
                    <td style={{ padding: '10px 14px' }}>
                      <div style={{ fontWeight: 600, color: 'var(--text)' }}>{i.name}</div>
                      <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 2 }}>{i.description}</div>
                    </td>
                    <td style={{ padding: '10px 14px' }}>
                      <span
                        style={{
                          fontSize: 11,
                          padding: '2px 8px',
                          borderRadius: 100,
                          background: `${categoryColor(i.category)}22`,
                          color: categoryColor(i.category),
                        }}
                      >
                        {i.category}
                      </span>
                    </td>
                    <td
                      style={{
                        padding: '10px 14px',
                        color: statusColor(isConnected ? 'connected' : i.status),
                        fontWeight: 600,
                      }}
                    >
                      {statusLabel(isConnected ? 'connected' : i.status)}
                    </td>
                    <td style={{ padding: '10px 14px' }}>
                      {!isConnected && (
                        <button
                          onClick={() => handleConnect(i.id)}
                          disabled={cs === 'loading'}
                          style={{
                            padding: '4px 12px',
                            borderRadius: 5,
                            border: 'none',
                            background: 'var(--accent)',
                            color: 'var(--text-inverse)',
                            fontWeight: 600,
                            cursor: cs === 'loading' ? 'not-allowed' : 'pointer',
                            fontSize: 11,
                            opacity: cs === 'loading' ? 0.7 : 1,
                          }}
                        >
                          {cs === 'loading' ? '…' : 'Connect'}
                        </button>
                      )}
                      {cs === 'error' && (
                        <span style={{ color: 'var(--danger)', fontSize: 11, marginLeft: 6 }}>
                          Failed
                        </span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Main export
// ──────────────────────────────────────────────────────────────────────────────

export default function IntegrationsPageV2({ initialIntegrations = [] }) {
  const [tab, setTab] = useState('recommended');
  const [integrations, setIntegrations] = useState(initialIntegrations.map(normalizeIntegration));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [showWizard, setShowWizard] = useState(false);
  const [drawerIntegrationId, setDrawerIntegrationId] = useState(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/integrations');
      const raw = data?.integrations ?? (Array.isArray(data) ? data : []);
      setIntegrations(raw.map(normalizeIntegration));
    } catch (err) {
      setError(err?.message || 'Failed to load integrations');
    } finally {
      setLoading(false);
    }
  }, []);

  const TABS = [
    { id: 'recommended', label: 'Recommended' },
    { id: 'connected', label: 'Connected' },
    { id: 'available', label: 'Available' },
    { id: 'advanced', label: 'Advanced' },
  ];

  return (
    <div data-cy="integrations-page" style={{ fontFamily: 'system-ui, sans-serif', color: 'var(--text)' }}>
      {/* Header */}
      <div style={{ marginBottom: 28 }}>
        <h1 style={{ fontSize: 28, fontWeight: 800, margin: 0, color: 'var(--text)' }}>
          Integrations
        </h1>
        <p style={{ fontSize: 14, color: 'var(--text-dim)', margin: '6px 0 0' }}>
          Connect external tools and services to your agents.
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
        <RecommendedIntegrationsTab
          allIntegrations={integrations}
          onOpenWizard={() => setShowWizard(true)}
          onOpenDetail={id => setDrawerIntegrationId(id)}
        />
      )}
      {tab === 'connected' && (
        <ConnectedIntegrationsTab integrations={integrations} loading={loading} onOpenDetail={id => setDrawerIntegrationId(id)} />
      )}
      {tab === 'available' && (
        <AvailableIntegrationsTab
          integrations={integrations}
          loading={loading}
          onRefresh={refresh}
        />
      )}
      {tab === 'advanced' && (
        <AdvancedIntegrationsTab
          integrations={integrations}
          loading={loading}
          error={error}
          onRefresh={refresh}
        />
      )}

      {/* Wizard */}
      {showWizard && (
        <IntegrationsWizard
          allIntegrations={integrations}
          onClose={() => setShowWizard(false)}
          onConnected={refresh}
        />
      )}
      <IntegrationDetailDrawer
        key={drawerIntegrationId ?? 'integration-detail-closed'}
        open={!!drawerIntegrationId}
        integrationId={drawerIntegrationId}
        onClose={() => setDrawerIntegrationId(null)}
        onConnect={async (id) => { await apiClient.post(`/api/integrations/${id}/connect`, {}); await refresh(); }}
        onDisconnect={async (id) => { await apiClient.post(`/api/integrations/${id}/disconnect`, {}); await refresh(); }}
        onTest={async (id) => apiClient.post(`/api/integrations/${id}/test`, {})}
      />
    </div>
  );
}
