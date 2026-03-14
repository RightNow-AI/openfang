import { api } from '../../lib/api-server';
import InboxClient from './InboxClient';

export default async function InboxPage() {
  let items = [];
  try {
    const data = await api.get('/api/work?status=pending');
    items = Array.isArray(data?.items) ? data.items : [];
  } catch {
    // handled by client error state
  }
  return <InboxClient initialItems={items} />;
}
