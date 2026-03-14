import { api } from '../../lib/api-server';
import DashboardClient from './DashboardClient';

export default async function DashboardPage() {
  let summary = null;
  let orchestratorStatus = null;
  let runningItems = [];
  let approvalItems = [];
  let failedItems = [];

  try {
    const [summaryData, statusData, runningData, approvalData, failedData] = await Promise.allSettled([
      api.get('/api/work/summary'),
      api.get('/api/orchestrator/status'),
      api.get('/api/work?status=running&limit=20'),
      api.get('/api/work?status=waiting_approval&limit=20'),
      api.get('/api/work?status=failed&limit=20'),
    ]);
    if (summaryData.status === 'fulfilled') summary = summaryData.value;
    if (statusData.status === 'fulfilled') orchestratorStatus = statusData.value;
    if (runningData.status === 'fulfilled') runningItems = runningData.value?.items ?? [];
    if (approvalData.status === 'fulfilled') approvalItems = approvalData.value?.items ?? [];
    if (failedData.status === 'fulfilled') failedItems = failedData.value?.items ?? [];
  } catch {
    // handled by client error state
  }

  return (
    <DashboardClient
      initialSummary={summary}
      initialOrchestratorStatus={orchestratorStatus}
      initialRunningItems={runningItems}
      initialApprovalItems={approvalItems}
      initialFailedItems={failedItems}
    />
  );
}
