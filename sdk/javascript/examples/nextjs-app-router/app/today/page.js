'use client';

import { useState, useEffect, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

function priorityBadgeClass(priority) {
  if (priority === 'urgent') return 'badge badge-error';
  if (priority === 'high') return 'badge badge-warn';
  if (priority === 'low') return 'badge badge-muted';
  return 'badge badge-info';
}

function priorityLabel(priority) {
  if (!priority) return 'Medium';
  return priority.charAt(0).toUpperCase() + priority.slice(1);
}

function TaskItem({ task }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="card" style={{ padding: '12px 14px', cursor: task.next_action ? 'pointer' : 'default' }} onClick={() => setExpanded(e => !e)}>
      <div className="flex items-center gap-2" style={{ flexWrap: 'wrap' }}>
        <span className={priorityBadgeClass(task.priority)}>{priorityLabel(task.priority)}</span>
        <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)', flex: 1 }}>{task.title || task.name || '—'}</span>
        {task.next_action && (
          <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>{expanded ? '▲' : '▼'}</span>
        )}
      </div>
      {expanded && task.next_action && (
        <div style={{ marginTop: 8, paddingTop: 8, borderTop: '1px solid var(--border-subtle)' }}>
          <div className="text-sm text-dim">{task.next_action}</div>
          {task.reason && <div className="text-xs text-muted" style={{ marginTop: 4 }}>{task.reason}</div>}
        </div>
      )}
    </div>
  );
}

function TaskSection({ title, tasks, emptyMsg }) {
  if (!tasks?.length) return (
    <div style={{ marginBottom: 20 }}>
      <h3 style={{ margin: '0 0 8px', fontSize: 14, fontWeight: 700, color: 'var(--text-dim)' }}>{title}</h3>
      <div className="text-dim text-sm" style={{ padding: '8px 0' }}>{emptyMsg}</div>
    </div>
  );
  return (
    <div style={{ marginBottom: 20 }}>
      <h3 style={{ margin: '0 0 8px', fontSize: 14, fontWeight: 700, color: 'var(--text)' }}>
        {title} <span className="text-muted" style={{ fontWeight: 400, fontSize: 12 }}>({tasks.length})</span>
      </h3>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {tasks.map((task, i) => <TaskItem key={task.id || i} task={task} />)}
      </div>
    </div>
  );
}

export default function TodayPage() {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [actionInFlight, setActionInFlight] = useState('');
  const [plan, setPlan] = useState(null);
  const [summary, setSummary] = useState(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/planner/today');
      setPlan(data.plan || data || null);
      setSummary(data.summary || null);
    } catch (e) {
      setError(e.message || 'Could not load today plan.');
    }
    setLoading(false);
  }, []);

  const rebuild = useCallback(async () => {
    setActionInFlight('rebuild');
    setError('');
    try {
      const data = await apiClient.post('/api/planner/today/rebuild', {});
      setPlan(data.plan || data || null);
      setSummary(data.summary || null);
    } catch (e) {
      setError(e.message || 'Could not rebuild today plan.');
    }
    setActionInFlight('');
  }, []);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      setLoading(true);
      setError('');
      try {
        const data = await apiClient.get('/api/planner/today');
        if (cancelled) {
          return;
        }
        setPlan(data.plan || data || null);
        setSummary(data.summary || null);
      } catch (e) {
        if (!cancelled) {
          setError(e.message || 'Could not load today plan.');
        }
      }
      if (!cancelled) {
        setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  const hasPlan = plan && (
    plan.daily_outcome ||
    (plan.must_do?.length) ||
    (plan.should_do?.length) ||
    (plan.could_do?.length) ||
    plan.focus_suggestion ||
    (plan.blockers?.length)
  );

  const focusTitle = summary?.focus_title || plan?.focus_suggestion?.title || 'No focus task selected';
  const focusAction = summary?.focus_action || plan?.focus_suggestion?.next_action || 'Rebuild the plan after clarifying Inbox items.';
  const blockerTitle = summary?.blocker_title || (plan?.blockers?.length ? 'Blocked work detected' : 'No blocker on deck');
  const blockerDetail = summary?.blocker_detail || (plan?.blockers?.length ? plan.blockers[0] : 'Blocked work is staying out of must-do.');

  return (
    <div data-cy="today-page">
      <div className="page-header">
        <h1>Today</h1>
        <div className="flex items-center gap-2">
          <button className="btn btn-ghost btn-sm" onClick={load} disabled={loading || !!actionInFlight}>Refresh</button>
          <button className="btn btn-primary btn-sm" onClick={rebuild} disabled={loading || !!actionInFlight}>
            {actionInFlight === 'rebuild' ? 'Rebuilding…' : 'Rebuild plan'}
          </button>
        </div>
      </div>

      <div className="page-body">
        {loading && <div className="loading-state"><div className="spinner" /><span>Loading today plan…</span></div>}

        {!loading && error && (
          <div className="error-state">
            ⚠ {error} <button className="btn btn-ghost btn-sm" style={{ marginLeft: 8 }} onClick={load}>Retry</button>
          </div>
        )}

        {!loading && !error && !hasPlan && (
          <div>
            <div className="info-card">
              <h4>No plan yet</h4>
              <p>Click <strong>Rebuild plan</strong> to generate today&apos;s plan using your connected agents and inbox.</p>
            </div>
          </div>
        )}

        {!loading && !error && hasPlan && (
          <div>
            {/* Focus + Blocker summary cards */}
            <div className="grid grid-2" style={{ marginBottom: 20 }}>
              <div className="card" style={{ borderLeft: '3px solid var(--accent)' }}>
                <div className="stat-label">Focus task</div>
                <div style={{ fontSize: 14, fontWeight: 600, margin: '6px 0 4px', color: 'var(--text)' }}>{focusTitle}</div>
                <div className="text-sm text-dim">{focusAction}</div>
              </div>
              <div className="card" style={{ borderLeft: `3px solid ${plan?.blockers?.length ? 'var(--error)' : 'var(--border)'}` }}>
                <div className="stat-label">Main blocker</div>
                <div style={{ fontSize: 14, fontWeight: 600, margin: '6px 0 4px', color: 'var(--text)' }}>{blockerTitle}</div>
                <div className="text-sm text-dim">{blockerDetail}</div>
              </div>
            </div>

            {/* Daily outcome */}
            {plan.daily_outcome && (
              <div className="card" style={{ marginBottom: 20, background: 'var(--accent-subtle)' }}>
                <div className="card-header" style={{ color: 'var(--accent)' }}>Daily outcome goal</div>
                <div style={{ fontSize: 14, marginTop: 4 }}>{plan.daily_outcome}</div>
              </div>
            )}

            {/* Task sections */}
            <TaskSection
              title="Must do"
              tasks={plan.must_do}
              emptyMsg="Nothing marked as must-do."
            />
            <TaskSection
              title="Should do"
              tasks={plan.should_do}
              emptyMsg="No should-do items."
            />
            <TaskSection
              title="Could do"
              tasks={plan.could_do}
              emptyMsg="No could-do items."
            />
          </div>
        )}
      </div>
    </div>
  );
}
