import { NextRequest } from 'next/server';
import { fallbackDraftRunResponse, proxyJson, readJson } from '../../../../_lib/studio-proxy';

type Props = {
  params: Promise<{ draftId: string }>;
};

export async function POST(request: NextRequest, { params }: Props) {
  const { draftId } = await params;
  const body = await readJson(request);
  try {
    return await proxyJson(`/v1/drafts/${draftId}/publish/run`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    return fallbackDraftRunResponse(draftId, 'publish', body);
  }
}