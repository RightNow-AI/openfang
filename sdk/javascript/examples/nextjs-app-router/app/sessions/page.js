import { api } from '../../lib/api-server';
import SessionsClient from './SessionsClient';

function normalizeAgent(raw) {
  return {
    id: raw?.id ?? '',
    name: raw?.name ?? raw?.id ?? 'Unnamed agent',
    model: raw?.model ?? '',
    provider: raw?.provider ?? '',
    status: raw?.status ?? raw?.loop_state ?? 'unknown',
    memory_backend: raw?.memory_backend ?? raw?.memory?.backend ?? 'default',
  };
}

export default async function SessionsPage() {
  let agents = [];
  try {
    const data = await api.get('/api/agents');
    const raw = Array.isArray(data) ? data : data?.agents ?? [];
    agents = raw.map(normalizeAgent);
  } catch {
    // handled by client error state
  }
  return <SessionsClient initialAgents={agents} />;
}


