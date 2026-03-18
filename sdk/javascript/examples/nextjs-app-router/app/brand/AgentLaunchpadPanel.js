'use client';

// ── Validation rules per task ─────────────────────────────────────────────

const TASKS = [
  {
    id: 'analyze_business',
    title: 'Analyze My Business',
    description: 'Turn your business inputs into a positioning summary, message opportunities, and gap analysis.',
    output: 'Brand Brief',
    icon: '🔍',
    tools: ['strategy planner', 'site summarizer'],
    validate: (p) => {
      const missing = [];
      if (!p.business_name) missing.push('Business name');
      if (!p.industry) missing.push('Industry');
      if (!p.primary_offer) missing.push('Primary offer');
      if (!p.main_goal_90_days) missing.push('90-day goal');
      if (!p.ideal_customer) missing.push('Ideal customer');
      return missing;
    },
  },
  {
    id: 'research_competitors',
    title: 'Research Competitors',
    description: 'Compare competitors, find positioning gaps, and identify angles this business should own.',
    output: 'Competitor Matrix',
    icon: '📊',
    tools: ['web research', 'review analysis'],
    validate: (p) => {
      const missing = [];
      if (!(p.top_competitors || []).some(c => c.name)) missing.push('At least 1 competitor');
      if (!p.industry) missing.push('Industry');
      if (!p.ideal_customer) missing.push('Ideal customer');
      if (!p.primary_offer) missing.push('Primary offer');
      return missing;
    },
  },
  {
    id: 'build_voice_guide',
    title: 'Build Voice Guide',
    description: 'Create a reusable voice system from your brand traits, examples, and tone preferences.',
    output: 'Voice Guide',
    icon: '🎙',
    tools: ['copy analyzer', 'site summarizer'],
    validate: (p) => {
      const missing = [];
      if (!(p.brand_traits || []).some(Boolean)) missing.push('Brand traits');
      if (!(p.traits_to_avoid || []).some(Boolean)) missing.push('Traits to avoid');
      if (!p.brand_promise) missing.push('Brand promise');
      if (!(p.liked_examples || []).some(e => e.value)) missing.push('At least 1 liked example');
      if (!(p.disliked_examples || []).some(e => e.value)) missing.push('At least 1 disliked example');
      return missing;
    },
  },
  {
    id: 'create_customer_avatar',
    title: 'Create Customer Avatar',
    description: 'Convert your audience notes into a specific buyer profile with hooks, triggers, and rebuttals.',
    output: 'Customer Avatar',
    icon: '👤',
    tools: ['strategy planner', 'review summarizer'],
    validate: (p) => {
      const missing = [];
      if (!p.ideal_customer) missing.push('Ideal customer');
      if (!(p.top_pain_points || []).some(Boolean)) missing.push('Pain points');
      if (!(p.desired_outcomes || []).some(Boolean)) missing.push('Desired outcomes');
      if (!(p.top_objections || []).some(Boolean)) missing.push('Objections');
      return missing;
    },
  },
  {
    id: 'draft_outreach_email_sequence',
    title: 'Draft Outreach Emails',
    description: 'Draft a 3-email outreach sequence aligned to your offer, audience, and brand voice.',
    output: 'Email Sequence',
    icon: '✉️',
    tools: ['copy generator', 'email operator'],
    validate: (p) => {
      const missing = [];
      if (!p.core_offer && !p.primary_offer) missing.push('Core offer');
      if (!p.ideal_customer) missing.push('Ideal customer');
      if (!p.primary_cta) missing.push('Primary CTA');
      if (!(p.top_objections || []).some(Boolean)) missing.push('Objections');
      return missing;
    },
  },
];

// ── Task card ─────────────────────────────────────────────────────────────

function TaskCard({ task, profile, runState, onRunTask }) {
  const missing = task.validate(profile);
  const canRun = missing.length === 0;
  const isRunning = runState?.task_type === task.id && runState?.status === 'running';

  return (
    <div style={{
      marginBottom: 10,
      borderRadius: 8,
      border: '1px solid var(--border)',
      background: 'var(--surface)',
      overflow: 'hidden',
    }}>
      {/* Task header */}
      <div style={{ padding: '10px 12px 8px' }}>
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
          <span style={{ fontSize: 18, lineHeight: 1, marginTop: 1 }}>{task.icon}</span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 700, fontSize: 13, color: 'var(--text)', marginBottom: 2 }}>
              {task.title}
            </div>
            <div style={{ fontSize: 11, color: 'var(--text-dim)', lineHeight: 1.5 }}>
              {task.description}
            </div>
          </div>
        </div>

        {/* Output type + tools */}
        <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
          <span style={{
            fontSize: 10, fontWeight: 600, padding: '2px 6px', borderRadius: 4,
            background: 'var(--accent-subtle)', color: 'var(--accent)',
          }}>→ {task.output}</span>
          {task.tools.map(t => (
            <span key={t} style={{
              fontSize: 10, padding: '2px 6px', borderRadius: 4,
              background: 'var(--surface3)', color: 'var(--text-muted)',
            }}>{t}</span>
          ))}
        </div>
      </div>

      {/* Missing fields */}
      {!canRun && (
        <div style={{
          borderTop: '1px solid var(--border)',
          padding: '7px 12px',
          background: 'var(--surface2)',
        }}>
          <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 4, fontWeight: 600, textTransform: 'uppercase', letterSpacing: '0.4px' }}>
            Needs:
          </div>
          {missing.map(f => (
            <div key={f} style={{ fontSize: 11, color: 'var(--warning)', display: 'flex', alignItems: 'center', gap: 4 }}>
              <span>◦</span>{f}
            </div>
          ))}
        </div>
      )}

      {/* Run button */}
      <div style={{ padding: '8px 12px', borderTop: '1px solid var(--border)' }}>
        <button
          onClick={() => onRunTask(task.id)}
          disabled={!canRun || isRunning}
          style={{
            width: '100%',
            padding: '7px 12px',
            borderRadius: 6,
            border: 'none',
            cursor: canRun && !isRunning ? 'pointer' : 'not-allowed',
            background: canRun ? 'var(--accent)' : 'var(--surface3)',
            color: canRun ? '#fff' : 'var(--text-muted)',
            fontSize: 12,
            fontWeight: 600,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            gap: 6,
            opacity: isRunning ? 0.7 : 1,
          }}
        >
          {isRunning ? (
            <>
              <RunSpinner />
              Running…
            </>
          ) : canRun ? (
            `Run — ${task.title}`
          ) : (
            `${missing.length} field${missing.length === 1 ? '' : 's'} needed`
          )}
        </button>
      </div>
    </div>
  );
}

function RunSpinner() {
  return (
    <span style={{
      display: 'inline-block',
      width: 11, height: 11,
      border: '2px solid rgba(255,255,255,0.3)',
      borderTopColor: '#fff',
      borderRadius: '50%',
      animation: 'spin 0.7s linear infinite',
    }} />
  );
}

// ── AgentLaunchpadPanel ───────────────────────────────────────────────────

export default function AgentLaunchpadPanel({ profile, runState, onRunTask }) {
  const isAnythingRunning = runState?.status === 'running';

  return (
    <div>
      {/* Panel header */}
      <div style={{
        padding: '12px 14px',
        borderBottom: '1px solid var(--border)',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
      }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 13, color: 'var(--text)' }}>
            Agent Launchpad
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 1 }}>
            Fill brand context → run a task
          </div>
        </div>
        {isAnythingRunning && (
          <span style={{
            fontSize: 10, padding: '2px 7px', borderRadius: 8,
            background: 'var(--warning-subtle)', color: 'var(--warning)',
            fontWeight: 600,
          }}>RUNNING</span>
        )}
      </div>

      {/* Task cards */}
      <div style={{ padding: 12 }}>
        {TASKS.map(task => (
          <TaskCard
            key={task.id}
            task={task}
            profile={profile}
            runState={runState}
            onRunTask={onRunTask}
          />
        ))}
      </div>

      {/* CSS for spinner animation */}
      <style>{`
        @keyframes spin { to { transform: rotate(360deg); } }
      `}</style>
    </div>
  );
}
