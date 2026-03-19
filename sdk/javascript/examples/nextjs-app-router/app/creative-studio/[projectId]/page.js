import { api } from '../../../lib/api-server';
import CreativeProjectPage from '../CreativeProjectPage';

export default async function CreativeProjectRoute({ params }) {
  const { projectId } = await params;

  let project   = null;
  let messages  = [];
  let references = [];
  let plan      = null;
  let assets    = [];

  try {
    project = await api.get(`/api/creative-projects/${projectId}`);
  } catch {
    // handled below
  }

  if (project) {
    const [messagesRes, referencesRes, planRes, assetsRes] = await Promise.allSettled([
      api.get(`/api/creative-projects/${projectId}/director/messages`),
      api.get(`/api/creative-projects/${projectId}/references`),
      api.get(`/api/creative-projects/${projectId}/director/plan`),
      api.get(`/api/creative-projects/${projectId}/results`),
    ]);
    if (messagesRes.status  === 'fulfilled') messages   = Array.isArray(messagesRes.value?.messages)   ? messagesRes.value.messages   : [];
    if (referencesRes.status === 'fulfilled') references = Array.isArray(referencesRes.value?.references) ? referencesRes.value.references : [];
    if (planRes.status      === 'fulfilled') plan       = planRes.value?.plan ?? null;
    if (assetsRes.status    === 'fulfilled') assets     = Array.isArray(assetsRes.value?.assets)       ? assetsRes.value.assets       : [];
  }

  if (!project) {
    return (
      <div style={{ padding: '64px 24px', textAlign: 'center', color: 'var(--text-dim,#888)' }}>
        <div style={{ fontSize: 36, marginBottom: 12 }}>404</div>
        <div>Project not found. <a href="/creative-studio" style={{ color: 'var(--accent,#7c3aed)' }}>← Back to Creative Studio</a></div>
      </div>
    );
  }

  return (
    <CreativeProjectPage
      initialProject={project}
      initialMessages={messages}
      initialReferences={references}
      initialPlan={plan}
      initialAssets={assets}
    />
  );
}
