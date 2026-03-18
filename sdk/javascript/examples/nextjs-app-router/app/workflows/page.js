import { api } from '../../lib/api-server';
import WorkflowsPageV2 from './WorkflowsPageV2';

export default async function WorkflowsPage() {
  let workflows = [];
  try {
    const data = await api.get('/api/workflows');
    workflows = Array.isArray(data) ? data : data?.workflows ?? [];
  } catch {
    // handled by client error state
  }
  return <WorkflowsPageV2 initialWorkflows={workflows} />;
}
