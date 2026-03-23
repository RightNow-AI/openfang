'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';
import { track } from '../../lib/telemetry';
import SkillDrawer from './SkillDrawer';
import InstallModal from './InstallModal';

function normalizeSkill(raw, i) {
  return {
    name: String(raw?.name ?? raw?.id ?? `skill-${i}`),
    description: String(raw?.description ?? ''),
    runtime: String(raw?.runtime ?? raw?.language ?? raw?.type ?? ''),
    installed: raw?.installed !== false,
    enabled: raw?.enabled !== false,
    bundled: raw?.bundled ?? raw?.builtin ?? !raw?.custom ?? true,
    version: String(raw?.version ?? ''),
    tool_count: Number(raw?.tool_count ?? 0),
    used_by_count: Number(raw?.used_by_count ?? 0),
  };
}

export default function SkillsClient({ initialSkills }) {
  const [skills, setSkills] = useState((initialSkills ?? []).map(normalizeSkill));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [selectedSkillName, setSelectedSkillName] = useState(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [togglePending, setTogglePending] = useState({});  // skillName → bool
  const [modalOpen, setModalOpen] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/skills');
      const raw = Array.isArray(data) ? data : data?.skills ?? [];
      setSkills(raw.map(normalizeSkill));
    } catch (e) {
      setError(e.message || 'Could not load skills.');
    }
    setLoading(false);
  }, []);

  const openDrawer = useCallback((name) => {
    setSelectedSkillName(name);
    setDrawerOpen(true);
    track('skill_detail_opened', { skill: name });
  }, []);

  const closeDrawer = useCallback(() => {
    setDrawerOpen(false);
    // leave selectedSkillName so drawer can animate out without blanking
  }, []);

  const handleToggle = useCallback(async (skillName, currentEnabled, usedByCount) => {
    if (togglePending[skillName]) return;                          // one request at a time

    const newEnabled = !currentEnabled;

    // Optimistic update
    setSkills(prev => prev.map(s => s.name === skillName ? { ...s, enabled: newEnabled } : s));
    setTogglePending(prev => ({ ...prev, [skillName]: true }));

    track('skill_toggle_started', {
      skill: skillName,
      enabled_before: currentEnabled,
      enabled_after: newEnabled,
      used_by_count: usedByCount,
    });

    try {
      const res = await apiClient.put(
        `/api/skills/${encodeURIComponent(skillName)}/enabled`,
        { enabled: newEnabled },
      );
      const confirmed = typeof res?.enabled === 'boolean' ? res.enabled : newEnabled;

      // Apply confirmed state (may differ from optimistic if daemon normalizes)
      setSkills(prev => prev.map(s => s.name === skillName ? { ...s, enabled: confirmed } : s));

      track('skill_toggle_succeeded', {
        skill: skillName,
        enabled_before: currentEnabled,
        enabled_after: confirmed,
        used_by_count: usedByCount,
      });
    } catch (e) {
      // Rollback
      setSkills(prev => prev.map(s => s.name === skillName ? { ...s, enabled: currentEnabled } : s));
      setError(e.message || 'Could not update skill.');

      track('skill_toggle_failed', {
        skill: skillName,
        enabled_before: currentEnabled,
        enabled_after: newEnabled,
        used_by_count: usedByCount,
        error_message: e.message,
      });
    }

    setTogglePending(prev => ({ ...prev, [skillName]: false }));
  }, [togglePending]);

  return (
    <div data-cy="skills-page">
      {/* Detail drawer */}
      {drawerOpen && selectedSkillName && (
        <SkillDrawer
          key={selectedSkillName}
          skillName={selectedSkillName}
          onClose={closeDrawer}
          onToggle={handleToggle}
          togglePending={!!togglePending[selectedSkillName]}
        />
      )}

      <div className="page-header">
        <h1>Skills</h1>
        <div className="flex items-center gap-2">
          <span className="text-dim text-sm">{skills.length} installed</span>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Loading…' : 'Refresh'}
          </button>
          <button
            data-cy="open-install-modal"
            className="btn btn-primary btn-sm"
            onClick={() => setModalOpen(true)}
          >
            + Install Skill
          </button>
        </div>
      </div>

      <div className="page-body">
        {error && (
          <div data-cy="skills-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={() => setError('')}>Dismiss</button>
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}

        {skills.length === 0 && !error && !loading && (
          <div data-cy="skills-empty" className="empty-state">
            No skills installed. Use the <strong>+ Install Skill</strong> button to browse and install from the registry.
          </div>
        )}

        <div data-cy="skills-grid" className="grid grid-auto" style={{ gap: 14 }}>
          {skills.map(s => (
            <div key={s.name} data-cy="skill-card" className="card" style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              {/* Card header */}
              <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div style={{ fontWeight: 700, fontSize: 14 }}>{s.name}</div>
                  {s.description && (
                    <div
                      className="text-sm text-dim"
                      style={{ marginTop: 3, overflow: 'hidden', display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical' }}
                    >
                      {s.description}
                    </div>
                  )}
                </div>
              </div>

              {/* Badges */}
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
                {s.runtime && (
                  <span className="badge badge-info" style={{ fontSize: 10 }}>{s.runtime}</span>
                )}
                <span
                  className={`badge ${s.bundled ? 'badge-success' : 'badge-warning'}`}
                  style={{ fontSize: 10 }}
                >
                  {s.bundled ? 'Bundled' : 'Custom'}
                </span>
                {s.version && (
                  <span className="badge badge-muted" style={{ fontSize: 10 }}>v{s.version}</span>
                )}
                {s.tool_count > 0 && (
                  <span className="badge badge-dim" style={{ fontSize: 10 }}>{s.tool_count} tool{s.tool_count !== 1 ? 's' : ''}</span>
                )}
              </div>

              {/* Usage + enable state row */}
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
                <span
                  data-cy="skill-usage-count"
                  style={{ fontSize: 12, color: 'var(--text-dim)' }}
                >
                  {s.used_by_count > 0
                    ? `Referenced by ${s.used_by_count} agent${s.used_by_count !== 1 ? 's' : ''}`
                    : 'Not referenced by any agent'}
                </span>
                <button
                  data-cy="skill-toggle"
                  className="btn btn-ghost btn-sm"
                  onClick={() => handleToggle(s.name, s.enabled, s.used_by_count)}
                  disabled={!!togglePending[s.name]}
                  style={{ fontSize: 11, minWidth: 80 }}
                >
                  {togglePending[s.name]
                    ? <span style={{ display: 'flex', alignItems: 'center', gap: 5 }}>
                        <div className="spinner" style={{ width: 10, height: 10 }} />
                      </span>
                    : s.enabled
                      ? <span style={{ color: 'var(--success)' }}>● Enabled</span>
                      : <span style={{ color: 'var(--text-dim)' }}>○ Disabled</span>
                  }
                </button>
              </div>

              {/* Details button */}
              <button
                data-cy="skill-details-btn"
                className="btn btn-primary btn-sm"
                onClick={() => openDrawer(s.name)}
                style={{ width: '100%' }}
              >
                Details
              </button>
            </div>
          ))}
        </div>
      </div>
      {modalOpen && (
        <InstallModal
          open={modalOpen}
          onClose={() => setModalOpen(false)}
          onInstallSuccess={() => {
            setModalOpen(false);
            refresh();
          }}
        />
      )}
    </div>
  );
}
