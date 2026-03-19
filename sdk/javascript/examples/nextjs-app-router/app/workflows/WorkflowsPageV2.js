'use client';
import { useState, useCallback } from 'react';
import { useRouter } from 'next/navigation';
import { apiClient } from '../../lib/api-client';
import { workApi } from '../../lib/work-api';
import { track } from '../../lib/telemetry';

// ─── Starter templates ────────────────────────────────────────────────────────

const WORKFLOW_STARTERS = [
  {
    id: 'client_onboarding',
    title: 'Client onboarding',
    description: 'Set up a checklist and welcome flow when a new client is added.',
    bestFor: 'Agency Mode',
    startsWhen: 'When a new client is added',
    whatHappensNext: ['Create onboarding tasks', 'Assign first steps', 'Wait for approval before sending welcome email'],
    needsApproval: true,
    category: 'agency',
    templatePayload: {
      name: 'Client onboarding',
      trigger_type: 'client_created',
      actions: ['create_tasks', 'assign_work', 'wait_for_approval', 'draft_email'],
      approval_rules: ['before_send'],
    },
  },
  {
    id: 'weekly_planning',
    title: 'Weekly planning',
    description: 'Build a fresh weekly plan every Monday.',
    bestFor: 'Agency Mode or School Mode',
    startsWhen: 'On a schedule',
    whatHappensNext: ['Create task plan', 'Assign work', 'Notify owner'],
    needsApproval: false,
    category: 'general',
    templatePayload: {
      name: 'Weekly planning',
      trigger_type: 'schedule',
      actions: ['create_summary', 'create_tasks', 'assign_work', 'notify_someone'],
      approval_rules: [],
    },
  },
  {
    id: 'approval_before_send',
    title: 'Approval before email send',
    description: 'Pause drafts until someone approves them.',
    bestFor: 'Agency Mode or Growth Mode',
    startsWhen: 'When approval is completed',
    whatHappensNext: ['Wait for approval', 'Send email', 'Create summary'],
    needsApproval: true,
    category: 'general',
    templatePayload: {
      name: 'Approval before email send',
      trigger_type: 'approval_completed',
      actions: ['wait_for_approval', 'send_email', 'create_summary'],
      approval_rules: ['before_send'],
    },
  },
  {
    id: 'content_review_flow',
    title: 'Content approval flow',
    description: 'Prepare content, then stop for review before publishing.',
    bestFor: 'Growth Mode',
    startsWhen: 'When a task is completed',
    whatHappensNext: ['Create summary', 'Wait for approval', 'Notify someone'],
    needsApproval: true,
    category: 'growth',
    templatePayload: {
      name: 'Content approval flow',
      trigger_type: 'task_completed',
      actions: ['create_summary', 'wait_for_approval', 'notify_someone'],
      approval_rules: ['before_publish'],
    },
  },
  {
    id: 'student_welcome_flow',
    title: 'Student welcome flow',
    description: 'Welcome a new student and prepare their first steps.',
    bestFor: 'School Mode',
    startsWhen: 'When a new client is added',
    whatHappensNext: ['Create tasks', 'Draft email', 'Wait for approval'],
    needsApproval: true,
    category: 'school',
    templatePayload: {
      name: 'Student welcome flow',
      trigger_type: 'client_created',
      actions: ['create_tasks', 'draft_email', 'wait_for_approval'],
      approval_rules: ['before_send'],
    },
  },
  {
    id: 'lead_followup_flow',
    title: 'Lead follow-up flow',
    description: 'Prepare follow-up steps when a lead needs attention.',
    bestFor: 'Agency Mode or Growth Mode',
    startsWhen: 'When a message arrives',
    whatHappensNext: ['Create summary', 'Draft email', 'Wait for approval'],
    needsApproval: true,
    category: 'general',
    templatePayload: {
      name: 'Lead follow-up flow',
      trigger_type: 'message_received',
      actions: ['create_summary', 'draft_email', 'wait_for_approval'],
      approval_rules: ['before_send', 'before_tool_use'],
    },
  },
];

// ─── Wizard choice data ───────────────────────────────────────────────────────

const GOALS = [
  { value: 'client-work',          label: 'Client work',           icon: '💼' },
  { value: 'email-followup',       label: 'Email follow-up',       icon: '📧' },
  { value: 'approvals',            label: 'Approvals',             icon: '✅' },
  { value: 'weekly-planning',      label: 'Weekly planning',       icon: '📅' },
  { value: 'content-publishing',   label: 'Content publishing',    icon: '📝' },
  { value: 'student-onboarding',   label: 'Student onboarding',    icon: '🎓' },
  { value: 'something-else',       label: 'Something else',        icon: '✨' },
];

const TRIGGERS = [
  { value: 'manual',              label: 'When I click Run',              icon: '▶' },
  { value: 'schedule',            label: 'On a schedule',                 icon: '⏰' },
  { value: 'approval_completed',  label: 'When something gets approved',  icon: '✅' },
  { value: 'client_created',      label: 'When a new client is added',    icon: '👤' },
  { value: 'task_completed',      label: 'When a task is completed',      icon: '☑' },
  { value: 'message_received',    label: 'When a message arrives',        icon: '💬' },
];

const ACTIONS = [
  { value: 'create_tasks',      label: 'Create tasks',          icon: '📋' },
  { value: 'assign_work',       label: 'Assign work',           icon: '👤' },
  { value: 'draft_email',       label: 'Draft email',           icon: '✉' },
  { value: 'wait_for_approval', label: 'Wait for approval',     icon: '⏸' },
  { value: 'send_email',        label: 'Send email',            icon: '📤' },
  { value: 'create_summary',    label: 'Create summary',        icon: '📄' },
  { value: 'move_stage',        label: 'Move to next stage',    icon: '➡' },
  { value: 'notify_someone',    label: 'Notify someone',        icon: '🔔' },
];

const APPROVAL_OPTIONS = [
  { value: 'before_send',        label: 'Before sending emails',          icon: '📧' },
  { value: 'before_publish',     label: 'Before publishing',              icon: '📢' },
  { value: 'before_assignment',  label: 'Before assigning work',          icon: '👤' },
  { value: 'before_tool_use',    label: 'Before using connected tools',   icon: '🔧' },
  { value: 'none',               label: 'No approval needed',             icon: '⚡' },
];

// ─── Normalizer ───────────────────────────────────────────────────────────────

function normalizeWorkflow(raw, i) {
  return {
    id:               String(raw?.id ?? `wf-${i}`),
    name:             String(raw?.name ?? 'Unnamed workflow'),
    description:      String(raw?.description ?? ''),
    steps:            Number(raw?.steps ?? raw?.step_count ?? 0),
    status:           raw?.status ?? 'ready',
    enabled:          raw?.enabled !== false,
    trigger_label:    raw?.trigger_label ?? raw?.trigger?.label ?? '—',
    last_run_label:   raw?.last_run_label ?? null,
    next_run_label:   raw?.next_run_label ?? null,
    approval_required: !!raw?.approval_required,
    category:         raw?.category ?? 'general',
  };
}

function triggerLabel(triggerValue) {
  return TRIGGERS.find(t => t.value === triggerValue)?.label ?? triggerValue ?? '—';
}
function actionLabel(actionValue) {
  return ACTIONS.find(a => a.value === actionValue)?.label ?? actionValue ?? '—';
}
function approvalLabel(ruleValue) {
  return APPROVAL_OPTIONS.find(a => a.value === ruleValue)?.label ?? ruleValue ?? '—';
}
function goalToTemplateId(goal) {
  const map = {
    'client-work':        'client_onboarding',
    'email-followup':     'approval_before_send',
    'approvals':          'approval_before_send',
    'weekly-planning':    'weekly_planning',
    'content-publishing': 'content_review_flow',
    'student-onboarding': 'student_welcome_flow',
    'something-else':     null,
  };
  return map[goal] ?? null;
}

// ─── Shared wizard primitives ─────────────────────────────────────────────────

function WizardStep({ step, totalSteps = 5, title, subtitle, onBack, onNext, nextLabel, nextDisabled, children }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 24 }}>
      <div style={{ fontSize: 11, color: 'var(--text-dim)', letterSpacing: 1, textTransform: 'uppercase' }}>Step {step} of {totalSteps}</div>
      <div>
        <h2 style={{ fontSize: 20, fontWeight: 700, margin: 0 }}>{title}</h2>
        {subtitle && <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '6px 0 0', lineHeight: 1.6 }}>{subtitle}</p>}
      </div>
      <div>{children}</div>
      <div style={{ display: 'flex', gap: 10, justifyContent: 'flex-end', paddingTop: 8, borderTop: '1px solid var(--border)' }}>
        {onBack && (
          <button onClick={onBack} style={{ padding: '8px 18px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontSize: 13 }}>
            ← Back
          </button>
        )}
        {onNext && (
          <button onClick={onNext} disabled={nextDisabled} style={{ padding: '8px 20px', borderRadius: 6, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: nextDisabled ? 'not-allowed' : 'pointer', fontWeight: 700, fontSize: 13, opacity: nextDisabled ? 0.5 : 1 }}>
            {nextLabel ?? 'Next →'}
          </button>
        )}
      </div>
    </div>
  );
}

function ChoiceButton({ selected, onClick, icon, label, sub }) {
  return (
    <button
      onClick={onClick}
      style={{
        display: 'block', width: '100%', textAlign: 'left',
        padding: '12px 16px', borderRadius: 8, marginBottom: 8,
        border: `1px solid ${selected ? 'var(--accent)' : 'var(--border)'}`,
        background: selected ? 'var(--accent-subtle)' : 'transparent',
        cursor: 'pointer',
      }}
    >
      {icon && <span style={{ marginRight: 10, fontSize: 16 }}>{icon}</span>}
      <span style={{ fontWeight: selected ? 700 : 400, fontSize: 14 }}>{label}</span>
      {sub && <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 3, marginLeft: icon ? 26 : 0 }}>{sub}</div>}
    </button>
  );
}

function ToggleChoiceButton({ selected, onClick, icon, label }) {
  return (
    <button
      onClick={onClick}
      style={{
        display: 'flex', alignItems: 'center', gap: 10,
        padding: '10px 14px', borderRadius: 8, marginBottom: 6,
        border: `1px solid ${selected ? 'var(--accent)' : 'var(--border)'}`,
        background: selected ? 'var(--accent-subtle)' : 'transparent',
        cursor: 'pointer', width: '100%',
      }}
    >
      <span style={{ fontSize: 18, width: 24, textAlign: 'center' }}>{icon}</span>
      <span style={{ fontWeight: selected ? 700 : 400, fontSize: 13 }}>{label}</span>
      {selected && <span style={{ marginLeft: 'auto', color: 'var(--accent)', fontSize: 16 }}>✓</span>}
    </button>
  );
}

// ─── Workflow wizard steps ────────────────────────────────────────────────────

function WfWizardStepGoal({ value, onSelect, onNext }) {
  return (
    <WizardStep step={1} title="What do you want to automate?" subtitle="We'll build the right workflow based on your goal." onNext={onNext} nextDisabled={!value}>
      {GOALS.map(g => (
        <ChoiceButton key={g.value} selected={value === g.value} onClick={() => onSelect(g.value)} icon={g.icon} label={g.label} />
      ))}
    </WizardStep>
  );
}

function WfWizardStepTrigger({ value, onSelect, onBack, onNext }) {
  return (
    <WizardStep step={2} title="What should start it?" subtitle="Choose what kicks this workflow off." onBack={onBack} onNext={onNext} nextDisabled={!value}>
      {TRIGGERS.map(t => (
        <ChoiceButton key={t.value} selected={value === t.value} onClick={() => onSelect(t.value)} icon={t.icon} label={t.label} />
      ))}
    </WizardStep>
  );
}

function WfWizardStepActions({ value, onToggle, onBack, onNext }) {
  return (
    <WizardStep step={3} title="What should happen next?" subtitle="Pick the steps that run in this workflow. You can pick more than one." onBack={onBack} onNext={onNext} nextDisabled={value.length === 0}>
      {ACTIONS.map(a => (
        <ToggleChoiceButton key={a.value} selected={value.includes(a.value)} onClick={() => onToggle(a.value)} icon={a.icon} label={a.label} />
      ))}
    </WizardStep>
  );
}

function WfWizardStepApproval({ value, onToggle, onBack, onNext }) {
  return (
    <WizardStep step={4} title="What should wait for approval?" subtitle="Work will pause at these points until someone approves." onBack={onBack} onNext={onNext}>
      {APPROVAL_OPTIONS.map(a => (
        <ToggleChoiceButton key={a.value} selected={value.includes(a.value)} onClick={() => onToggle(a.value)} icon={a.icon} label={a.label} />
      ))}
    </WizardStep>
  );
}

function WfWizardStepReview({ summary, creating, onBack, onCreate }) {
  return (
    <WizardStep step={5} title="Review and turn it on" subtitle="Here's what your workflow will do. You can change it later." onBack={onBack} onNext={onCreate} nextLabel={creating ? 'Saving…' : 'Save workflow'} nextDisabled={creating}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
        <ReviewRow label="Name" value={summary.name || 'My workflow'} />
        <ReviewRow label="Starts when" value={summary.triggerLabel} />
        {summary.actionLabels.length > 0 && (
          <ReviewRow label="What happens next" value={summary.actionLabels.join(' → ')} />
        )}
        {summary.approvalLabels.length > 0 && (
          <ReviewRow label="Pauses for approval" value={summary.approvalLabels.join(', ')} />
        )}
        <ReviewRow label="Status on day one" value="Off (you can turn it on after)">
          <span style={{ display: 'inline-block', padding: '2px 8px', borderRadius: 999, background: 'rgba(156,163,175,0.15)', color: '#9ca3af', fontSize: 12 }}>Off</span>
        </ReviewRow>
      </div>
    </WizardStep>
  );
}

function ReviewRow({ label, value, children }) {
  return (
    <div style={{ padding: '12px 14px', background: 'var(--surface2)', borderRadius: 8 }}>
      <div style={{ fontSize: 11, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.8, marginBottom: 4 }}>{label}</div>
      {children ?? <div style={{ fontSize: 14, fontWeight: 500 }}>{value}</div>}
    </div>
  );
}

// ─── WorkflowWizard ───────────────────────────────────────────────────────────

function WorkflowWizard({ open, onClose, onCreate }) {
  const [step,          setStep]          = useState(1);
  const [goal,          setGoal]          = useState(null);
  const [trigger,       setTrigger]       = useState(null);
  const [actions,       setActions]       = useState([]);
  const [approvalRules, setApprovalRules] = useState([]);
  const [creating,      setCreating]      = useState(false);

  const toggleAction = (v) => setActions(prev => prev.includes(v) ? prev.filter(a => a !== v) : [...prev, v]);
  const toggleRule   = (v) => {
    if (v === 'none') { setApprovalRules(['none']); return; }
    setApprovalRules(prev => {
      const without = prev.filter(r => r !== 'none');
      return without.includes(v) ? without.filter(r => r !== v) : [...without, v];
    });
  };

  const summary = {
    name:           GOALS.find(g => g.value === goal)?.label ?? 'My workflow',
    triggerLabel:   triggerLabel(trigger),
    actionLabels:   actions.map(actionLabel),
    approvalLabels: approvalRules.filter(r => r !== 'none').map(approvalLabel),
  };

  const handleCreate = async () => {
    setCreating(true);
    try {
      await onCreate({
        name:           summary.name,
        description:    `Automates: ${summary.name}`,
        trigger_type:   trigger,
        actions:        actions,
        approval_rules: approvalRules.filter(r => r !== 'none'),
        enabled:        false,
      });
      handleClose();
    } catch {
      // parent handles error display
    }
    setCreating(false);
  };

  const handleClose = () => {
    setStep(1); setGoal(null); setTrigger(null); setActions([]); setApprovalRules([]);
    onClose();
  };

  if (!open) return null;

  return (
    <div
      data-cy="workflow-wizard"
      style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,0.65)', backdropFilter: 'blur(3px)', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 24 }}
      onClick={e => { if (e.target === e.currentTarget) handleClose(); }}
    >
      <div style={{ width: '100%', maxWidth: 540, background: 'var(--bg-elevated)', border: '1px solid var(--border)', borderRadius: 14, padding: '28px 32px', maxHeight: '90vh', overflowY: 'auto' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 24 }}>
          <div style={{ fontWeight: 700, fontSize: 13, color: 'var(--text-dim)' }}>Let&apos;s build a workflow together</div>
          <button onClick={handleClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 20, color: 'var(--text-dim)', lineHeight: 1 }}>✕</button>
        </div>
        {step === 1 && <WfWizardStepGoal    value={goal}          onSelect={setGoal}    onNext={() => setStep(2)} />}
        {step === 2 && <WfWizardStepTrigger value={trigger}       onSelect={setTrigger} onBack={() => setStep(1)} onNext={() => setStep(3)} />}
        {step === 3 && <WfWizardStepActions value={actions}       onToggle={toggleAction} onBack={() => setStep(2)} onNext={() => setStep(4)} />}
        {step === 4 && <WfWizardStepApproval value={approvalRules} onToggle={toggleRule}  onBack={() => setStep(3)} onNext={() => setStep(5)} />}
        {step === 5 && <WfWizardStepReview   summary={summary}     creating={creating}    onBack={() => setStep(4)} onCreate={handleCreate} />}
      </div>
    </div>
  );
}

// ─── Cards ────────────────────────────────────────────────────────────────────

function ApprovalBadge({ required }) {
  if (!required) return null;
  return (
    <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: 'rgba(251,191,36,0.15)', color: '#fbbf24', border: '1px solid rgba(251,191,36,0.3)' }}>
      ⏸ Needs approval
    </span>
  );
}

function CategoryBadge({ category }) {
  const colors = { agency: 'var(--accent)', growth: '#059669', school: '#2563eb', general: '#6b7280' };
  const color = colors[category] ?? colors.general;
  return (
    <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: `${color}20`, color, border: `1px solid ${color}44`, textTransform: 'capitalize' }}>
      {category}
    </span>
  );
}

function WorkflowStarterCard({ template, creating, onUse }) {
  return (
    <div data-cy="workflow-starter-card" style={{ border: '1px solid var(--border)', borderRadius: 10, padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: 14 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8 }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>{template.title}</div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{template.bestFor}</div>
        </div>
        <ApprovalBadge required={template.needsApproval} />
      </div>
      <div style={{ fontSize: 13, color: 'var(--text-secondary, #ccc)', lineHeight: 1.5 }}>{template.description}</div>
      <div>
        <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 5 }}>Starts when: <span style={{ color: 'var(--text-secondary)' }}>{template.startsWhen}</span></div>
        <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 8 }}>What happens next:</div>
        <ol style={{ margin: 0, paddingLeft: 18, display: 'flex', flexDirection: 'column', gap: 3 }}>
          {template.whatHappensNext.map((step, i) => (
            <li key={i} style={{ fontSize: 12, color: 'var(--text-secondary, #ccc)' }}>{step}</li>
          ))}
        </ol>
      </div>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
        <button
          data-cy="use-workflow-template-btn"
          onClick={onUse}
          disabled={creating}
          style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: creating ? 'wait' : 'pointer', fontWeight: 600, fontSize: 13 }}
        >
          {creating ? 'Adding…' : 'Use this workflow'}
        </button>
        <CategoryBadge category={template.category} />
      </div>
    </div>
  );
}

function WorkflowCardSimple({ workflow, running, onRun, onOpenDetail }) {
  const statusColors = { on: '#22c55e', off: '#6b7280', ready: '#3b82f6', draft: '#6b7280' };
  const color = statusColors[workflow.status] ?? '#6b7280';
  return (
    <div data-cy="workflow-card-simple" style={{ border: '1px solid var(--border)', borderRadius: 8, padding: '14px 16px', display: 'flex', gap: 12, alignItems: 'center' }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>{workflow.name}</div>
        {workflow.description && <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{workflow.description}</div>}
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginTop: 6, flexWrap: 'wrap' }}>
          <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: `${color}20`, color, border: `1px solid ${color}44` }}>{workflow.status}</span>
          {workflow.trigger_label && workflow.trigger_label !== '—' && (
            <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>Starts when: {workflow.trigger_label}</span>
          )}
          <ApprovalBadge required={workflow.approval_required} />
        </div>
      </div>
      <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
        <button onClick={onRun} disabled={running} style={{ padding: '5px 12px', borderRadius: 6, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: running ? 'wait' : 'pointer', fontWeight: 600, fontSize: 12 }}>
          {running ? '…' : '▶ Run'}
        </button>
        <button onClick={onOpenDetail} style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontSize: 12 }}>
          Open
        </button>
      </div>
    </div>
  );
}

function WorkflowCardDetailed({ workflow, running, onRun, onOpenDetail }) {
  const statusColors = { on: '#22c55e', off: '#6b7280', ready: '#3b82f6', draft: '#6b7280' };
  const color = statusColors[workflow.status] ?? '#6b7280';
  return (
    <div data-cy="workflow-card-detailed" className="card">
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8, marginBottom: 10 }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 14 }}>{workflow.name}</div>
          {workflow.description && <div className="text-sm text-dim" style={{ marginTop: 2 }}>{workflow.description}</div>}
        </div>
        <span className={`badge`} style={{ fontSize: 11, background: `${color}20`, color, border: `1px solid ${color}44`, flexShrink: 0 }}>{workflow.status}</span>
      </div>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginBottom: 10 }}>
        {workflow.trigger_label && workflow.trigger_label !== '—' && (
          <span className="badge badge-muted" style={{ fontSize: 11 }}>Starts when: {workflow.trigger_label}</span>
        )}
        {workflow.steps > 0 && <span className="badge badge-dim" style={{ fontSize: 11 }}>{workflow.steps} step{workflow.steps !== 1 ? 's' : ''}</span>}
        <ApprovalBadge required={workflow.approval_required} />
        <CategoryBadge category={workflow.category} />
      </div>
      {(workflow.last_run_label || workflow.next_run_label) && (
        <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 10 }}>
          {workflow.last_run_label && <span>Last run: {workflow.last_run_label}</span>}
          {workflow.last_run_label && workflow.next_run_label && <span> · </span>}
          {workflow.next_run_label && <span>Next run: {workflow.next_run_label}</span>}
        </div>
      )}
      <div style={{ display: 'flex', gap: 8 }}>
        <button onClick={onRun} disabled={running} className="btn btn-primary btn-sm" style={{ flex: 1 }}>
          {running ? '…' : '▶ Run now'}
        </button>
        <button onClick={onOpenDetail} className="btn btn-ghost btn-sm">Open</button>
      </div>
    </div>
  );
}

// ─── Tab: Recommended ─────────────────────────────────────────────────────────

function RecommendedWorkflowsTab({ starters, workflows, creatingTemplateId, runningById, onUseTemplate, onOpenWizard, onRun }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
      {/* Wizard CTA */}
      <div style={{ padding: '20px 24px', background: 'rgba(124,58,237,0.07)', border: '1px solid rgba(124,58,237,0.28)', borderRadius: 10, display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>Not sure where to start?</div>
          <div style={{ fontSize: 13, color: 'var(--text-dim)', marginTop: 3 }}>
            {"Answer a few questions and we'll build the right workflow for you."}
          </div>
        </div>
        <button
          data-cy="open-wizard-from-rec"
          onClick={onOpenWizard}
          style={{ padding: '8px 18px', borderRadius: 8, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}
        >
          Set up a workflow for me
        </button>
      </div>

      {/* Starter templates */}
      <div>
        <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 14 }}>Starter workflows</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
          {starters.map(tmpl => (
            <WorkflowStarterCard
              key={tmpl.id}
              template={tmpl}
              creating={creatingTemplateId === tmpl.id}
              onUse={() => onUseTemplate(tmpl)}
            />
          ))}
        </div>
      </div>

      {/* Quick-run existing if any */}
      {workflows.length > 0 && (
        <div>
          <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 10 }}>Your workflows</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {workflows.slice(0, 5).map(wf => (
              <WorkflowCardSimple key={wf.id} workflow={wf} running={!!runningById[wf.id]} onRun={() => onRun(wf)} onOpenDetail={() => {}} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ─── Tab: My workflows ────────────────────────────────────────────────────────

function MyWorkflowsTab({ workflows, view, runningById, runResultById, onRun, onOpenDetail, onOpenWizard }) {
  const router = useRouter();

  if (workflows.length === 0) {
    return (
      <div data-cy="workflows-empty" style={{ padding: '48px 24px', textAlign: 'center', border: '1px dashed var(--border)', borderRadius: 10 }}>
        <div style={{ fontSize: 36, marginBottom: 12 }}>▶</div>
        <div style={{ fontSize: 17, fontWeight: 700, marginBottom: 6 }}>No workflows yet</div>
        <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 24, maxWidth: 360, margin: '0 auto 24px' }}>
          Workflows help OpenFang repeat the same process without missing steps.<br />Start with a template or use the guided setup.
        </div>
        <div style={{ display: 'flex', gap: 10, justifyContent: 'center', flexWrap: 'wrap' }}>
          <button
            data-cy="empty-open-wizard"
            onClick={onOpenWizard}
            style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 14 }}
          >
            Set up a workflow for me
          </button>
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {workflows.map(wf => {
        const handleRun = async () => onRun(wf);
        return view === 'simple' ? (
          <WorkflowCardSimple key={wf.id} workflow={wf} running={!!runningById[wf.id]} onRun={handleRun} onOpenDetail={() => onOpenDetail(wf.id)} />
        ) : (
          <WorkflowCardDetailed key={wf.id} workflow={wf} running={!!runningById[wf.id]} onRun={handleRun} onOpenDetail={() => onOpenDetail(wf.id)} />
        );
      })}
    </div>
  );
}

// ─── Tab: Templates ───────────────────────────────────────────────────────────

function WorkflowTemplatesTab({ starters, creatingTemplateId, onUseTemplate }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>
        {"Pick a starter template and we'll add it to your workflow list. You can edit it after."}
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
        {starters.map(tmpl => (
          <WorkflowStarterCard
            key={tmpl.id}
            template={tmpl}
            creating={creatingTemplateId === tmpl.id}
            onUse={() => onUseTemplate(tmpl)}
          />
        ))}
      </div>
    </div>
  );
}

// ─── Tab: Advanced ────────────────────────────────────────────────────────────

function AdvancedWorkflowsTab({ workflows, runningById, runResultById, onRun, loading, onOpenWizard }) {
  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <button onClick={onOpenWizard} style={{ padding: '6px 14px', borderRadius: 6, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}>
          + New workflow (guided)
        </button>
      </div>
      {workflows.length === 0 && !loading && (
        <div data-cy="workflows-empty" className="empty-state">
          <span style={{ fontSize: 28, opacity: 0.4 }}>▶</span>
          <div>
            <div style={{ fontWeight: 600, color: 'var(--text-secondary)', marginBottom: 4 }}>No workflows defined</div>
            <div className="text-dim text-sm">Create workflow TOML files to add automation pipelines, or use the guided setup above.</div>
          </div>
        </div>
      )}
      {workflows.length > 0 && (
        <div data-cy="workflows-table" className="card" style={{ padding: 0, overflow: 'hidden' }}>
          <table className="data-table">
            <thead>
              <tr>
                <th style={{ width: '28%' }}>Name</th>
                <th>Description</th>
                <th style={{ width: 60 }}>Steps</th>
                <th style={{ width: 100 }}>Status</th>
                <th style={{ width: 80 }}></th>
              </tr>
            </thead>
            <tbody>
              {workflows.map(w => (
                <tr data-cy="workflow-row" key={w.id}>
                  <td style={{ fontWeight: 600, color: 'var(--text)' }}>{w.name}</td>
                  <td style={{ fontSize: 12, color: 'var(--text-dim)', maxWidth: 280 }}>{w.description || <span className="text-muted">—</span>}</td>
                  <td style={{ fontSize: 12, color: 'var(--text-dim)' }}>{w.steps || '—'}</td>
                  <td>
                    {runResultById[w.id] ? (
                      <span data-cy="workflow-result-badge" className={`badge ${runResultById[w.id].ok ? 'badge-success' : 'badge-error'}`}>
                        {runResultById[w.id].ok ? 'Queued' : 'Error'}
                      </span>
                    ) : (
                      <span className="badge badge-dim">{w.status}</span>
                    )}
                  </td>
                  <td style={{ textAlign: 'right' }}>
                    <button
                      data-cy="workflow-run-btn"
                      className="btn btn-primary btn-xs"
                      onClick={() => onRun(w)}
                      disabled={!!runningById[w.id]}
                      title={`Run ${w.name}`}
                    >
                      {runningById[w.id] ? '…' : '▶ Run'}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ─── WorkflowsPageV2 — main export ───────────────────────────────────────────

export default function WorkflowsPageV2({ initialWorkflows }) {
  const router = useRouter();
  const raw = initialWorkflows ?? [];
  const hasWorkflows = raw.length > 0;

  const [activeTab,          setActiveTab]          = useState('recommended');
  const [activeView,         setActiveView]         = useState('simple');
  const [workflows,          setWorkflows]          = useState(raw.map(normalizeWorkflow));
  const [loading,            setLoading]            = useState(false);
  const [error,              setError]              = useState('');
  const [runningById,        setRunningById]        = useState({});
  const [runResultById,      setRunResultById]      = useState({});
  const [creatingTemplateId, setCreatingTemplateId] = useState(null);
  const [wizardOpen,         setWizardOpen]         = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/workflows');
      setWorkflows((Array.isArray(data) ? data : data?.workflows ?? []).map(normalizeWorkflow));
    } catch (e) {
      setError(e.message || 'Could not load workflows.');
    }
    setLoading(false);
  }, []);

  const runWorkflow = useCallback(async (wf) => {
    if (runningById[wf.id]) return;
    setRunningById(prev => ({ ...prev, [wf.id]: true }));
    setError('');
    try {
      const created = await workApi.createWork({
        title: wf.name,
        description: wf.description || '',
        work_type: 'workflow',
        payload: { workflow_template_id: wf.id },
      });
      setRunResultById(prev => ({ ...prev, [wf.id]: { ok: true, id: created.id } }));
      track('workflow_run', { id: wf.id });
      router.push(`/work/${created.id}`);
    } catch (e) {
      setRunResultById(prev => ({ ...prev, [wf.id]: { ok: false, msg: e.message } }));
      setError(e.message || 'Could not run workflow.');
    }
    setRunningById(prev => ({ ...prev, [wf.id]: false }));
  }, [runningById, router]);

  const useTemplate = useCallback(async (template) => {
    setCreatingTemplateId(template.id);
    setError('');
    try {
      await apiClient.post('/api/workflows', template.templatePayload);
      track('workflow_template_used', { templateId: template.id });
      await refresh();
      setActiveTab('my');
    } catch (e) {
      setError(e.message || `Could not add "${template.title}". The backend may not support workflow creation yet.`);
    }
    setCreatingTemplateId(null);
  }, [refresh]);

  const createFromWizard = useCallback(async (payload) => {
    setError('');
    try {
      await apiClient.post('/api/workflows', payload);
      track('workflow_wizard_created');
      await refresh();
      setActiveTab('my');
    } catch (e) {
      setError(e.message || 'Could not create workflow. The backend may not support workflow creation yet.');
      throw e;
    }
  }, [refresh]);

  const TABS = [
    { key: 'recommended', label: 'Recommended' },
    { key: 'my',          label: `My workflows${workflows.length > 0 ? ` (${workflows.length})` : ''}` },
    { key: 'templates',   label: 'Templates' },
    { key: 'advanced',    label: 'Advanced' },
  ];

  return (
    <div data-cy="workflows-page">
      {/* Wizard overlay */}
      <WorkflowWizard open={wizardOpen} onClose={() => setWizardOpen(false)} onCreate={createFromWizard} />

      {/* Header */}
      <div className="page-header">
        <div>
          <h1 style={{ margin: 0 }}>Workflows</h1>
          <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '4px 0 0' }}>
            Workflows help OpenFang repeat the same process without missing steps.
          </p>
        </div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          <button
            data-cy="open-wizard-btn"
            onClick={() => setWizardOpen(true)}
            style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}
          >
            Set up a workflow for me
          </button>
          <button className="btn btn-ghost btn-sm" onClick={() => { setActiveTab('templates'); }} style={{ fontSize: 13 }}>
            Use a starter template
          </button>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading} style={{ fontSize: 13 }}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && (
        <div data-cy="workflows-error" className="error-state" style={{ margin: '0 0 16px' }}>
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
              data-cy={`workflows-tab-${tab.key}`}
              onClick={() => setActiveTab(tab.key)}
              style={{
                padding: '10px 16px', background: 'transparent', border: 'none',
                borderBottom: `2px solid ${activeTab === tab.key ? 'var(--accent)' : 'transparent'}`,
                color: activeTab === tab.key ? 'var(--text-primary)' : 'var(--text-dim)',
                cursor: 'pointer', fontSize: 14, fontWeight: activeTab === tab.key ? 700 : 400,
              }}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {activeTab === 'my' && workflows.length > 0 && (
          <div style={{ display: 'flex', gap: 4 }}>
            {['simple', 'detailed'].map(v => (
              <button
                key={v}
                onClick={() => setActiveView(v)}
                style={{
                  padding: '4px 10px', borderRadius: 6,
                  background: activeView === v ? 'var(--accent)' : 'transparent',
                  border: `1px solid ${activeView === v ? 'var(--accent)' : 'var(--border)'}`,
                  color: activeView === v ? '#fff' : 'var(--text-dim)',
                  cursor: 'pointer', fontSize: 12, textTransform: 'capitalize',
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
          <RecommendedWorkflowsTab
            starters={WORKFLOW_STARTERS}
            workflows={workflows}
            creatingTemplateId={creatingTemplateId}
            runningById={runningById}
            onUseTemplate={useTemplate}
            onOpenWizard={() => setWizardOpen(true)}
            onRun={runWorkflow}
          />
        )}
        {activeTab === 'my' && (
          <MyWorkflowsTab
            workflows={workflows}
            view={activeView}
            runningById={runningById}
            runResultById={runResultById}
            onRun={runWorkflow}
            onOpenDetail={(id) => { /* TODO: detail drawer */ }}
            onOpenWizard={() => setWizardOpen(true)}
          />
        )}
        {activeTab === 'templates' && (
          <WorkflowTemplatesTab
            starters={WORKFLOW_STARTERS}
            creatingTemplateId={creatingTemplateId}
            onUseTemplate={useTemplate}
          />
        )}
        {activeTab === 'advanced' && (
          <AdvancedWorkflowsTab
            workflows={workflows}
            runningById={runningById}
            runResultById={runResultById}
            onRun={runWorkflow}
            loading={loading}
            onOpenWizard={() => setWizardOpen(true)}
          />
        )}
      </div>
    </div>
  );
}
