import type {
  ModeRecord,
  ModeTask,
  ModeApproval,
  ModeResult,
  BusinessMode,
} from "./mode-types";

async function json<T>(res: Response): Promise<T> {
  if (!res.ok) {
    const body = await res.json().catch(() => ({})) as Record<string, unknown>;
    // Backend returns { success: false, error: { code, message } } or { error: "string" }
    const errField = body.error;
    const msg =
      typeof errField === "string" ? errField :
      typeof errField === "object" && errField !== null && typeof (errField as Record<string,unknown>).message === "string"
        ? (errField as Record<string,unknown>).message as string
        : `Request failed: ${res.status}`;
    throw new Error(msg);
  }
  return res.json();
}

// ── Records (clients / campaigns / programs) ────────────────────────────────

export async function createRecord(
  mode: BusinessMode,
  input: Partial<ModeRecord>
) {
  return json<{ record: ModeRecord }>(
    await fetch(`/api/modes/${mode}/records`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    })
  );
}

export async function getRecord(mode: BusinessMode, id: string) {
  return json<{ record: ModeRecord }>(
    await fetch(`/api/modes/${mode}/records/${id}`)
  );
}

export async function updateRecord(
  mode: BusinessMode,
  id: string,
  patch: Partial<ModeRecord>
) {
  return json<{ record: ModeRecord }>(
    await fetch(`/api/modes/${mode}/records/${id}`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ patch }),
    })
  );
}

export async function listRecords(mode: BusinessMode) {
  return json<{ records: ModeRecord[] }>(
    await fetch(`/api/modes/${mode}/records`)
  );
}

// ── Plan generation ──────────────────────────────────────────────────────────

export async function generateModePlan(
  mode: BusinessMode,
  record_id: string,
  selected_task_ids: string[]
) {
  return json<{ tasks: ModeTask[] }>(
    await fetch(`/api/modes/${mode}/generate-plan`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ record_id, selected_task_ids }),
    })
  );
}

// ── Tasks ───────────────────────────────────────────────────────────────────

export async function getModeTasks(mode: BusinessMode, record_id: string) {
  return json<{ tasks: ModeTask[] }>(
    await fetch(
      `/api/modes/${mode}/tasks?record_id=${encodeURIComponent(record_id)}`
    )
  );
}

export async function approveModeTask(mode: BusinessMode, task_id: string) {
  return json<{ ok: true }>(
    await fetch(`/api/modes/${mode}/tasks/${task_id}/approve`, {
      method: "POST",
    })
  );
}

export async function runModeTask(mode: BusinessMode, task_id: string) {
  return json<{ result: ModeResult }>(
    await fetch(`/api/modes/${mode}/tasks/${task_id}/run`, { method: "POST" })
  );
}

// ── Approvals ───────────────────────────────────────────────────────────────

export async function getModeApprovals(
  mode: BusinessMode,
  record_id: string
) {
  return json<{ approvals: ModeApproval[] }>(
    await fetch(
      `/api/modes/${mode}/approvals?record_id=${encodeURIComponent(record_id)}`
    )
  );
}

// ── Results ─────────────────────────────────────────────────────────────────

export async function getModeResults(mode: BusinessMode, record_id: string) {
  return json<{ results: ModeResult[] }>(
    await fetch(
      `/api/modes/${mode}/results?record_id=${encodeURIComponent(record_id)}`
    )
  );
}
