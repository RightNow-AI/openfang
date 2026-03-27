import { NextResponse } from 'next/server';

import { getDaemonUrl } from '../../../../../../../lib/api-server';
import { env } from '../../../../../../../lib/env';
import routeAuthorizationModule from '../../../../../../../lib/route-authorization';

const { jsonFromAuthError, requireApiPolicy } = routeAuthorizationModule;

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

async function proxyRunArtifacts(request, runId) {
  const targetUrl = new URL(`${getDaemonUrl()}/api/runs/${encodeURIComponent(runId)}/artifacts`);
  const headers = new Headers(request.headers);
  headers.delete('host');
  headers.delete('connection');
  headers.delete('cookie');
  headers.delete('origin');
  headers.delete('x-csrf-token');
  if (env.OPENFANG_API_KEY) {
    headers.set('authorization', `Bearer ${env.OPENFANG_API_KEY}`);
  }

  return fetch(targetUrl, {
    method: 'GET',
    headers,
    redirect: 'manual',
    cache: 'no-store',
  });
}

export async function GET(request, { params }) {
  const { workspaceId, runId } = await params;

  try {
    await requireApiPolicy(request, `/api/workspaces/${workspaceId}/runs/${runId}/artifacts`);
  } catch (error) {
    return jsonFromAuthError(error);
  }

  try {
    const upstream = await proxyRunArtifacts(request, runId);
    const data = await upstream.json();
    return NextResponse.json(
      {
        workspaceId,
        runId,
        artifacts: Array.isArray(data?.artifacts) ? data.artifacts : [],
        total: Number.isFinite(data?.total) ? data.total : 0,
      },
      { status: upstream.status }
    );
  } catch (error) {
    const message = error instanceof Error ? error.message : 'Run artifact proxy request failed';
    return NextResponse.json({ error: message }, { status: 502 });
  }
}