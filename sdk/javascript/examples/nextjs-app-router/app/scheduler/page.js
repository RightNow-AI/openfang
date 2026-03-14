import { api } from '../../lib/api-server';
import SchedulerClient from './SchedulerClient';

export default async function SchedulerPage() {
  let items = [];
  try {
    const data = await api.get('/api/work?scheduled=true');
    items = Array.isArray(data?.items) ? data.items : [];
  } catch {
    // handled by client error state
  }
  return <SchedulerClient initialItems={items} />;
}
