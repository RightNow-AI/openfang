import { NextRequest } from 'next/server';
import {
  fallbackDraftPatchResponse,
  fallbackDraftResponse,
  proxyJson,
  readJson,
} from '../../_lib/studio-proxy';

type Props = {
  params: Promise<{ draftId: string }>;
};

const RUST_V1_BASE = process.env.RUST_API_URL ?? process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

export async function GET(_: NextRequest, { params }: Props) {
  const { draftId } = await params;
  try {
    const response = await fetch(`${RUST_V1_BASE}/v1/drafts/${draftId}`, { cache: 'no-store' });
    if (!response.ok) throw new Error('draft unavailable');
    const text = await response.text();
    return new Response(text, {
      status: response.status,
      headers: { 'Content-Type': 'application/json' },
    });
  } catch {
    return fallbackDraftResponse(draftId);
  }
}

export async function PATCH(request: NextRequest, { params }: Props) {
  const { draftId } = await params;
  const body = await readJson(request);
  try {
    return await proxyJson(`/v1/drafts/${draftId}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    return fallbackDraftPatchResponse(draftId, body);
  }
}