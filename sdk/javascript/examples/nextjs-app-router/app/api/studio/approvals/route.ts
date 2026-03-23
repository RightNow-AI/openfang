import { NextRequest } from 'next/server';
import {
  fallbackApprovalResponse,
  proxyJson,
  readJson,
} from '../_lib/studio-proxy';

export async function POST(request: NextRequest) {
  const body = await readJson(request);
  try {
    return await proxyJson('/api/studio/approvals', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch {
    return fallbackApprovalResponse(body);
  }
}
