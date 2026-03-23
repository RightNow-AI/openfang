import { NextRequest } from 'next/server';
import { fallbackEventsResponse, proxyJson } from '../_lib/studio-proxy';

export async function GET(request: NextRequest) {
  const workspaceId = request.nextUrl.searchParams.get('workspace_id');
  try {
    const query = workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : '';
    return await proxyJson(`/api/studio/events${query}`);
  } catch {
    return fallbackEventsResponse(workspaceId);
  }
}
