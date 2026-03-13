import { api } from '../../lib/api-server';
import SkillsClient from './SkillsClient';

function normalizeSkill(raw, i) {
  return {
    id: raw?.id ?? raw?.name ?? `skill-${i}`,
    name: raw?.name ?? raw?.id ?? 'Skill',
    description: raw?.description ?? '',
    version: raw?.version ?? '',
    enabled: raw?.enabled !== false,
    tags: Array.isArray(raw?.tags) ? raw.tags : [],
    entry_point: raw?.entry_point ?? '',
  };
}

export default async function SkillsPage() {
  let skills = [];
  try {
    const data = await api.get('/api/skills');
    const raw = Array.isArray(data) ? data : data?.skills ?? [];
    skills = raw.map(normalizeSkill);
  } catch {
    // error shown by client
  }
  return <SkillsClient initialSkills={skills} />;
}

