import { NextResponse } from 'next/server';
import { listFounderPlaybooks } from '../../../lib/founder-playbooks';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  return NextResponse.json({ playbooks: listFounderPlaybooks() });
}