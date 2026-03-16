/**
 * Centralised Work API client.
 *
 * All pages that read or mutate WorkItems must import from here.
 * Never scatter fetch calls directly against /api/work across the app.
 */

/**
 * @typedef {Object} AdapterSelection
 * @property {string} chosen
 * @property {string[]} rejected
 * @property {string} rationale
 */

/**
 * @typedef {Object} ActionResult
 * @property {string|null} output
 * @property {number} tokens_in
 * @property {number} tokens_out
 * @property {number|null} cost_usd
 * @property {number} iterations
 * @property {boolean} action_succeeded
 * @property {string|null} error
 */

/**
 * @typedef {Object} VerificationResult
 * @property {boolean} passed
 * @property {Object} method_used
 * @property {string} evidence
 * @property {string} verified_at
 */

/**
 * ExecutionReport is returned by POST /api/work/:id/run.
 * It is NOT a WorkItem — it does not have `.id` or `.status` at the top level.
 * Detect by checking `result?.work_item_id !== undefined`.
 *
 * @typedef {Object} ExecutionReport
 * @property {string} work_item_id
 * @property {string} execution_path  - "fast_path" | "planned_swarm" | "review_swarm"
 * @property {AdapterSelection} adapter_selection
 * @property {Object} objective
 * @property {ActionResult} action_result
 * @property {VerificationResult} verification
 * @property {string} status  - "completed" | "failed" | "blocked" | "waiting_approval" | "retry_scheduled" | "delegated_to_subagent"
 * @property {Object|null} block_reason
 * @property {string} result_summary
 * @property {string[]} artifact_refs
 * @property {number} retry_count
 * @property {boolean} retry_scheduled
 * @property {string|null} delegated_to
 * @property {number|null} cost_usd
 * @property {string[]} warnings
 * @property {string[]} events_emitted
 * @property {string} started_at
 * @property {string} finished_at
 */

import { apiClient } from './api-client';

const BASE = '/api/work';

function qs(params) {
  const p = Object.fromEntries(
    Object.entries(params).filter(([, v]) => v != null && v !== ''),
  );
  const s = new URLSearchParams(p).toString();
  return s ? `?${s}` : '';
}

export const workApi = {
  /** GET /api/work */
  getWork: (params = {}) => apiClient.get(`${BASE}${qs(params)}`),

  /** GET /api/work/:id */
  getWorkById: (id) => apiClient.get(`${BASE}/${id}`),

  /** POST /api/work */
  createWork: (body) => apiClient.post(BASE, body),

  /** POST /api/work/:id/run */
  runWork: (id) => apiClient.post(`${BASE}/${id}/run`, {}),

  /** POST /api/work/:id/approve */
  approveWork: (id, note = '') => apiClient.post(`${BASE}/${id}/approve`, { note }),

  /** POST /api/work/:id/reject */
  rejectWork: (id, reason = '') => apiClient.post(`${BASE}/${id}/reject`, { reason }),

  /** POST /api/work/:id/cancel */
  cancelWork: (id) => apiClient.post(`${BASE}/${id}/cancel`, {}),

  /** POST /api/work/:id/retry */
  retryWork: (id) => apiClient.post(`${BASE}/${id}/retry`, {}),

  /** GET /api/work/:id/events */
  getWorkEvents: (id) => apiClient.get(`${BASE}/${id}/events`),

  /** GET /api/work/summary */
  getSummary: () => apiClient.get(`${BASE}/summary`),

  /** POST /api/work/:id/delegate */
  delegateWork: (id, body) => apiClient.post(`${BASE}/${id}/delegate`, body),

  /** GET /api/orchestrator/status */
  getOrchestratorStatus: () => apiClient.get('/api/orchestrator/status'),

  /** GET /api/orchestrator/runs */
  getOrchestratorRuns: () => apiClient.get('/api/orchestrator/runs'),

  /** POST /api/orchestrator/heartbeat */
  triggerHeartbeat: () => apiClient.post('/api/orchestrator/heartbeat', {}),
};
