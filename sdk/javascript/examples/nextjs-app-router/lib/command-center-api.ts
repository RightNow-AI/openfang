import type {
  ApprovalItem,
  ClientProfile,
  GeneratePlanRequest,
  PlannedTask,
  RunResult,
} from "./command-center-types";

async function json<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error((body as { error?: string }).error || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function createClient(input: Partial<ClientProfile>) {
  return json<{ client: ClientProfile }>(
    await fetch("/api/clients", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    }),
  );
}

export async function getClient(id: string) {
  return json<{ client: ClientProfile }>(await fetch(`/api/clients/${id}`));
}

export async function updateClient(id: string, patch: Partial<ClientProfile>) {
  return json<{ client: ClientProfile }>(
    await fetch(`/api/clients/${id}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ patch }),
    }),
  );
}

export async function generatePlan(input: GeneratePlanRequest) {
  return json<{ tasks: PlannedTask[] }>(
    await fetch("/api/wizard/generate-plan", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    }),
  );
}

export async function getTasks(clientId: string) {
  return json<{ tasks: PlannedTask[] }>(
    await fetch(`/api/tasks?client_id=${encodeURIComponent(clientId)}`),
  );
}

export async function getApprovals(clientId: string) {
  return json<{ approvals: ApprovalItem[] }>(
    await fetch(`/api/cc-approvals?client_id=${encodeURIComponent(clientId)}`),
  );
}

export async function approveTask(taskId: string) {
  return json<{ ok: true }>(
    await fetch(`/api/tasks/${taskId}/approve`, { method: "POST" }),
  );
}

export async function runTask(taskId: string) {
  return json<{ result: RunResult }>(
    await fetch(`/api/tasks/${taskId}/run`, { method: "POST" }),
  );
}

export async function getResults(clientId: string) {
  return json<{ results: RunResult[] }>(
    await fetch(`/api/cc-results?client_id=${encodeURIComponent(clientId)}`),
  );
}
