import { getFallbackStudioIndex, getFallbackStudioWorkspace } from './studio-fixtures';
import {
  getStudioDraft as getRuntimeStudioDraft,
  getStudioWorkspaceDashboard as getRuntimeStudioWorkspaceDashboard,
  listWorkspaceAlerts as getRuntimeWorkspaceAlerts,
} from './studio-runtime';
import type {
  StudioDraftPagePayload,
  StudioIndexPayload,
  StudioPolicyAlert,
  StudioWorkspaceDashboardPayload,
  StudioWorkspaceDetailPayload,
} from './studio-types';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

async function fetchJson<T>(path: string, fallback: T): Promise<T> {
  try {
    const response = await fetch(`${BASE}${path}`, { cache: 'no-store' });
    if (!response.ok) {
      return fallback;
    }
    return (await response.json()) as T;
  } catch {
    return fallback;
  }
}

export function getStudioIndex(): Promise<StudioIndexPayload> {
  return fetchJson('/api/studio/workspaces', getFallbackStudioIndex());
}

export function getStudioWorkspace(workspaceId: string): Promise<StudioWorkspaceDetailPayload> {
  return fetchJson(`/api/studio/workspaces/${workspaceId}`, getFallbackStudioWorkspace(workspaceId));
}

export async function getStudioWorkspaceDashboard(workspaceId: string): Promise<StudioWorkspaceDashboardPayload | null> {
  return getRuntimeStudioWorkspaceDashboard(workspaceId);
}

export async function getStudioDraft(draftId: string): Promise<StudioDraftPagePayload | null> {
  return getRuntimeStudioDraft(draftId);
}

export async function getStudioWorkspaceAlerts(workspaceId: string): Promise<StudioPolicyAlert[]> {
  return getRuntimeWorkspaceAlerts(workspaceId);
}
