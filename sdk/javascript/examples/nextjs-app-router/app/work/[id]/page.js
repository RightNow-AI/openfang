import { api } from '../../../lib/api-server';
import WorkDetailClient from './WorkDetailClient';

export default async function WorkDetailPage({ params }) {
  const { id } = await params;
  let item = null;
  let events = [];
  let children = [];

  try {
    item = await api.get(`/api/work/${id}`);
  } catch {
    // handled by client error state
  }

  try {
    const data = await api.get(`/api/work/${id}/events`);
    events = Array.isArray(data?.events) ? data.events : [];
  } catch {
    // non-fatal
  }

  try {
    const data = await api.get(`/api/work?parent_id=${id}&limit=50`);
    children = Array.isArray(data?.items) ? data.items : [];
  } catch {
    // non-fatal
  }

  return (
    <WorkDetailClient
      initialItem={item}
      initialEvents={events}
      initialChildren={children}
      id={id}
    />
  );
}
