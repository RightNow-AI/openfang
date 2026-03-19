'use client';

function getMissingFields(project) {
  const required = [['topic', 'Topic'], ['goal', 'Goal'], ['project_type', 'Project type']];
  return required.filter(([k]) => !project?.[k]).map(([, label]) => label);
}

function ContextRow({ label, value }) {
  return (
    <div style={{ marginBottom: 10 }}>
      <div style={{ fontSize: 10, color: 'var(--text-dim,#888)', textTransform: 'uppercase', letterSpacing: 0.5, marginBottom: 2 }}>{label}</div>
      <div style={{ fontSize: 12, color: value ? 'var(--text-primary,#f1f1f1)' : 'var(--text-dim,#888)', fontStyle: value ? 'normal' : 'italic', lineHeight: 1.4 }}>{value || 'Not set'}</div>
    </div>
  );
}

export default function CreativeContextSidebar({ project, plan, onEditBrief, onOpenAiChoices }) {
  const missing = getMissingFields(project);

  return (
    <div
      data-cy="context-sidebar"
      style={{ width: 210, borderRight: '1px solid var(--border,#333)', padding: '20px 14px', overflowY: 'auto', flexShrink: 0, fontSize: 13 }}
    >
      <div style={{ fontWeight: 700, marginBottom: 12, color: 'var(--text-dim,#888)', fontSize: 10, textTransform: 'uppercase', letterSpacing: 1 }}>Project brief</div>

      <ContextRow label="Type"     value={project?.project_type?.replace(/_/g, ' + ')} />
      <ContextRow label="Goal"     value={project?.goal?.replace(/_/g, ' ')} />
      <ContextRow label="Topic"    value={project?.topic} />
      <ContextRow label="Audience" value={project?.audience} />
      <ContextRow label="Platform" value={project?.platform} />

      {missing.length > 0 && (
        <div style={{ marginTop: 10, padding: '8px 10px', borderRadius: 7, background: 'rgba(249,115,22,.08)', border: '1px solid rgba(249,115,22,.25)' }}>
          <div style={{ fontWeight: 700, fontSize: 10, color: '#f97316', marginBottom: 5, textTransform: 'uppercase', letterSpacing: 0.5 }}>Missing</div>
          {missing.map(f => <div key={f} style={{ fontSize: 11, color: '#f97316' }}>· {f}</div>)}
        </div>
      )}

      <button
        onClick={onEditBrief}
        style={{ marginTop: 12, width: '100%', padding: '6px', borderRadius: 7, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-dim,#888)', cursor: 'pointer', fontSize: 12 }}
      >
        Edit brief
      </button>

      {plan && (
        <div style={{ marginTop: 20, borderTop: '1px solid var(--border,#333)', paddingTop: 14 }}>
          <div style={{ fontWeight: 700, marginBottom: 8, color: 'var(--text-dim,#888)', fontSize: 10, textTransform: 'uppercase', letterSpacing: 1 }}>Plan</div>
          <div style={{ fontSize: 12, lineHeight: 1.55, color: 'var(--text-secondary,#bbb)' }}>{plan.thesis}</div>
          {plan.approval_points?.length > 0 && (
            <div style={{ marginTop: 8 }}>
              {plan.approval_points.map(pt => (
                <div key={pt} style={{ fontSize: 10, padding: '2px 7px', borderRadius: 5, background: 'rgba(249,115,22,.08)', color: '#f97316', marginBottom: 4, display: 'inline-block', marginRight: 4 }}>
                  ⏸ {pt.replace(/before_/g, '').replace(/_/g, ' ')}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {project?.selected_ai && Object.values(project.selected_ai).some(Boolean) && (
        <div style={{ marginTop: 20, borderTop: '1px solid var(--border,#333)', paddingTop: 14 }}>
          <div style={{ fontWeight: 700, marginBottom: 8, color: 'var(--text-dim,#888)', fontSize: 10, textTransform: 'uppercase', letterSpacing: 1 }}>AI choices</div>
          {Object.entries(project.selected_ai).filter(([, v]) => v).map(([k, v]) => (
            <div key={k} style={{ fontSize: 11, marginBottom: 5 }}>
              <span style={{ color: 'var(--text-dim,#888)' }}>{k.replace('_model', '')}</span>{' '}
              <span style={{ color: 'var(--text-primary,#f1f1f1)' }}>{v}</span>
            </div>
          ))}
          <button onClick={onOpenAiChoices} style={{ marginTop: 6, fontSize: 11, padding: '4px 8px', borderRadius: 5, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-dim,#888)', cursor: 'pointer' }}>Change</button>
        </div>
      )}
    </div>
  );
}
