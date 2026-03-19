'use client';
import { useState, useCallback, useEffect } from 'react';
import { applyStarterDefaults, emptyWizardState } from './lib/creative-ui';
import CreativeWizard         from './CreativeWizard';
import RecommendedCreativeTab from './RecommendedCreativeTab';
import MyProjectsTab          from './MyProjectsTab';
import CreativeTemplatesTab   from './CreativeTemplatesTab';
import AdvancedCreativeTab    from './AdvancedCreativeTab';

const TABS = [
  { id: 'recommended', label: 'Recommended' },
  { id: 'projects',    label: 'My projects' },
  { id: 'templates',   label: 'Templates' },
  { id: 'advanced',    label: 'Advanced' },
];

export default function CreativeStudioPage({ initialProjects = [] }) {
  const [tab,           setTab]           = useState('recommended');
  const [showWizard,    setShowWizard]    = useState(false);
  const [wizardInitial, setWizardInitial] = useState(null);
  const [projects,      setProjects]      = useState(initialProjects);
  const [projLoading,   setProjLoading]   = useState(false);
  const [projError,     setProjError]     = useState('');

  const openBlankWizard = useCallback(() => {
    setWizardInitial(emptyWizardState());
    setShowWizard(true);
  }, []);

  const openTemplateWizard = useCallback((starter) => {
    setWizardInitial(applyStarterDefaults(starter));
    setShowWizard(true);
  }, []);

  const openProjectWizard = useCallback((project) => {
    // Re-open an existing project in the wizard at step 6 (results)
    setWizardInitial({
      ...emptyWizardState(),
      step: 6,
      creation_type: project.creation_type,
      goal: project.goal,
      name: project.name,
      topic: project.topic ?? '',
      ai_choices: project.ai_choices ?? {},
      _projectId: project.id,
    });
    setShowWizard(true);
  }, []);

  const closeWizard = useCallback(() => {
    setShowWizard(false);
    setWizardInitial(null);
  }, []);

  const handleWizardSave = useCallback((project) => {
    setProjects(prev => {
      const exists = prev.find(p => p.id === project.id);
      return exists ? prev.map(p => p.id === project.id ? project : p) : [project, ...prev];
    });
  }, []);

  const refreshProjects = useCallback(async () => {
    setProjLoading(true);
    setProjError('');
    try {
      const res = await fetch('/api/creative-projects');
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.error || 'Could not load projects.');
      setProjects(Array.isArray(data) ? data : data.items ?? []);
    } catch (e) {
      setProjError(e.message);
    }
    setProjLoading(false);
  }, []);

  useEffect(() => {
    if (tab === 'projects') refreshProjects();
  }, [tab, refreshProjects]);

  return (
    <div data-cy="creative-studio-page">
      {/* Header */}
      <div className="page-header">
        <div>
          <h1 style={{ margin: 0, fontSize: 22, fontWeight: 800 }}>🎨 Creative Studio</h1>
          <div style={{ fontSize: 13, color: 'var(--text-dim)', marginTop: 2 }}>
            Generate images, videos, and full creative campaigns — step by step
          </div>
        </div>
        <button
          data-cy="open-wizard-btn"
          className="btn"
          style={{ background: 'var(--accent)', color: '#fff', border: 'none', fontWeight: 700 }}
          onClick={openBlankWizard}
        >
          + New project
        </button>
      </div>

      {/* Tabs */}
      <div className="page-body">
        <div style={{ display: 'flex', gap: 0, borderBottom: '1px solid var(--border)', marginBottom: 24 }}>
          {TABS.map(t => (
            <button
              key={t.id}
              data-cy={`tab-${t.id}`}
              onClick={() => setTab(t.id)}
              style={{
                padding: '10px 18px',
                background: 'none',
                border: 'none',
                borderBottom: tab === t.id ? '2px solid var(--accent)' : '2px solid transparent',
                color: tab === t.id ? 'var(--accent)' : 'var(--text-dim)',
                fontWeight: tab === t.id ? 700 : 400,
                fontSize: 13,
                cursor: 'pointer',
                transition: 'color 0.15s',
              }}
            >
              {t.label}
            </button>
          ))}
        </div>

        {tab === 'recommended' && (
          <RecommendedCreativeTab
            onStartBlank={openBlankWizard}
            onStartTemplate={openTemplateWizard}
          />
        )}
        {tab === 'projects' && (
          <MyProjectsTab
            projects={projects}
            loading={projLoading}
            error={projError}
            onOpen={openProjectWizard}
            onRefresh={refreshProjects}
          />
        )}
        {tab === 'templates' && (
          <CreativeTemplatesTab onStartTemplate={openTemplateWizard} />
        )}
        {tab === 'advanced' && <AdvancedCreativeTab />}
      </div>

      {/* Wizard overlay / drawer */}
      {showWizard && (
        <div
          data-cy="creative-wizard-overlay"
          style={{
            position: 'fixed', inset: 0,
            background: 'rgba(0,0,0,0.45)',
            zIndex: 1000,
            display: 'flex',
            justifyContent: 'flex-end',
          }}
          onClick={e => { if (e.target === e.currentTarget) closeWizard(); }}
        >
          <div style={{
            width: 'min(820px, 100vw)',
            height: '100vh',
            background: 'var(--bg-elevated)',
            display: 'flex',
            flexDirection: 'column',
            overflow: 'hidden',
            boxShadow: 'var(--shadow-md)',
          }}>
            <CreativeWizard
              initialState={wizardInitial}
              onClose={closeWizard}
              onSave={handleWizardSave}
            />
          </div>
        </div>
      )}
    </div>
  );
}
