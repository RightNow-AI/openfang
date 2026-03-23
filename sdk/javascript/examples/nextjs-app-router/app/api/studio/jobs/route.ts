import { NextRequest } from 'next/server';
import {
  fallbackCreateJobResponse,
  fallbackJobsResponse,
  proxyJson,
  readJson,
} from '../_lib/studio-proxy';

export async function GET(request: NextRequest) {
  const workspaceId = request.nextUrl.searchParams.get('workspace_id');
  try {
    const query = workspaceId ? `?workspace_id=${encodeURIComponent(workspaceId)}` : '';
    return await proxyJson(`/api/studio/jobs${query}`);
  } catch {
    return fallbackJobsResponse(workspaceId);
  }
}

export async function POST(request: NextRequest) {
  const body = await readJson(request);
  try {
    return await proxyJson('/api/studio/jobs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    return fallbackCreateJobResponse(body);
  }
}
