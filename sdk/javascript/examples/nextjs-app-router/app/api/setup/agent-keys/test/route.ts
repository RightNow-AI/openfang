import { NextRequest, NextResponse } from 'next/server';
import { getDecryptedProviderKey } from '../../../../lib/provider-credential-store';
import { testProviderConnection } from '../../../../lib/provider-connection-test';

export const runtime = 'nodejs';

function readString(value: unknown) {
  return typeof value === 'string' ? value.trim() : '';
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json().catch(() => ({}));
    const workspaceId = readString(body.workspaceId);
    const providerId = readString(body.providerId);

    if (!workspaceId || !providerId) {
      return NextResponse.json({ error: 'Missing required fields' }, { status: 400 });
    }

    const credential = await getDecryptedProviderKey(workspaceId, providerId);
    if (!credential) {
      return NextResponse.json({ error: `No saved key for ${providerId}` }, { status: 404 });
    }

    const result = await testProviderConnection(providerId, credential.apiKey);
    return NextResponse.json(result, { status: result.ok ? 200 : 502 });
  } catch (error) {
    console.error('Failed to test provider key', error);
    return NextResponse.json({ error: 'Internal Server Error' }, { status: 500 });
  }
}