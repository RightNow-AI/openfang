import { NextRequest } from 'next/server';
import {
  fallbackDraftCreateResponse,
  fallbackDraftListResponse,
  proxyJson,
  readJson,
} from '../../../_lib/studio-proxy';

type Props = {
  params: Promise<{ workspaceId: string }>;
};

export async function GET(_: NextRequest, { params }: Props) {
  const { workspaceId } = await params;
  try {
    return await proxyJson(`/api/studio/workspaces/${workspaceId}/drafts`);
  } catch {
    return fallbackDraftListResponse(workspaceId);
  }
}

export async function POST(request: NextRequest, { params }: Props) {
  const { workspaceId } = await params;
  const body = await readJson(request);
  try {
    return await proxyJson(`/api/studio/workspaces/${workspaceId}/drafts`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    return fallbackDraftCreateResponse(workspaceId, body);
  }
}
