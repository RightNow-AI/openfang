/**
 * Centralised Work API client.
 *
 * All pages that read or mutate WorkItems must import from here.
 * Never scatter fetch calls directly against /api/work across the app.
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
