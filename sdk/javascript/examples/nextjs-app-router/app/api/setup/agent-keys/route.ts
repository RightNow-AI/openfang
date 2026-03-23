import { NextRequest, NextResponse } from 'next/server';
import { deleteProviderCredential, listProviderCredentialMetadata, upsertProviderCredential } from '../../../lib/provider-credential-store';

export const runtime = 'nodejs';

function readString(value: unknown) {
  return typeof value === 'string' ? value.trim() : '';
}

export async function POST(request: NextRequest) {
  try {
    const body = await request.json().catch(() => ({}));
    const workspaceId = readString(body.workspaceId);
    const providerId = readString(body.providerId);
    const apiKey = readString(body.apiKey);

    if (!workspaceId || !providerId || !apiKey) {
      return NextResponse.json({ error: 'Missing required fields' }, { status: 400 });
    }

    const credential = await upsertProviderCredential({
      workspaceId,
      providerId,
      apiKey,
      keyVersion: 1,
    });

    return NextResponse.json({ success: true, ...credential }, { status: 201 });
  } catch (error) {
    console.error('Failed to save agent key', error);
    return NextResponse.json({ error: 'Internal Server Error' }, { status: 500 });
  }
}

export async function GET(request: NextRequest) {
  try {
    const workspaceId = readString(new URL(request.url).searchParams.get('workspaceId'));
    if (!workspaceId) {
      return NextResponse.json({ error: 'Missing workspaceId' }, { status: 400 });
    }

    const credentials = await listProviderCredentialMetadata(workspaceId);
    return NextResponse.json(credentials);
  } catch (error) {
    console.error('Failed to list agent keys', error);
    return NextResponse.json({ error: 'Internal Server Error' }, { status: 500 });
  }
}

export async function DELETE(request: NextRequest) {
  try {
    const body = await request.json().catch(() => ({}));
    const workspaceId = readString(body.workspaceId);
    const providerId = readString(body.providerId);

    if (!workspaceId || !providerId) {
      return NextResponse.json({ error: 'Missing required fields' }, { status: 400 });
    }

    const deleted = await deleteProviderCredential(workspaceId, providerId);
    return NextResponse.json({ success: deleted, deleted });
  } catch (error) {
    console.error('Failed to delete agent key', error);
    return NextResponse.json({ error: 'Internal Server Error' }, { status: 500 });
  }
}