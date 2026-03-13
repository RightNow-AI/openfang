import { api } from '../../lib/api-server';
import LogsClient from './LogsClient';

function normalizeEntry(raw, i) {
  return {
    id: raw?.id ?? `entry-${i}`,
    timestamp: raw?.timestamp ?? raw?.created_at ?? '',
    action: raw?.action ?? raw?.event_type ?? '',
    subject: raw?.subject ?? raw?.resource ?? raw?.target ?? '',
    detail: raw?.detail ?? raw?.message ?? raw?.description ?? '',
    actor: raw?.actor ?? raw?.user ?? raw?.source ?? '',
  };
}

export default async function LogsPage() {
  let entries = [];
  try {
    const data = await api.get('/api/audit/recent?n=50');
    const raw = Array.isArray(data) ? data : data?.entries ?? data?.events ?? [];
    entries = raw.map(normalizeEntry);
  } catch {
    // error shown by client
  }
  return <LogsClient initialEntries={entries} />;
}

