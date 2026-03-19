'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';
import { track } from '../../lib/telemetry';
import CommsDraftDrawer from './CommsDraftDrawer';

// ─── Starters ─────────────────────────────────────────────────────────────────

const COMMS_STARTERS = [
  { id: 'client_followup', title: 'Client follow-up', description: 'Prepare a client follow-up message and stop for review before sending.', bestFor: 'Agency Mode', channel: 'email', approvalRequired: true, templatePayload: { goal: 'followup', channel: 'email', approval_required: true } },
  { id: 'lead_outreach', title: 'Lead outreach', description: 'Prepare an outreach draft for a lead and wait for approval.', bestFor: 'Growth Mode', channel: 'email', approvalRequired: true, templatePayload: { goal: 'outreach', channel: 'email', approval_required: true } },
  { id: 'approval_digest', title: 'Approval digest', description: 'Prepare a summary of work that needs approval.', bestFor: 'Agency or Growth', channel: 'internal', approvalRequired: false, templatePayload: { goal: 'approval-digest', channel: 'internal', approval_required: false } },
  { id: 'student_welcome_message', title: 'Student welcome message', description: 'Prepare a welcome message for a new student.', bestFor: 'School Mode', channel: 'email', approvalRequired: true, templatePayload: { goal: 'welcome-message', channel: 'email', approval_required: true } },
  { id: 'weekly_checkin', title: 'Weekly check-in', description: 'Prepare a weekly check-in draft for clients or students.', bestFor: 'General', channel: 'email', approvalRequired: true, templatePayload: { goal: 'weekly-checkin', channel: 'email', approval_required: true } },
];

const GOALS = [
  { value: 'followup',        label: 'Follow up with someone', icon: '↩' },
  { value: 'outreach',        label: 'Reach out to a lead',    icon: '📣' },
  { value: 'approval-digest', label: 'Summarise approvals',    icon: '✅' },
  { value: 'welcome-message', label: 'Welcome a new person',   icon: '👋' },
  { value: 'weekly-checkin',  label: 'Weekly check-in',        icon: '📅' },
];

const CHANNELS = [
  { value: 'email',    label: 'Email',           icon: '✉' },
  { value: 'internal', label: 'Internal message', icon: '💬' },
];

// ─── CommsClient helpers (preserved for Advanced tab) ─────────────────────────

const KIND_LABELS = { agent_message: 'Message', agent_spawned: 'Spawned', agent_terminated: 'Terminated', task_posted: 'Task Posted', task_claimed: 'Task Claimed', task_completed: 'Task Completed' };

function kindBadge(kind) {
  if (kind === 'agent_message')    return <span className="badge badge-info">Message</span>;
  if (kind === 'agent_spawned')    return <span className="badge badge-success">Spawned</span>;
  if (kind === 'agent_terminated') return <span className="badge badge-muted">Terminated</span>;
  if (kind === 'task_posted')      return <span className="badge badge-created">Task Posted</span>;
  if (kind === 'task_claimed')     return <span className="badge badge-warn">Task Claimed</span>;
  if (kind === 'task_completed')   return <span className="badge badge-success">Task Done</span>;
  return <span className="badge badge-dim">{KIND_LABELS[kind] ?? kind}</span>;
}

function stateBadge(state) {
  const s = (state || '').toLowerCase();
  if (s === 'running')                  return <span className="badge badge-success">{state}</span>;
  if (s === 'suspended' || s === 'idle') return <span className="badge badge-muted">{state}</span>;
  if (s === 'error')                    return <span className="badge badge-error">{state}</span>;
  return <span className="badge badge-dim">{state || 'unknown'}</span>;
}

function fmtTime(ts) {
  if (!ts) return '—';
  try { return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' }); } catch { return ts; }
}

function fmtDate(iso) {
  if (!iso) return '—';
  try { return new Date(iso).toLocaleString(); } catch { return iso; }
}

// ─── Wizard primitives ────────────────────────────────────────────────────────

function WizardStep({ step, total = 5, title, subtitle, onBack, onNext, nextLabel, nextDisabled, children }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      <div style={{ fontSize: 11, color: 'var(--text-dim, #888)', letterSpacing: 1, textTransform: 'uppercase' }}>Step {step} of {total}</div>
      <div>
        <h2 style={{ fontSize: 20, fontWeight: 700, margin: 0 }}>{title}</h2>
        {subtitle && <p style={{ fontSize: 13, color: 'var(--text-dim, #888)', margin: '6px 0 0', lineHeight: 1.6 }}>{subtitle}</p>}
      </div>
      <div>{children}</div>
      <div style={{ display: 'flex', gap: 10, justifyContent: 'flex-end', paddingTop: 8, borderTop: '1px solid var(--border, #2a2a3e)' }}>
        {onBack && <button onClick={onBack} style={{ padding: '8px 18px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border, #333)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontSize: 13 }}>← Back</button>}
        {onNext && <button onClick={onNext} disabled={nextDisabled} style={{ padding: '8px 20px', borderRadius: 6, background: 'var(--accent, #7c3aed)', color: '#fff', border: 'none', cursor: nextDisabled ? 'not-allowed' : 'pointer', fontWeight: 700, fontSize: 13, opacity: nextDisabled ? 0.5 : 1 }}>{nextLabel ?? 'Next →'}</button>}
      </div>
    </div>
  );
}

function ChoiceBtn({ selected, onClick, icon, label }) {
  return (
    <button onClick={onClick} style={{ display: 'block', width: '100%', textAlign: 'left', padding: '11px 16px', borderRadius: 8, marginBottom: 8, border: `1px solid ${selected ? 'var(--accent, #7c3aed)' : 'var(--border, #333)'}`, background: selected ? 'rgba(124,58,237,0.10)' : 'transparent', cursor: 'pointer' }}>
      {icon && <span style={{ marginRight: 10 }}>{icon}</span>}
      <span style={{ fontWeight: selected ? 700 : 400, fontSize: 14 }}>{label}</span>
    </button>
  );
}

function TxtInput({ label, value, onChange, placeholder, type = 'text' }) {
  return (
    <div style={{ marginBottom: 12 }}>
      <label style={{ fontSize: 12, color: 'var(--text-dim, #888)', display: 'block', marginBottom: 4 }}>{label}</label>
      <input type={type} value={value} onChange={e => onChange(e.target.value)} placeholder={placeholder} style={{ width: '100%', padding: '8px 12px', borderRadius: 6, background: 'var(--input-bg, #1a1a2e)', border: '1px solid var(--border, #333)', color: 'var(--text-primary, #fff)', fontSize: 13 }} />
    </div>
  );
}

// ─── Wizard steps ─────────────────────────────────────────────────────────────

function StepGoal({ value, onSelect, onNext }) {
  return (
    <WizardStep step={1} title="What do you need to send?" subtitle="Pick the type of message and we'll draft it for you." onNext={onNext} nextDisabled={!value}>
      {GOALS.map(g => <ChoiceBtn key={g.value} selected={value === g.value} onClick={() => onSelect(g.value)} icon={g.icon} label={g.label} />)}
    </WizardStep>
  );
}

function StepAudience({ name, email, onChange, onBack, onNext }) {
  return (
    <WizardStep step={2} title="Who is it going to?" subtitle="This helps us write the right message." onBack={onBack} onNext={onNext}>
      <TxtInput label="Name or company" value={name} onChange={v => onChange('name', v)} placeholder="e.g. Acme Corp, Sarah" />
      <TxtInput label="Email (optional)" type="email" value={email} onChange={v => onChange('email', v)} placeholder="someone@example.com" />
    </WizardStep>
  );
}

function StepChannel({ value, onSelect, onBack, onNext }) {
  return (
    <WizardStep step={3} title="How should it be sent?" onBack={onBack} onNext={onNext} nextDisabled={!value}>
      {CHANNELS.map(c => <ChoiceBtn key={c.value} selected={value === c.value} onClick={() => onSelect(c.value)} icon={c.icon} label={c.label} />)}
    </WizardStep>
  );
}

function StepApproval({ value, onChange, onBack, onNext }) {
  return (
    <WizardStep step={4} title="Should someone review before it sends?" onBack={onBack} onNext={onNext}>
      <ChoiceBtn selected={value === true}  onClick={() => onChange(true)}  icon="⏸" label="Yes — pause and wait for approval" />
      <ChoiceBtn selected={value === false} onClick={() => onChange(false)} icon="⚡" label="No — draft is enough for now" />
    </WizardStep>
  );
}

function StepReview({ state, creating, onBack, onCreate }) {
  const goalLabel = GOALS.find(g => g.value === state.goal)?.label ?? state.goal ?? '—';
  const channelLabel = CHANNELS.find(c => c.value === state.channel)?.label ?? state.channel ?? '—';
  return (
    <WizardStep step={5} title="Review and create" onBack={onBack} onNext={onCreate} nextLabel={creating ? 'Creating…' : 'Create draft'} nextDisabled={creating}>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
        {[['What to send', goalLabel], ['To', state.audienceName || '—'], ['Channel', channelLabel], ['Needs approval', state.approvalRequired ? 'Yes' : 'No']].map(([l, v]) => (
          <div key={l} style={{ padding: '10px 14px', background: 'var(--surface2, #1a1a2e)', borderRadius: 8 }}>
            <div style={{ fontSize: 11, color: 'var(--text-dim, #888)', textTransform: 'uppercase', letterSpacing: 0.8, marginBottom: 4 }}>{l}</div>
            <div style={{ fontSize: 14, fontWeight: 500 }}>{v}</div>
          </div>
        ))}
      </div>
    </WizardStep>
  );
}

// ─── CommsWizard ──────────────────────────────────────────────────────────────

function CommsWizard({ open, onClose, onCreateDraft }) {
  const [step,   setStep]   = useState(1);
  const [goal,   setGoal]   = useState(null);
  const [audienceName, setAudienceName] = useState('');
  const [audienceEmail, setAudienceEmail] = useState('');
  const [channel, setChannel] = useState(null);
  const [approvalRequired, setApprovalRequired] = useState(true);
  const [creating, setCreating] = useState(false);

  const handleCreate = async () => {
    setCreating(true);
    try {
      await onCreateDraft({ goal, audience_name: audienceName || 'Unknown', audience_email: audienceEmail || undefined, channel, approval_required: approvalRequired });
      handleClose();
    } catch { /* parent shows error */ }
    setCreating(false);
  };

  const handleClose = () => {
    setStep(1); setGoal(null); setAudienceName(''); setAudienceEmail(''); setChannel(null); setApprovalRequired(true); onClose();
  };

  if (!open) return null;

  return (
    <div data-cy="comms-wizard" style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,0.65)', backdropFilter: 'blur(3px)', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 24 }} onClick={e => { if (e.target === e.currentTarget) handleClose(); }}>
      <div style={{ width: '100%', maxWidth: 520, background: 'var(--bg-elevated, #111)', border: '1px solid var(--border, #333)', borderRadius: 14, padding: '28px 32px', maxHeight: '90vh', overflowY: 'auto' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 24 }}>
          <div style={{ fontWeight: 700, fontSize: 13, color: 'var(--text-dim, #888)' }}>Create a draft message</div>
          <button onClick={handleClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 20, color: 'var(--text-dim, #888)' }}>✕</button>
        </div>
        {step === 1 && <StepGoal    value={goal}    onSelect={setGoal}  onNext={() => setStep(2)} />}
        {step === 2 && <StepAudience name={audienceName} email={audienceEmail} onChange={(k, v) => k === 'name' ? setAudienceName(v) : setAudienceEmail(v)} onBack={() => setStep(1)} onNext={() => setStep(3)} />}
        {step === 3 && <StepChannel value={channel}  onSelect={setChannel} onBack={() => setStep(2)} onNext={() => setStep(4)} />}
        {step === 4 && <StepApproval value={approvalRequired} onChange={setApprovalRequired} onBack={() => setStep(3)} onNext={() => setStep(5)} />}
        {step === 5 && <StepReview state={{ goal, audienceName, channel, approvalRequired }} creating={creating} onBack={() => setStep(4)} onCreate={handleCreate} />}
      </div>
    </div>
  );
}

// ─── Cards ────────────────────────────────────────────────────────────────────

function Badge({ color, children }) {
  return <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 999, background: `${color}20`, color, border: `1px solid ${color}44` }}>{children}</span>;
}

function statusColor(s) {
  if (s === 'sent' || s === 'ready')     return '#22c55e';
  if (s === 'waiting_approval')          return '#f59e0b';
  if (s === 'new')                       return '#7c3aed';
  if (s === 'archived')                  return '#6b7280';
  return '#6b7280';
}

function CommsStarterCard({ template, creating, onUse }) {
  return (
    <div data-cy="comms-starter-card" style={{ border: '1px solid var(--border, #333)', borderRadius: 10, padding: '16px 18px', display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div>
        <div style={{ fontWeight: 700, fontSize: 15 }}>{template.title}</div>
        <div style={{ fontSize: 12, color: 'var(--text-dim, #888)', marginTop: 2 }}>{template.bestFor}</div>
      </div>
      <div style={{ fontSize: 13, color: 'var(--text-secondary, #ccc)', lineHeight: 1.5 }}>{template.description}</div>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
        <Badge color="#7c3aed">{template.channel}</Badge>
        {template.approvalRequired && <Badge color="#f59e0b">⏸ Needs approval</Badge>}
      </div>
      <button data-cy="use-comms-template-btn" onClick={onUse} disabled={creating} style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent, #7c3aed)', color: '#fff', border: 'none', cursor: creating ? 'wait' : 'pointer', fontWeight: 600, fontSize: 13 }}>
        {creating ? 'Creating…' : 'Use this template'}
      </button>
    </div>
  );
}

function ThreadCardSimple({ thread, onOpen }) {
  const color = statusColor(thread.status);
  return (
    <div data-cy="thread-card" style={{ border: '1px solid var(--border, #333)', borderRadius: 8, padding: '12px 16px', display: 'flex', gap: 12, alignItems: 'center', cursor: 'pointer' }} onClick={onOpen}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>{thread.subject}</div>
        <div style={{ fontSize: 12, color: 'var(--text-dim, #888)', marginTop: 2 }}>{thread.preview}</div>
      </div>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexShrink: 0 }}>
        <Badge color={color}>{thread.status.replace('_', ' ')}</Badge>
        {thread.approval_required && <Badge color="#f59e0b">⏸</Badge>}
      </div>
    </div>
  );
}

function DraftCard({ draft, acting, onApprove, onSend, onOpen }) {
  const color = statusColor(draft.approval_status === 'approved' ? 'ready' : draft.status);
  return (
    <div data-cy="draft-card" style={{ border: '1px solid var(--border, #333)', borderRadius: 8, padding: '12px 16px', display: 'flex', gap: 12, alignItems: 'center' }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>{draft.subject}</div>
        <div style={{ fontSize: 12, color: 'var(--text-dim, #888)', marginTop: 2 }}>{draft.channel} · {draft.recipients?.join(', ') || '—'}</div>
      </div>
      <div style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
        {draft.send_ready && <button data-cy="draft-send-btn" onClick={onSend} disabled={acting === 'send'} style={{ padding: '5px 10px', borderRadius: 6, background: 'var(--accent, #7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontSize: 12, fontWeight: 600 }}>{acting === 'send' ? '…' : 'Send'}</button>}
        {draft.approval_required && draft.approval_status === 'pending' && <button data-cy="draft-approve-btn" onClick={onApprove} disabled={acting === 'approve'} style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid #22c55e', color: '#22c55e', cursor: 'pointer', fontSize: 12 }}>{acting === 'approve' ? '…' : 'Approve'}</button>}
        <button onClick={onOpen} style={{ padding: '5px 10px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border, #333)', color: 'var(--text-secondary, #ccc)', cursor: 'pointer', fontSize: 12 }}>Open</button>
      </div>
    </div>
  );
}

// ─── Tabs ─────────────────────────────────────────────────────────────────────

function RecommendedCommsTab({ starters, creatingId, onUseTemplate, onOpenWizard }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 24 }}>
      <div style={{ padding: '18px 22px', background: 'rgba(124,58,237,0.07)', border: '1px solid rgba(124,58,237,0.28)', borderRadius: 10, display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>Need a custom message?</div>
          <div style={{ fontSize: 13, color: 'var(--text-dim, #888)', marginTop: 3 }}>{"Tell us who it's for and what to say — we'll write the draft."}</div>
        </div>
        <button data-cy="open-comms-wizard-from-rec" onClick={onOpenWizard} style={{ padding: '8px 18px', borderRadius: 8, background: 'var(--accent, #7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}>
          Create a draft for me
        </button>
      </div>
      <div>
        <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 12 }}>Starter templates</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(260px, 1fr))', gap: 12 }}>
          {starters.map(t => <CommsStarterCard key={t.id} template={t} creating={creatingId === t.id} onUse={() => onUseTemplate(t)} />)}
        </div>
      </div>
    </div>
  );
}

function InboxCommsTab({ threads, loading, onOpenWizard }) {
  if (threads.length === 0 && !loading) {
    return (
      <div data-cy="comms-inbox-empty" style={{ padding: '48px 24px', textAlign: 'center', border: '1px dashed var(--border, #333)', borderRadius: 10 }}>
        <div style={{ fontSize: 36, marginBottom: 12 }}>📭</div>
        <div style={{ fontSize: 17, fontWeight: 700, marginBottom: 6 }}>Inbox is empty</div>
        <div style={{ fontSize: 13, color: 'var(--text-dim, #888)', marginBottom: 24 }}>No threads yet. Start by creating a draft.</div>
        <button onClick={onOpenWizard} style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent, #7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 14 }}>Create a draft for me</button>
      </div>
    );
  }
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      {threads.map(t => <ThreadCardSimple key={t.id} thread={t} onOpen={() => {}} />)}
    </div>
  );
}

function DraftsCommsTab({ drafts, actingById, onApprove, onSend, loading, onOpenWizard }) {
  if (drafts.length === 0 && !loading) {
    return (
      <div data-cy="comms-drafts-empty" style={{ padding: '48px 24px', textAlign: 'center', border: '1px dashed var(--border, #333)', borderRadius: 10 }}>
        <div style={{ fontSize: 36, marginBottom: 12 }}>📝</div>
        <div style={{ fontSize: 17, fontWeight: 700, marginBottom: 6 }}>No drafts yet</div>
        <div style={{ fontSize: 13, color: 'var(--text-dim, #888)', marginBottom: 24 }}>Drafts waiting for review or send will appear here.</div>
        <button onClick={onOpenWizard} style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent, #7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 14 }}>Create a draft for me</button>
      </div>
    );
  }
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      {drafts.map(d => (
        <DraftCard key={d.id} draft={d} acting={actingById[d.id]} onApprove={() => onApprove(d.id)} onSend={() => onSend(d.id)} onOpen={() => {}} />
      ))}
    </div>
  );
}

function AdvancedCommsTab({ topology, events, loading, error, onRefresh }) {
  const [innerTab, setInnerTab] = useState('topology');
  const nodes = topology?.nodes ?? [];
  const edges = topology?.edges ?? [];
  const tabStyle = active => ({ padding: '5px 14px', borderRadius: 'var(--radius-sm)', fontSize: 13, fontWeight: active ? 600 : 400, cursor: 'pointer', background: active ? 'var(--accent-subtle)' : 'transparent', color: active ? 'var(--accent)' : 'var(--text-dim)', border: '1px solid ' + (active ? 'rgba(255,106,26,0.2)' : 'transparent') });

  return (
    <div>
      {error && (
        <div data-cy="comms-error" className="error-state" style={{ marginBottom: 12 }}>⚠ {error} <button className="btn btn-ghost btn-sm" onClick={onRefresh}>Retry</button></div>
      )}
      <div style={{ display: 'flex', gap: 6, marginBottom: 16 }}>
        <button data-cy="comms-tab-topology" style={tabStyle(innerTab === 'topology')} onClick={() => setInnerTab('topology')}>Topology ({nodes.length})</button>
        <button data-cy="comms-tab-events"   style={tabStyle(innerTab === 'events')}   onClick={() => setInnerTab('events')}>Events ({events.length})</button>
      </div>

      {innerTab === 'topology' && (
        <div data-cy="comms-topology-panel">
          {nodes.length === 0 && !error && <div data-cy="comms-empty-topology" className="empty-state">No agents in topology yet.</div>}
          {nodes.length > 0 && (
            <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
              <table data-cy="comms-topology-table" className="data-table">
                <thead><tr><th>Agent</th><th>Model</th><th>State</th><th>Connections</th></tr></thead>
                <tbody>
                  {nodes.map(node => {
                    const connections = edges.filter(e => e.from === node.id || e.to === node.id).length;
                    return (
                      <tr key={node.id}>
                        <td style={{ fontWeight: 600 }}>{node.name}</td>
                        <td><code style={{ fontSize: 11 }}>{node.model || '—'}</code></td>
                        <td>{stateBadge(node.state)}</td>
                        <td style={{ fontSize: 12, color: 'var(--text-dim)' }}>{connections > 0 ? connections : '—'}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
          {edges.length > 0 && (
            <div style={{ marginTop: 16 }}>
              <div style={{ fontSize: 12, fontWeight: 600, color: 'var(--text-muted)', marginBottom: 8, textTransform: 'uppercase', letterSpacing: '0.5px' }}>Edges ({edges.length})</div>
              <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
                <table className="data-table">
                  <thead><tr><th>From</th><th>Kind</th><th>To</th></tr></thead>
                  <tbody>
                    {edges.map((e, i) => {
                      const fromNode = nodes.find(n => n.id === e.from);
                      const toNode   = nodes.find(n => n.id === e.to);
                      return <tr key={i}><td>{fromNode?.name ?? e.from}</td><td><span className="badge badge-dim">{e.kind}</span></td><td>{toNode?.name ?? e.to}</td></tr>;
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      )}

      {innerTab === 'events' && (
        <div data-cy="comms-events-panel">
          {events.length === 0 && !error && <div data-cy="comms-empty-events" className="empty-state">No communication events yet.</div>}
          {events.length > 0 && (
            <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
              <table data-cy="comms-events-table" className="data-table">
                <thead><tr><th>Time</th><th>Kind</th><th>From</th><th>To</th><th>Detail</th></tr></thead>
                <tbody>
                  {events.map(ev => (
                    <tr key={ev.id}>
                      <td style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>{fmtTime(ev.timestamp)}</td>
                      <td>{kindBadge(ev.kind)}</td>
                      <td style={{ fontSize: 12 }}>{ev.source_name || ev.source_id || '—'}</td>
                      <td style={{ fontSize: 12 }}>{ev.target_name || ev.target_id || '—'}</td>
                      <td style={{ fontSize: 12, color: 'var(--text-dim)', maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{ev.detail || '—'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── CommsPageV2 ──────────────────────────────────────────────────────────────

export default function CommsPageV2({ initialTopology, initialEvents }) {
  const [activeTab,    setActiveTab]    = useState('recommended');
  const [topology,     setTopology]     = useState(initialTopology ?? { nodes: [], edges: [] });
  const [events,       setEvents]       = useState(initialEvents ?? []);
  const [threads,      setThreads]      = useState([]);
  const [drafts,       setDrafts]       = useState([]);
  const [loading,      setLoading]      = useState(false);
  const [error,        setError]        = useState('');
  const [actingById,   setActingById]   = useState({});
  const [creatingId,   setCreatingId]   = useState(null);
  const [wizardOpen,   setWizardOpen]   = useState(false);
  const [drawerDraftId, setDrawerDraftId] = useState(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const [topoData, eventsData, threadsData, draftsData] = await Promise.allSettled([
        apiClient.get('/api/comms/topology'),
        apiClient.get('/api/comms/events?limit=50'),
        apiClient.get('/api/comms/threads'),
        apiClient.get('/api/comms/drafts'),
      ]);
      if (topoData.status === 'fulfilled' && topoData.value)   setTopology(topoData.value);
      if (eventsData.status === 'fulfilled' && Array.isArray(eventsData.value)) setEvents(eventsData.value);
      if (threadsData.status === 'fulfilled' && threadsData.value?.items)  setThreads(threadsData.value.items);
      if (draftsData.status === 'fulfilled'  && draftsData.value?.items)   setDrafts(draftsData.value.items);
    } catch (e) {
      setError(e?.message || 'Could not refresh comms data.');
    }
    setLoading(false);
  }, []);

  const useTemplate = useCallback(async (template) => {
    setCreatingId(template.id);
    setError('');
    try {
      await apiClient.post('/api/comms/drafts', template.templatePayload);
      track('comms_template_used', { templateId: template.id });
      await refresh();
      setActiveTab('drafts');
    } catch (e) {
      setError(e?.message || `Could not create draft. Backend may not support /api/comms/drafts yet.`);
    }
    setCreatingId(null);
  }, [refresh]);

  const createDraft = useCallback(async (payload) => {
    setError('');
    try {
      await apiClient.post('/api/comms/drafts', payload);
      track('comms_wizard_draft_created');
      await refresh();
      setActiveTab('drafts');
    } catch (e) {
      setError(e?.message || 'Could not create draft.');
      throw e;
    }
  }, [refresh]);

  const approveDraft = useCallback(async (id) => {
    setActingById(prev => ({ ...prev, [id]: 'approve' }));
    try {
      await apiClient.post(`/api/comms/drafts/${id}/approve`, {});
      await refresh();
    } catch (e) {
      setError(e?.message || 'Could not approve draft.');
    }
    setActingById(prev => ({ ...prev, [id]: null }));
  }, [refresh]);

  const sendDraft = useCallback(async (id) => {
    if (!window.confirm('Send this draft now?')) return;
    setActingById(prev => ({ ...prev, [id]: 'send' }));
    try {
      await apiClient.post(`/api/comms/drafts/${id}/send`, {});
      await refresh();
    } catch (e) {
      setError(e?.message || 'Could not send draft.');
    }
    setActingById(prev => ({ ...prev, [id]: null }));
  }, [refresh]);

  const inboxCount  = threads.length;
  const draftCount  = drafts.length;

  const TABS = [
    { key: 'recommended', label: 'Recommended' },
    { key: 'inbox',       label: `Inbox${inboxCount > 0 ? ` (${inboxCount})` : ''}` },
    { key: 'drafts',      label: `Drafts${draftCount > 0 ? ` (${draftCount})` : ''}` },
    { key: 'advanced',    label: 'Advanced' },
  ];

  return (
    <div data-cy="comms-page">
      <CommsWizard open={wizardOpen} onClose={() => setWizardOpen(false)} onCreateDraft={createDraft} />

      <div className="page-header">
        <div>
          <h1 style={{ margin: 0 }}>Comms</h1>
          <p style={{ fontSize: 13, color: 'var(--text-dim, #888)', margin: '4px 0 0' }}>Drafts, messages, and approvals in one place.</p>
        </div>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'center' }}>
          <button data-cy="open-comms-wizard-btn" onClick={() => setWizardOpen(true)} style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent, #7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}>
            Create a draft for me
          </button>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading} style={{ fontSize: 13 }}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && (
        <div data-cy="comms-error" className="error-state" style={{ margin: '0 0 16px' }}>
          ⚠ {error}
          <button className="btn btn-ghost btn-sm" onClick={() => setError('')} style={{ marginLeft: 8 }}>Dismiss</button>
        </div>
      )}

      <div style={{ display: 'flex', borderBottom: '1px solid var(--border, #333)', marginBottom: 20 }}>
        {TABS.map(t => (
          <button key={t.key} data-cy={`comms-tab-v2-${t.key}`} onClick={() => setActiveTab(t.key)} style={{ padding: '10px 14px', background: 'transparent', border: 'none', borderBottom: `2px solid ${activeTab === t.key ? 'var(--accent, #7c3aed)' : 'transparent'}`, color: activeTab === t.key ? 'var(--text-primary, #fff)' : 'var(--text-dim, #888)', cursor: 'pointer', fontSize: 13, fontWeight: activeTab === t.key ? 700 : 400 }}>
            {t.label}
          </button>
        ))}
      </div>

      <div className="page-body">
        {activeTab === 'recommended' && <RecommendedCommsTab starters={COMMS_STARTERS} creatingId={creatingId} onUseTemplate={useTemplate} onOpenWizard={() => setWizardOpen(true)} />}
        {activeTab === 'inbox'       && <InboxCommsTab  threads={threads} loading={loading} onOpenWizard={() => setWizardOpen(true)} />}
        {activeTab === 'drafts'      && <DraftsCommsTab drafts={drafts}   actingById={actingById} onApprove={approveDraft} onSend={sendDraft} loading={loading} onOpenWizard={() => setWizardOpen(true)} onOpenDetail={id => setDrawerDraftId(id)} />}
        {activeTab === 'advanced'    && <AdvancedCommsTab topology={topology} events={events} loading={loading} error={error} onRefresh={refresh} />}
      </div>
      <CommsDraftDrawer
        open={!!drawerDraftId}
        draftId={drawerDraftId}
        onClose={() => setDrawerDraftId(null)}
        onApprove={approveDraft}
        onRequestChanges={async (id, note) => { await apiClient.post(`/api/comms/drafts/${id}/request-changes`, { note }); await refresh(); }}
        onSend={sendDraft}
      />
    </div>
  );
}
