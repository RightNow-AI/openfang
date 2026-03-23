import { NextRequest } from 'next/server';
import {
  fallbackStageListResponse,
  fallbackStagePatchResponse,
  proxyJson,
  readJson,
} from '../../../_lib/studio-proxy';

type Props = {
  params: Promise<{ workspaceId: string }>;
};

export async function GET(_: NextRequest, { params }: Props) {
  const { workspaceId } = await params;
  try {
    return await proxyJson(`/api/studio/workspaces/${workspaceId}/stages`);
  } catch {
    return fallbackStageListResponse(workspaceId);
  }
}

export async function PATCH(request: NextRequest, { params }: Props) {
  const { workspaceId } = await params;
  const body = await readJson(request);
  try {
    return await proxyJson(`/api/studio/workspaces/${workspaceId}/stages`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    return fallbackStagePatchResponse(workspaceId, body);
  }
}
