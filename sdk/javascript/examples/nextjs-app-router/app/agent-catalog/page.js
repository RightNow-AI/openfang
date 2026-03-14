import { api } from '../../lib/api-server';
import AgentCatalogClient from './AgentCatalogClient';

function normalizeEntry(raw) {
  return {
    catalog_id: raw?.catalog_id ?? '',
    agent_id: raw?.agent_id ?? '',
    name: raw?.name ?? raw?.agent_id ?? 'Unknown',
    description: raw?.description ?? '',
    division: raw?.division ?? '',
    source: raw?.source ?? 'native',
    source_label: raw?.source_label ?? '',
    tags: Array.isArray(raw?.tags) ? raw.tags : [],
    enabled: raw?.enabled ?? true,
    best_for: raw?.best_for ?? '',
    avoid_for: raw?.avoid_for ?? '',
    example: raw?.example ?? '',
    purpose: raw?.purpose ?? '',
  };
}

export default async function AgentCatalogPage() {
  let entries = [];
  try {
    const data = await api.get('/api/agents/catalog');
    const raw = Array.isArray(data?.agents) ? data.agents : Array.isArray(data) ? data : [];
    entries = raw.map(normalizeEntry);
  } catch {
    // handled by client error state
  }
  return <AgentCatalogClient initialEntries={entries} />;
}
