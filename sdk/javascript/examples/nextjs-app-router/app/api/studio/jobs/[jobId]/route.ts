import { NextRequest } from 'next/server';
import { fallbackJobResponse, proxyJson } from '../../_lib/studio-proxy';

type Props = {
  params: Promise<{ jobId: string }>;
};

export async function GET(_: NextRequest, { params }: Props) {
  const { jobId } = await params;
  try {
    return await proxyJson(`/api/studio/jobs/${jobId}`);
  } catch {
    return fallbackJobResponse(jobId);
  }
}
