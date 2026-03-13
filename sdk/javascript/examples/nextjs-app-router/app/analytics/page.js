import { api } from '../../lib/api-server';
import AnalyticsClient from './AnalyticsClient';

function normalizeUsage(u, b, ab) {
  const usage = u ?? {};
  const budget = b ?? {};
  const agentBudgets = Array.isArray(ab) ? ab : ab?.agents ?? [];
  return {
    totalTokens: usage.total_tokens ?? usage.tokens ?? 0,
    promptTokens: usage.prompt_tokens ?? usage.input_tokens ?? 0,
    completionTokens: usage.completion_tokens ?? usage.output_tokens ?? 0,
    totalCost: usage.total_cost ?? budget.spent ?? 0,
    totalRequests: usage.total_requests ?? usage.requests ?? 0,
    budgetLimit: budget.limit ?? budget.budget_limit ?? null,
    budgetSpent: budget.spent ?? budget.total_spent ?? usage.total_cost ?? 0,
    agentBudgets: agentBudgets.map(a => ({
      agentId: a?.agent_id ?? '',
      name: a?.name ?? a?.agent_name ?? a?.agent_id ?? 'Unknown',
      totalTokens: a?.total_tokens ?? a?.tokens ?? 0,
      totalCost: a?.total_cost ?? a?.cost ?? a?.spent ?? 0,
      totalRequests: a?.total_requests ?? a?.requests ?? 0,
    })),
  };
}

export default async function AnalyticsPage() {
  const [u, b, ab] = await api.gather(['/api/usage', '/api/budget', '/api/budget/agents']);
  const stats = normalizeUsage(u, b, ab);
  return <AnalyticsClient initialStats={stats} />;
}

