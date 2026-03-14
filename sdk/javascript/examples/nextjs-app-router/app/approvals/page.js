import { api } from '../../lib/api-server';
import ApprovalsClient from './ApprovalsClient';

export default async function ApprovalsPage() {
  let approvals = [];
  try {
    const data = await api.get('/api/work?approval_status=pending');
    approvals = Array.isArray(data?.items) ? data.items : [];
  } catch {
    // handled by client error state
  }
  return <ApprovalsClient initialApprovals={approvals} />;
}
