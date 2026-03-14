import { api } from '../../lib/api-server';
import WorkflowsClient from './WorkflowsClient';

export default async function WorkflowsPage() {
  let workflows = [];
  try {
    const data = await api.get('/api/workflows');
    workflows = Array.isArray(data) ? data : data?.workflows ?? [];
  } catch {
    // handled by client error state
  }
  return <WorkflowsClient initialWorkflows={workflows} />;
}
