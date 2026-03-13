import { api } from '../../lib/api-server';
import ChannelsClient from './ChannelsClient';

function normalizeChannel(raw, i) {
  return {
    id: raw?.id ?? raw?.name ?? `ch-${i}`,
    name: raw?.name ?? raw?.id ?? 'Channel',
    type: raw?.type ?? raw?.adapter ?? '',
    adapter: raw?.adapter ?? raw?.type ?? '',
    status: raw?.status ?? raw?.state ?? 'unknown',
    description: raw?.description ?? '',
    agent_id: raw?.agent_id ?? '',
  };
}

export default async function ChannelsPage() {
  let channels = [];
  try {
    const data = await api.get('/api/channels');
    const raw = Array.isArray(data) ? data : data?.channels ?? [];
    channels = raw.map(normalizeChannel);
  } catch {
    // error shown by client
  }
  return <ChannelsClient initialChannels={channels} />;
}

