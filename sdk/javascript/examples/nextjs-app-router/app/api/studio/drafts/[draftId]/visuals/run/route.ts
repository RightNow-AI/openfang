import { NextRequest, NextResponse } from 'next/server';
import { getDecryptedProviderKey } from '../../../../../../lib/provider-credential-store';
import { fallbackDraftRunResponse, proxyJson, readJson } from '../../../../_lib/studio-proxy';
import { getStudioDraft } from '../../../../../../studio/lib/studio-runtime';

type Props = {
  params: Promise<{ draftId: string }>;
};

export const runtime = 'nodejs';

export async function POST(request: NextRequest, { params }: Props) {
  const { draftId } = await params;
  const body = await readJson(request);
  const providerId = typeof body.provider === 'string' ? body.provider.trim() : '';

  if (!providerId) {
    return NextResponse.json({ error: 'provider is required' }, { status: 400 });
  }

  const draft = getStudioDraft(draftId);
  if (!draft) {
    return NextResponse.json({ error: 'Draft not found' }, { status: 404 });
  }

  const credential = await getDecryptedProviderKey(draft.draft.workspaceId, providerId, { allowGlobalFallback: true });
  if (!credential) {
    return NextResponse.json({ error: `No API key configured for ${providerId}` }, { status: 403 });
  }

  try {
    return await proxyJson(`/v1/drafts/${draftId}/visuals/run`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Provider-Key': credential.apiKey,
      },
      body: JSON.stringify(body),
    });
  } catch {
    return fallbackDraftRunResponse(draftId, 'visuals', body);
  }
}