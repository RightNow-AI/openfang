import { api } from '../../lib/api-server';
import { normalizeSkillCard } from '../../lib/skills';
import SkillsPageV2 from './SkillsPageV2';

export default async function SkillsPage() {
  let skills = [];
  try {
    const data = await api.get('/api/skills');
    const raw = Array.isArray(data) ? data : data?.skills ?? [];
    skills = raw.map(normalizeSkillCard);
  } catch {
    // error shown by client
  }
  return <SkillsPageV2 initialSkills={skills} />;
}

