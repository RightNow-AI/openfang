'use client';
import { useState, useCallback } from 'react';
import CreativeDirectorPanel from './CreativeDirectorPanel';
import CreativeVisualBoard from './CreativeVisualBoard';
import CreativePlanPanel from './CreativePlanPanel';
import CreativeApprovalChecklist from './CreativeApprovalChecklist';
import CreativeTaskLauncher from './CreativeTaskLauncher';
import CreativeResultsPanel from './CreativeResultsPanel';
import CreativeContextSidebar from './CreativeContextSidebar';
import CreativeProjectDrawer from './CreativeProjectDrawer';

const TABS = [
  { id: 'director',    label: 'Director' },
  { id: 'references',  label: 'References' },
  { id: 'plan',        label: 'Plan' },
  { id: 'generations', label: 'Generations' },
  { id: 'approvals',   label: 'Approvals' },
  { id: 'results',     label: 'Results' },
];

export default function CreativeProjectPage({ initialProject, initialMessages = [], initialReferences = [], initialPlan = null, initialAssets = [] }) {
  const [project,    setProject]    = useState(initialProject);
  const [messages,   setMessages]   = useState(initialMessages);
  const [references, setReferences] = useState(initialReferences);
  const [plan,       setPlan]       = useState(initialPlan);
  const [assets,     setAssets]     = useState(initialAssets);

  const [activeTab,   setActiveTab]   = useState('director');
  const [selectedRefIds, setSelectedRefIds] = useState([]);
  const [isThinking,  setIsThinking]  = useState(false);
  const [runningTask, setRunningTask] = useState(null);
  const [drawerOpen,  setDrawerOpen]  = useState(false);

  /* ── Director messages ── */
  const sendMessage = useCallback(async (input) => {
    const userMsg = { id: `u-${Date.now()}`, role: 'user', text: input.text, created_at: new Date().toISOString() };
    setMessages(prev => [...prev, userMsg]);
    setIsThinking(true);
    try {
      const res = await fetch(`/api/creative-projects/${project.id}/director/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(input),
      });
      const data = await res.json();
      if (data?.message) setMessages(prev => [...prev, data.message]);
    } catch {
      setMessages(prev => [...prev, { id: `err-${Date.now()}`, role: 'system', text: 'Failed to reach director. Please try again.', created_at: new Date().toISOString() }]);
    }
    setIsThinking(false);
  }, [project.id]);

  /* ── Quick actions / task runner ── */
  const runAction = useCallback(async (taskType) => {
    setRunningTask(taskType);
    try {
      const res = await fetch(`/api/creative-projects/${project.id}/tasks/launch`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ task_type: taskType }),
      });
      const data = await res.json();
      if (data?.message) setMessages(prev => [...prev, data.message]);
      if (data?.plan)    setPlan(data.plan);
      if (Array.isArray(data?.assets)) setAssets(prev => [...prev, ...data.assets]);
    } catch {}
    setRunningTask(null);
  }, [project.id]);

  /* ── Plan approval ── */
  const approvePlan = useCallback(async () => {
    try {
      await fetch(`/api/creative-projects/${project.id}/director/approve`, { method: 'POST' });
      setProject(prev => ({ ...prev, status: 'approved' }));
      if (plan) setPlan(prev => ({ ...prev, status: 'approved' }));
    } catch {}
  }, [project.id, plan]);

  /* ── Reference mgmt ── */
  const uploadReference = useCallback(async (files) => {
    const form = new FormData();
    files.forEach(f => form.append('file', f));
    try {
      const res = await fetch(`/api/creative-projects/${project.id}/references`, { method: 'POST', body: form });
      const data = await res.json();
      if (Array.isArray(data?.references)) setReferences(prev => [...prev, ...data.references]);
    } catch {}
  }, [project.id]);

  const addReferenceUrl = useCallback(async (url) => {
    try {
      const res = await fetch(`/api/creative-projects/${project.id}/references`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url }),
      });
      const data = await res.json();
      if (data?.reference) setReferences(prev => [...prev, data.reference]);
    } catch {}
  }, [project.id]);

  const toggleReference = useCallback((id) => {
    setSelectedRefIds(prev => prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id]);
  }, []);

  const askDirectorAboutSelection = useCallback(async () => {
    const ids = selectedRefIds;
    if (!ids.length) return;
    await sendMessage({ text: 'Please analyse these references and tell me how they relate to the brief.', imageIds: ids, referenceUrls: [] });
    setSelectedRefIds([]);
    setActiveTab('director');
  }, [selectedRefIds, sendMessage]);

  /* ── Asset approvals ── */
  const approveAsset = useCallback(async (assetId) => {
    try {
      await fetch(`/api/creative-projects/${project.id}/approvals`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ asset_id: assetId }),
      });
      setAssets(prev => prev.map(a => a.id === assetId ? { ...a, status: 'approved' } : a));
    } catch {}
  }, [project.id]);

  /* ── Project save ── */
  const saveProject = useCallback(async (patch) => {
    try {
      const res = await fetch(`/api/creative-projects/${project.id}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(patch),
      });
      if (res.ok) setProject(prev => ({ ...prev, ...patch }));
    } catch {}
  }, [project.id]);

  const approvalTypes = project.approval_points ?? [];

  return (
    <div data-cy="creative-project-page" style={{ display: 'flex', flexDirection: 'column', height: '100%', minHeight: 0 }}>
      {/* Header */}
      <div style={{ padding: '14px 24px', borderBottom: '1px solid var(--border,#333)', display: 'flex', alignItems: 'center', gap: 14, flexShrink: 0, flexWrap: 'wrap' }}>
        <a href="/creative-studio" style={{ color: 'var(--text-dim,#888)', textDecoration: 'none', fontSize: 13, flexShrink: 0 }}>← Creative Studio</a>
        <div style={{ flex: 1, minWidth: 0, display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ fontWeight: 700, fontSize: 16, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{project.name || 'Untitled project'}</span>
          <StatusChip status={project.status} />
        </div>
        <button onClick={() => setDrawerOpen(true)} style={{ padding: '6px 14px', borderRadius: 7, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-dim,#888)', cursor: 'pointer', fontSize: 13, flexShrink: 0 }}>Edit brief</button>
      </div>

      {/* Tab bar */}
      <div style={{ display: 'flex', borderBottom: '1px solid var(--border,#333)', padding: '0 24px', flexShrink: 0, overflowX: 'auto' }}>
        {TABS.map(t => (
          <button
            key={t.id}
            data-cy={`project-tab-${t.id}`}
            onClick={() => setActiveTab(t.id)}
            style={{ padding: '10px 14px', background: 'transparent', border: 'none', borderBottom: `2px solid ${activeTab === t.id ? 'var(--accent,#7c3aed)' : 'transparent'}`, color: activeTab === t.id ? 'var(--text-primary,#fff)' : 'var(--text-dim,#888)', cursor: 'pointer', fontSize: 13, fontWeight: activeTab === t.id ? 700 : 400, whiteSpace: 'nowrap' }}
          >
            {t.label}
          </button>
        ))}
      </div>

      {/* Body: sidebar | main | right-rail */}
      <div style={{ flex: 1, display: 'flex', minHeight: 0, overflow: 'hidden' }}>
        {/* Left sidebar: brief summary */}
        <CreativeContextSidebar
          project={project}
          plan={plan}
          onEditBrief={() => setDrawerOpen(true)}
          onOpenAiChoices={() => setActiveTab('plan')}
        />

        {/* Main content */}
        <div style={{ flex: 1, minWidth: 0, overflowY: 'auto', padding: activeTab === 'director' ? '16px 20px' : '20px 24px' }}>
          {activeTab === 'director' && (
            <CreativeDirectorPanel
              projectId={project.id}
              messages={messages}
              isThinking={isThinking}
              onSendMessage={sendMessage}
              onApprovePlan={approvePlan}
              onRunNextAction={runAction}
            />
          )}
          {activeTab === 'references' && (
            <CreativeVisualBoard
              references={references}
              selectedReferenceIds={selectedRefIds}
              onToggleReference={toggleReference}
              onAskDirectorAboutSelection={askDirectorAboutSelection}
              onUploadReference={uploadReference}
              onAddReferenceUrl={addReferenceUrl}
            />
          )}
          {activeTab === 'plan' && (
            <CreativePlanPanel
              plan={plan}
              onApprove={approvePlan}
              onRevise={note => sendMessage({ text: `Please revise the plan: ${note}`, imageIds: [], referenceUrls: [] })}
              onLaunchTask={runAction}
            />
          )}
          {activeTab === 'results' && (
            <CreativeResultsPanel
              assets={assets}
              onOpenAsset={() => {}}
              onApproveAsset={approveAsset}
              onExportAsset={() => {}}
              onArchiveAsset={() => {}}
            />
          )}
          {activeTab === 'generations' && (
            <div style={{ padding: '48px 0', textAlign: 'center', color: 'var(--text-dim,#888)', fontSize: 14 }}>
              <div style={{ fontSize: 32, marginBottom: 10 }}>🖼</div>
              No generations yet. Approve the plan and launch a generation task.
            </div>
          )}
          {activeTab === 'approvals' && (
            <div style={{ padding: '48px 0', textAlign: 'center', color: 'var(--text-dim,#888)', fontSize: 14 }}>
              <div style={{ fontSize: 32, marginBottom: 10 }}>✅</div>
              No approvals pending.
            </div>
          )}
        </div>

        {/* Right rail */}
        <div style={{ width: 220, borderLeft: '1px solid var(--border,#333)', padding: '20px 14px', overflowY: 'auto', flexShrink: 0 }}>
          <CreativeApprovalChecklist
            approvalTypes={approvalTypes}
            approved={[]}
            onApproveType={async () => {}}
          />
          <div style={{ marginTop: approvalTypes.length ? 24 : 0 }}>
            <CreativeTaskLauncher
              projectStatus={project.status}
              availableTasks={['generate_moodboard_directions', 'generate_prompt_pack', 'generate_script_strategy', 'generate_image_drafts', 'generate_video_plan']}
              runningTask={runningTask}
              onLaunchTask={runAction}
            />
          </div>
        </div>
      </div>

      {/* Brief edit drawer */}
      <CreativeProjectDrawer
        open={drawerOpen}
        project={project}
        onClose={() => setDrawerOpen(false)}
        onSave={saveProject}
      />
    </div>
  );
}

function StatusChip({ status }) {
  const MAP = {
    draft:                 '#6b7280',
    direction_in_progress: '#f59e0b',
    plan_ready:            '#3b82f6',
    waiting_approval:      '#f97316',
    approved:              '#10b981',
    running:               '#8b5cf6',
    completed:             '#10b981',
    failed:                '#ef4444',
  };
  const c = MAP[status] ?? '#6b7280';
  return (
    <span style={{ fontSize: 10, padding: '2px 8px', borderRadius: 999, background: `${c}22`, color: c, border: `1px solid ${c}44`, flexShrink: 0, textTransform: 'capitalize' }}>
      {(status ?? 'draft').replace(/_/g, ' ')}
    </span>
  );
}
