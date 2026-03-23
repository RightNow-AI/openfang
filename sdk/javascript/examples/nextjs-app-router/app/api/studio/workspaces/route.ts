import { NextRequest } from 'next/server';
import {
  fallbackCreateWorkspaceResponse,
  fallbackIndexResponse,
  proxyJson,
  readJson,
} from '../_lib/studio-proxy';

export async function GET() {
  try {
    return await proxyJson('/api/studio/workspaces');
  } catch {
    return fallbackIndexResponse();
  }
}

export async function POST(request: NextRequest) {
  const body = await readJson(request);
  try {
    return await proxyJson('/api/studio/workspaces', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    return fallbackCreateWorkspaceResponse(body);
  }
}
