'use client';
import { useState, useCallback, useEffect } from 'react';
import Link from 'next/link';
import { apiClient } from '../../lib/api-client';
import { workApi } from '../../lib/work-api';
import { track } from '../../lib/telemetry';

// ─── Starter templates ────────────────────────────────────────────────────────

const SCHEDULE_STARTERS = [
  {
    id: 'daily_approval_check',
    title: 'Daily approvals check',
    description: 'Check what is waiting for approval every day.',
    bestFor: 'Agency Mode or Growth Mode',
    runsWhen: 'Every day',
    whatItRuns: 'Check approvals and prepare a summary',
    needsApproval: false,
    category: 'general',
    templatePayload: { name: 'Daily approvals check', action_type: 'check_approvals', frequency: 'daily', timezone: 'UTC', approval_rules: [] },
  },
  {
    id: 'weekly_planning_monday',
    title: 'Weekly planning',
    description: 'Build the weekly plan every Monday morning.',
    bestFor: 'Agency Mode or School Mode',
    runsWhen: 'Every Monday morning',
    whatItRuns: 'Create task list',
    needsApproval: false,
    category: 'general',
    templatePayload: { name: 'Weekly planning', action_type: 'create_task_list', frequency: 'weekly', timezone: 'UTC', approval_rules: [] },
  },
  {
    id: 'friday_followup_prep',
    title: 'Friday follow-up prep',
    description: 'Prepare follow-up work before the week ends.',
    bestFor: 'Agency Mode or Growth Mode',
    runsWhen: 'Every Friday',
    whatItRuns: 'Draft email or create reminders',
    needsApproval: true,
    category: 'general',
    templatePayload: { name: 'Friday follow-up prep', action_type: 'draft_email', frequency: 'weekly', timezone: 'UTC', approval_rules: ['pause_before_send'] },
  },
  {
    id: 'monthly_renewal_review',
    title: 'Monthly renewal review',
    description: 'Prepare renewal follow-up at the start of each month.',
    bestFor: 'Agency Mode',
    runsWhen: 'Every month',
    whatItRuns: 'Prepare report and task list',
    needsApproval: false,
    category: 'agency',
    templatePayload: { name: 'Monthly renewal review', action_type: 'prepare_report', frequency: 'monthly', timezone: 'UTC', approval_rules: [] },
  },
  {
    id: 'student_progress_friday',
    title: 'Student progress review',
    description: 'Build a student progress summary every Friday.',
    bestFor: 'School Mode',
    runsWhen: 'Every Friday',
    whatItRuns: 'Prepare report',
    needsApproval: false,
    category: 'school',
    templatePayload: { name: 'Student progress review', action_type: 'prepare_report', frequency: 'weekly', timezone: 'UTC', approval_rules: [] },
  },
  {
    id: 'morning_inbox_cleanup',
    title: 'Morning inbox cleanup',
    description: 'Create a clean task list from new messages each morning.',
    bestFor: 'Growth Mode or Agency Mode',
    runsWhen: 'Every weekday morning',
    whatItRuns: 'Create task list',
    needsApproval: false,
    category: 'general',
    templatePayload: { name: 'Morning inbox cleanup', action_type: 'create_task_list', frequency: 'weekday', timezone: 'UTC', approval_rules: [] },
  },
];

// ─── Wizard choice data ───────────────────────────────────────────────────────

const ACTIONS = [
  { value: 'run_workflow',     label: 'Run a workflow',      icon: '▶' },
  { value: 'create_reminder',  label: 'Create a reminder',  icon: '🔔' },
  { value: 'prepare_report',   label: 'Prepare a report',   icon: '📊' },
  { value: 'draft_email',      label: 'Draft an email',      icon: '✉' },
  { value: 'create_task_list', label: 'Create a task list',  icon: '📋' },
  { value: 'check_approvals',  label: 'Check approvals',     icon: '✅' },
];

const FREQUENCIES = [
  { value: 'once',    label: 'Once',           icon: '1️⃣' },
  { value: 'daily',   label: 'Every day',      icon: '📅' },
  { value: 'weekday', label: 'Every weekday',  icon: '🗓' },
  { value: 'weekly',  label: 'Every week',     icon: '📆' },
  { value: 'monthly', label: 'Every month',    icon: '🗃' },
  { value: 'custom',  label: 'Custom',         icon: '⚙' },
];

const APPROVAL_OPTIONS = [
  { value: 'pause_before_send',     label: 'Pause before sending',      icon: '📧' },
  { value: 'pause_before_publish',  label: 'Pause before publishing',   icon: '📢' },
  { value: 'pause_before_tool_use', label: 'Pause before using tools',  icon: '🔧' },
  { value: 'none',                  label: 'No approval needed',        icon: '⚡' },
];

// ─── Normalizer + helpers ─────────────────────────────────────────────────────

function fmtDate(iso) {
  if (!iso) return '—';
  try { return new Date(iso).toLocaleString(); } catch { return iso; }
}

function statusBadgeColor(s) {
  if (s === 'completed')       return '#22c55e';
  if (s === 'running')         return 'var(--accent)';
  if (s === 'failed' || s === 'cancelled') return '#ef4444';
  if (s === 'waiting_approval') return '#f59e0b';
  return '#6b7280';
}

function normalizeScheduleItem(raw, i) {
  return {
    id:               String(raw?.id ?? `sched-${i}`),
    name:             String(raw?.title ?? raw?.name ?? 'Untitled'),
    description:      String(raw?.description ?? ''),
    status:           String(raw?.status ?? 'draft'),
    runs_when_label:  raw?.scheduled_at ? `Scheduled for ${fmtDate(raw.scheduled_at)}` : '—',
    next_run_label:   raw?.scheduled_at ? fmtDate(raw.scheduled_at) : null,
    linked_work_label: null,
    approval_required: raw?.status === 'waiting_approval',
    category:         'general',
    raw,
  };
}

function actionLabel(v) {
  return ACTIONS.find(a => a.value === v)?.label ?? v ?? '—';
}
function frequencyLabel(v) {
  return FREQUENCIES.find(f => f.value === v)?.label ?? v ?? '—';
}
function approvalLabel(v) {
  return APPROVAL_OPTIONS.find(a => a.value === v)?.label ?? v ?? '—';
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

function ChoiceButton({ selected, onClick, icon, label }) {
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
    </button>
  );
}

function Label({ children }) {
  return <label style={{ fontSize: 12, color: 'var(--text-dim)', display: 'block', marginBottom: 4 }}>{children}</label>;
}

function TextInput({ label, value, onChange, placeholder, type = 'text' }) {
  return (
    <div style={{ marginBottom: 12 }}>
      <Label>{label}</Label>
      <input
        type={type}
        value={value}
        onChange={e => onChange(e.target.value)}
        placeholder={placeholder}
        style={{ width: '100%', padding: '8px 12px', borderRadius: 6, background: 'var(--input-bg)', border: '1px solid var(--border)', color: 'var(--text-primary)', fontSize: 13 }}
      />
    </div>
  );
}

function ReviewRow({ label, value }) {
  return (
    <div style={{ padding: '12px 14px', background: 'var(--surface2)', borderRadius: 8 }}>
      <div style={{ fontSize: 11, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.8, marginBottom: 4 }}>{label}</div>
      <div style={{ fontSize: 14, fontWeight: 500 }}>{value || '—'}</div>
    </div>
  );
}

// ─── Scheduler wizard steps ───────────────────────────────────────────────────

function SchWizardStepAction({ value, onSelect, onNext }) {
  return (
    <WizardStep step={1} title="What should happen?" subtitle="Tell us what the schedule will do." onNext={onNext} nextDisabled={!value}>
      {ACTIONS.map(a => (
        <ChoiceButton key={a.value} selected={value === a.value} onClick={() => onSelect(a.value)} icon={a.icon} label={a.label} />
      ))}
    </WizardStep>
  );
}

function SchWizardStepTiming({ value, onChange, onBack, onNext }) {
  const needsDate = value.frequency === 'once' || value.frequency === 'custom';
  return (
    <WizardStep step={2} title="When should it happen?" subtitle="Choose how often, then set the time." onBack={onBack} onNext={onNext} nextDisabled={!value.frequency}>
      <div style={{ marginBottom: 16 }}>
        <Label>How often</Label>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(160px, 1fr))', gap: 8 }}>
          {FREQUENCIES.map(f => (
            <button
              key={f.value}
              onClick={() => onChange({ frequency: f.value })}
              style={{
                padding: '10px 12px', borderRadius: 8,
                border: `1px solid ${value.frequency === f.value ? 'var(--accent)' : 'var(--border)'}`,
                background: value.frequency === f.value ? 'var(--accent-subtle)' : 'transparent',
                cursor: 'pointer', textAlign: 'left',
              }}
            >
              <div style={{ fontSize: 18 }}>{f.icon}</div>
              <div style={{ fontSize: 13, fontWeight: value.frequency === f.value ? 700 : 400, marginTop: 4 }}>{f.label}</div>
            </button>
          ))}
        </div>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: needsDate ? '1fr 1fr 1fr' : '1fr 1fr', gap: 10 }}>
        {needsDate && (
          <TextInput label="Date" type="date" value={value.date ?? ''} onChange={v => onChange({ date: v })} />
        )}
        <TextInput label="Time" type="time" value={value.time ?? ''} onChange={v => onChange({ time: v })} />
        <TextInput label="Timezone" value={value.timezone ?? 'UTC'} onChange={v => onChange({ timezone: v })} placeholder="UTC" />
      </div>
    </WizardStep>
  );
}

function SchWizardStepAudience({ value, onChange, onBack, onNext }) {
  return (
    <WizardStep step={3} title="Who is it for?" subtitle="This is optional — helps keep your schedules organised." onBack={onBack} onNext={onNext}>
      <TextInput label="Client or project name" value={value.audienceName} onChange={v => onChange({ audienceName: v })} placeholder="e.g. Acme Corp, Project Alpha" />
      <TextInput label="Optional email recipient" type="email" value={value.audienceEmail} onChange={v => onChange({ audienceEmail: v })} placeholder="someone@example.com" />
    </WizardStep>
  );
}

function SchWizardStepApproval({ value, onToggle, onBack, onNext }) {
  return (
    <WizardStep step={4} title="Should anything wait for approval?" subtitle="Work will pause at these points until someone approves." onBack={onBack} onNext={onNext}>
      {APPROVAL_OPTIONS.map(a => {
        const selected = value.includes(a.value);
        return (
          <button
            key={a.value}
            onClick={() => onToggle(a.value)}
            style={{
              display: 'flex', alignItems: 'center', gap: 10,
              padding: '10px 14px', borderRadius: 8, marginBottom: 6,
              border: `1px solid ${selected ? 'var(--accent)' : 'var(--border)'}`,
              background: selected ? 'var(--accent-subtle)' : 'transparent',
              cursor: 'pointer', width: '100%',
            }}
          >
            <span style={{ fontSize: 18, width: 24, textAlign: 'center' }}>{a.icon}</span>
            <span style={{ fontWeight: selected ? 700 : 400, fontSize: 13 }}>{a.label}</span>
            {selected && <span style={{ marginLeft: 'auto', color: 'var(--accent)', fontSize: 16 }}>✓</span>}
          </button>
        );
      })}
    </WizardStep>
  );
}

function SchWizardStepReview({ summary, creating, onBack, onCreate }) {
  return (
    <WizardStep step={5} title="Review and save" subtitle="Here's what your schedule will do. You can change it later." onBack={onBack} onNext={onCreate} nextLabel={creating ? 'Saving…' : 'Save schedule'} nextDisabled={creating}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
        <ReviewRow label="What will happen"     value={summary.actionLabel} />
        <ReviewRow label="When it will happen"  value={summary.timingLabel} />
        {summary.audienceLabel && <ReviewRow label="Who it affects" value={summary.audienceLabel} />}
        {summary.approvalLabels.length > 0 && (
          <ReviewRow label="What needs approval" value={summary.approvalLabels.join(', ')} />
        )}
      </div>
    </WizardStep>
  );
}

// ─── SchedulerWizard ──────────────────────────────────────────────────────────

function SchedulerWizard({ open, onClose, onCreate }) {
  const [step,     setStep]     = useState(1);
  const [action,   setAction]   = useState(null);
  const [timing,   setTiming]   = useState({ frequency: null, date: null, time: null, timezone: 'UTC' });
  const [audience, setAudience] = useState({ audienceName: '', audienceEmail: '' });
  const [approvalRules, setApprovalRules] = useState([]);
  const [creating, setCreating] = useState(false);

  const patchTiming   = (patch) => setTiming(prev => ({ ...prev, ...patch }));
  const patchAudience = (patch) => setAudience(prev => ({ ...prev, ...patch }));
  const toggleRule    = (v) => {
    if (v === 'none') { setApprovalRules(['none']); return; }
    setApprovalRules(prev => {
      const without = prev.filter(r => r !== 'none');
      return without.includes(v) ? without.filter(r => r !== v) : [...without, v];
    });
  };

  const summary = {
    actionLabel:   actionLabel(action),
    timingLabel:   [frequencyLabel(timing.frequency), timing.time, timing.timezone].filter(Boolean).join(' · '),
    audienceLabel: [audience.audienceName, audience.audienceEmail].filter(Boolean).join(' / '),
    approvalLabels: approvalRules.filter(r => r !== 'none').map(approvalLabel),
  };

  const handleCreate = async () => {
    setCreating(true);
    try {
      await onCreate({
        name:           `${actionLabel(action)} — ${frequencyLabel(timing.frequency)}`,
        description:    `Runs ${frequencyLabel(timing.frequency)}: ${actionLabel(action)}`,
        action_type:    action,
        frequency:      timing.frequency,
        date:           timing.date,
        time:           timing.time,
        timezone:       timing.timezone || 'UTC',
        audience_name:  audience.audienceName || undefined,
        audience_email: audience.audienceEmail || undefined,
        approval_rules: approvalRules.filter(r => r !== 'none'),
        enabled:        false,
      });
      handleClose();
    } catch {
      // parent handles error
    }
    setCreating(false);
  };

  const handleClose = () => {
    setStep(1); setAction(null); setTiming({ frequency: null, date: null, time: null, timezone: 'UTC' });
    setAudience({ audienceName: '', audienceEmail: '' }); setApprovalRules([]);
    onClose();
  };

  if (!open) return null;

  return (
    <div
      data-cy="scheduler-wizard"
      style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,0.65)', backdropFilter: 'blur(3px)', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 24 }}
      onClick={e => { if (e.target === e.currentTarget) handleClose(); }}
    >
      <div style={{ width: '100%', maxWidth: 560, background: 'var(--bg-elevated)', border: '1px solid var(--border)', borderRadius: 14, padding: '28px 32px', maxHeight: '90vh', overflowY: 'auto' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 24 }}>
          <div style={{ fontWeight: 700, fontSize: 13, color: 'var(--text-dim)' }}>Let&apos;s choose when this should happen</div>
          <button onClick={handleClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 20, color: 'var(--text-dim)', lineHeight: 1 }}>✕</button>
        </div>
        {step === 1 && <SchWizardStepAction   value={action}        onSelect={setAction}  onNext={() => setStep(2)} />}
        {step === 2 && <SchWizardStepTiming   value={timing}        onChange={patchTiming}  onBack={() => setStep(1)} onNext={() => setStep(3)} />}
        {step === 3 && <SchWizardStepAudience value={audience}      onChange={patchAudience} onBack={() => setStep(2)} onNext={() => setStep(4)} />}
        {step === 4 && <SchWizardStepApproval value={approvalRules} onToggle={toggleRule}    onBack={() => setStep(3)} onNext={() => setStep(5)} />}
        {step === 5 && <SchWizardStepReview   summary={summary} creating={creating}          onBack={() => setStep(4)} onCreate={handleCreate} />}
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
  const c = colors[category] ?? colors.general;
  return (
    <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: `${c}20`, color: c, border: `1px solid ${c}44`, textTransform: 'capitalize' }}>
      {category}
    </span>
  );
}

function ScheduleStarterCard({ template, creating, onUse }) {
  return (
    <div data-cy="schedule-starter-card" style={{ border: '1px solid var(--border)', borderRadius: 10, padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: 14 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8 }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>{template.title}</div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{template.bestFor}</div>
        </div>
        <ApprovalBadge required={template.needsApproval} />
      </div>
      <div style={{ fontSize: 13, color: 'var(--text-secondary, #ccc)', lineHeight: 1.5 }}>{template.description}</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
          How often: <span style={{ color: 'var(--text-secondary)' }}>{template.runsWhen}</span>
        </div>
        <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
          What it runs: <span style={{ color: 'var(--text-secondary)' }}>{template.whatItRuns}</span>
        </div>
      </div>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
        <button
          data-cy="use-schedule-template-btn"
          onClick={onUse}
          disabled={creating}
          style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: creating ? 'wait' : 'pointer', fontWeight: 600, fontSize: 13 }}
        >
          {creating ? 'Adding…' : 'Use this schedule'}
        </button>
        <CategoryBadge category={template.category} />
      </div>
    </div>
  );
}

function ScheduleCardSimple({ schedule, acting, onCancel, onRetry, onOpenDetail }) {
  const color = statusBadgeColor(schedule.status);
  return (
    <div data-cy="schedule-card-simple" style={{ border: '1px solid var(--border)', borderRadius: 8, padding: '14px 16px', display: 'flex', gap: 12, alignItems: 'center' }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>{schedule.name}</div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginTop: 5, flexWrap: 'wrap' }}>
          <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: `${color}20`, color, border: `1px solid ${color}44` }}>{schedule.status}</span>
          {schedule.next_run_label && <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>Next run: {schedule.next_run_label}</span>}
          <ApprovalBadge required={schedule.approval_required} />
        </div>
      </div>
      <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
        {schedule.status !== 'cancelled' && schedule.status !== 'completed' && (
          <button
            data-cy="schedule-cancel-btn"
            onClick={onCancel}
            disabled={acting === 'cancel'}
            style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--error-border, #ef4444)', color: 'var(--error, #ef4444)', cursor: 'pointer', fontSize: 12 }}
          >
            {acting === 'cancel' ? '…' : 'Cancel'}
          </button>
        )}
        {schedule.status === 'failed' && (
          <button
            data-cy="schedule-retry-btn"
            onClick={onRetry}
            disabled={acting === 'retry'}
            style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontSize: 12 }}
          >
            {acting === 'retry' ? '…' : 'Retry'}
          </button>
        )}
        <button onClick={onOpenDetail} style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontSize: 12 }}>
          Open
        </button>
      </div>
    </div>
  );
}

// ─── Calendar view ────────────────────────────────────────────────────────────

function ScheduleCalendarView({ schedules }) {
  const groups = {};
  schedules.forEach(s => {
    const key = s.next_run_label ?? 'Unscheduled';
    if (!groups[key]) groups[key] = [];
    groups[key].push(s);
  });
  const sortedKeys = Object.keys(groups).sort();

  if (schedules.length === 0) {
    return (
      <div style={{ padding: '32px 24px', textAlign: 'center', border: '1px dashed var(--border)', borderRadius: 10, color: 'var(--text-dim)', fontSize: 13 }}>
        No scheduled work to show in calendar view.
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>Showing {schedules.length} scheduled item{schedules.length !== 1 ? 's' : ''}</div>
      {sortedKeys.map(key => (
        <div key={key}>
          <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 8, padding: '6px 0', borderBottom: '1px solid var(--border)' }}>
            {key}
          </div>
          {groups[key].map(s => {
            const color = statusBadgeColor(s.status);
            return (
              <div key={s.id} style={{ display: 'flex', gap: 12, padding: '10px 14px', borderRadius: 8, background: 'var(--surface2)', marginBottom: 6, alignItems: 'center' }}>
                <div style={{ width: 8, height: 8, borderRadius: '50%', background: color, flexShrink: 0 }} />
                <div style={{ flex: 1 }}>
                  <div style={{ fontWeight: 600, fontSize: 13 }}>{s.name}</div>
                  {s.approval_required && <div style={{ fontSize: 11, color: '#fbbf24', marginTop: 2 }}>⏸ Waiting for approval</div>}
                </div>
                <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: `${color}20`, color, border: `1px solid ${color}44` }}>{s.status}</span>
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

// ─── Tabs ─────────────────────────────────────────────────────────────────────

function RecommendedSchedulesTab({ starters, schedules, creatingTemplateId, actingById, onUseTemplate, onOpenWizard, onCancel, onRetry }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
      {/* Wizard CTA */}
      <div style={{ padding: '20px 24px', background: 'rgba(124,58,237,0.07)', border: '1px solid rgba(124,58,237,0.28)', borderRadius: 10, display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>Not sure when things should run?</div>
          <div style={{ fontSize: 13, color: 'var(--text-dim)', marginTop: 3 }}>
            {"Tell us what needs to happen and we'll build the schedule for you."}
          </div>
        </div>
        <button
          data-cy="open-wizard-from-rec"
          onClick={onOpenWizard}
          style={{ padding: '8px 18px', borderRadius: 8, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}
        >
          Set up a schedule for me
        </button>
      </div>

      {/* Starter schedule cards */}
      <div>
        <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 14 }}>Starter schedules</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
          {starters.map(tmpl => (
            <ScheduleStarterCard
              key={tmpl.id}
              template={tmpl}
              creating={creatingTemplateId === tmpl.id}
              onUse={() => onUseTemplate(tmpl)}
            />
          ))}
        </div>
      </div>

      {/* Upcoming items */}
      {schedules.length > 0 && (
        <div>
          <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 10 }}>Upcoming scheduled work</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {schedules.slice(0, 5).map(s => (
              <ScheduleCardSimple
                key={s.id}
                schedule={s}
                acting={actingById[s.id]}
                onCancel={() => onCancel(s.raw)}
                onRetry={() => onRetry(s.raw)}
                onOpenDetail={() => {}}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function MySchedulesTab({ schedules, actingById, onCancel, onRetry, onOpenDetail, onOpenWizard }) {
  if (schedules.length === 0) {
    return (
      <div data-cy="scheduler-empty" style={{ padding: '48px 24px', textAlign: 'center', border: '1px dashed var(--border)', borderRadius: 10 }}>
        <div style={{ fontSize: 36, marginBottom: 12 }}>⏰</div>
        <div style={{ fontSize: 17, fontWeight: 700, marginBottom: 6 }}>No schedules yet</div>
        <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 24, maxWidth: 360, margin: '0 auto 24px' }}>
          The scheduler decides when work should happen.<br />Start with a template or use the guided setup.
        </div>
        <div style={{ display: 'flex', gap: 10, justifyContent: 'center', flexWrap: 'wrap' }}>
          <button
            data-cy="empty-open-wizard"
            onClick={onOpenWizard}
            style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 14 }}
          >
            Set up a schedule for me
          </button>
        </div>
      </div>
    );
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      {schedules.map(s => (
        <ScheduleCardSimple
          key={s.id}
          schedule={s}
          acting={actingById[s.id]}
          onCancel={() => onCancel(s.raw)}
          onRetry={() => onRetry(s.raw)}
          onOpenDetail={() => onOpenDetail(s.id)}
        />
      ))}
    </div>
  );
}

function ScheduleTemplatesTab({ starters, creatingTemplateId, onUseTemplate }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>
        {"Pick a starter schedule and we'll add it to your list. You can edit it after."}
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
        {starters.map(tmpl => (
          <ScheduleStarterCard
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

function AdvancedSchedulerTab({ schedules, actingById, onCancel, onRetry, loading, onOpenWizard }) {
  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <button onClick={onOpenWizard} style={{ padding: '6px 14px', borderRadius: 6, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}>
          + New schedule (guided)
        </button>
      </div>
      {schedules.length === 0 && !loading && (
        <div data-cy="scheduler-empty" className="empty-state">
          No scheduled work items. Create work items with a scheduled_at time to see them here.
        </div>
      )}
      {schedules.length > 0 && (
        <div data-cy="scheduled-list" className="card" style={{ padding: 0, overflow: 'hidden' }}>
          <table className="data-table">
            <thead>
              <tr>
                <th>Title</th>
                <th>Scheduled For</th>
                <th>Status</th>
                <th>Retries</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {schedules.map(s => {
                const raw = s.raw ?? {};
                return (
                  <tr key={s.id} data-cy="scheduled-item">
                    <td style={{ fontWeight: 600, maxWidth: 220 }}>
                      <Link href={`/work/${s.id}`} style={{ color: 'var(--accent)' }}>{s.name}</Link>
                    </td>
                    <td style={{ fontSize: 12, whiteSpace: 'nowrap' }}>{s.next_run_label ?? '—'}</td>
                    <td>
                      <span className="badge" style={{ fontSize: 11, background: `${statusBadgeColor(s.status)}20`, color: statusBadgeColor(s.status), border: `1px solid ${statusBadgeColor(s.status)}44` }}>
                        {s.status}
                      </span>
                    </td>
                    <td style={{ fontSize: 12, color: 'var(--text-dim)' }}>{raw.retry_count ?? 0} / {raw.max_retries ?? 0}</td>
                    <td style={{ display: 'flex', gap: 6 }}>
                      {s.status !== 'cancelled' && s.status !== 'completed' && (
                        <button
                          data-cy="schedule-cancel-btn"
                          className="btn btn-ghost btn-xs"
                          style={{ color: 'var(--error)' }}
                          onClick={() => onCancel(raw)}
                          disabled={actingById[s.id] === 'cancel'}
                        >
                          {actingById[s.id] === 'cancel' ? '…' : 'Cancel'}
                        </button>
                      )}
                      {s.status === 'failed' && (
                        <button
                          data-cy="schedule-retry-btn"
                          className="btn btn-ghost btn-xs"
                          onClick={() => onRetry(raw)}
                          disabled={actingById[s.id] === 'retry'}
                        >
                          {actingById[s.id] === 'retry' ? '…' : 'Retry'}
                        </button>
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

// ─── SchedulerPageV2 — main export ────────────────────────────────────────────

export default function SchedulerPageV2({ initialItems }) {
  const raw = initialItems ?? [];

  const [activeTab,          setActiveTab]          = useState('recommended');
  const [activeView,         setActiveView]         = useState('simple');
  const [schedules,          setSchedules]          = useState(raw.map(normalizeScheduleItem));
  const [loading,            setLoading]            = useState(false);
  const [error,              setError]              = useState('');
  const [actingById,         setActingById]         = useState({});
  const [creatingTemplateId, setCreatingTemplateId] = useState(null);
  const [wizardOpen,         setWizardOpen]         = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await workApi.getWork({ scheduled: 'true' });
      setSchedules((Array.isArray(data?.items) ? data.items : []).map(normalizeScheduleItem));
    } catch (e) {
      setError(e.message || 'Could not load scheduled items.');
    }
    setLoading(false);
  }, []);

  const handleCancel = useCallback(async (rawItem) => {
    const id = rawItem?.id;
    if (!id || !window.confirm('Cancel this scheduled item?')) return;
    setActingById(prev => ({ ...prev, [id]: 'cancel' }));
    try {
      await workApi.cancelWork(id);
      await refresh();
    } catch (e) {
      setError(e.message || 'Could not cancel item.');
    }
    setActingById(prev => ({ ...prev, [id]: null }));
  }, [refresh]);

  const handleRetry = useCallback(async (rawItem) => {
    const id = rawItem?.id;
    if (!id) return;
    setActingById(prev => ({ ...prev, [id]: 'retry' }));
    try {
      await workApi.retryWork(id);
      await refresh();
    } catch (e) {
      setError(e.message || 'Could not retry item.');
    }
    setActingById(prev => ({ ...prev, [id]: null }));
  }, [refresh]);

  const useTemplate = useCallback(async (template) => {
    setCreatingTemplateId(template.id);
    setError('');
    try {
      await apiClient.post('/api/schedules', template.templatePayload);
      track('schedule_template_used', { templateId: template.id });
      await refresh();
      setActiveTab('my');
    } catch (e) {
      setError(e.message || `Could not add "${template.title}". The backend may not support schedule creation yet.`);
    }
    setCreatingTemplateId(null);
  }, [refresh]);

  const createFromWizard = useCallback(async (payload) => {
    setError('');
    try {
      await apiClient.post('/api/schedules', payload);
      track('schedule_wizard_created');
      await refresh();
      setActiveTab('my');
    } catch (e) {
      setError(e.message || 'Could not create schedule. The backend may not support schedule creation yet.');
      throw e;
    }
  }, [refresh]);

  const TABS = [
    { key: 'recommended', label: 'Recommended' },
    { key: 'my',          label: `My schedules${schedules.length > 0 ? ` (${schedules.length})` : ''}` },
    { key: 'templates',   label: 'Templates' },
    { key: 'calendar',    label: 'Calendar view' },
    { key: 'advanced',    label: 'Advanced' },
  ];

  // The calendar view and simple view are active based on activeView for My tab
  const calendarActive = activeTab === 'calendar';

  return (
    <div data-cy="scheduler-page">
      {/* Wizard overlay */}
      <SchedulerWizard open={wizardOpen} onClose={() => setWizardOpen(false)} onCreate={createFromWizard} />

      {/* Header */}
      <div className="page-header">
        <div>
          <h1 style={{ margin: 0 }}>Scheduler</h1>
          <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '4px 0 0' }}>
            The scheduler decides when work should happen.
          </p>
        </div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          <button
            data-cy="open-wizard-btn"
            onClick={() => setWizardOpen(true)}
            style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent)', color: 'var(--text-inverse)', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}
          >
            Set up a schedule for me
          </button>
          <button className="btn btn-ghost btn-sm" onClick={() => setActiveTab('templates')} style={{ fontSize: 13 }}>
            Use a starter schedule
          </button>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading} style={{ fontSize: 13 }}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && (
        <div data-cy="scheduler-error" className="error-state" style={{ margin: '0 0 16px' }}>
          ⚠ {error}
          <button className="btn btn-ghost btn-sm" onClick={() => setError('')} style={{ marginLeft: 8 }}>Dismiss</button>
        </div>
      )}

      {/* Tab bar + view toggle */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', borderBottom: '1px solid var(--border)', marginBottom: 20 }}>
        <div style={{ display: 'flex', flexWrap: 'wrap' }}>
          {TABS.map(tab => (
            <button
              key={tab.key}
              data-cy={`scheduler-tab-${tab.key}`}
              onClick={() => setActiveTab(tab.key)}
              style={{
                padding: '10px 14px', background: 'transparent', border: 'none',
                borderBottom: `2px solid ${activeTab === tab.key ? 'var(--accent)' : 'transparent'}`,
                color: activeTab === tab.key ? 'var(--text-primary)' : 'var(--text-dim)',
                cursor: 'pointer', fontSize: 13, fontWeight: activeTab === tab.key ? 700 : 400,
              }}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {activeTab === 'my' && schedules.length > 0 && (
          <div style={{ display: 'flex', gap: 4 }}>
            {['simple', 'calendar'].map(v => (
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
          <RecommendedSchedulesTab
            starters={SCHEDULE_STARTERS}
            schedules={schedules}
            creatingTemplateId={creatingTemplateId}
            actingById={actingById}
            onUseTemplate={useTemplate}
            onOpenWizard={() => setWizardOpen(true)}
            onCancel={handleCancel}
            onRetry={handleRetry}
          />
        )}
        {activeTab === 'my' && (
          activeView === 'calendar' ? (
            <ScheduleCalendarView schedules={schedules} onOpenDetail={() => {}} />
          ) : (
            <MySchedulesTab
              schedules={schedules}
              actingById={actingById}
              onCancel={handleCancel}
              onRetry={handleRetry}
              onOpenDetail={() => {}}
              onOpenWizard={() => setWizardOpen(true)}
            />
          )
        )}
        {activeTab === 'templates' && (
          <ScheduleTemplatesTab
            starters={SCHEDULE_STARTERS}
            creatingTemplateId={creatingTemplateId}
            onUseTemplate={useTemplate}
          />
        )}
        {activeTab === 'calendar' && (
          <ScheduleCalendarView schedules={schedules} onOpenDetail={() => {}} />
        )}
        {activeTab === 'advanced' && (
          <AdvancedSchedulerTab
            schedules={schedules}
            actingById={actingById}
            onCancel={handleCancel}
            onRetry={handleRetry}
            loading={loading}
            onOpenWizard={() => setWizardOpen(true)}
          />
        )}
      </div>
    </div>
  );
}
