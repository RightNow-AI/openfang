import { NextRequest, NextResponse } from "next/server";

const BASE = process.env.OPENFANG_BASE_URL ?? "http://127.0.0.1:50051";

export async function POST(
  _req: NextRequest,
  { params }: { params: Promise<{ mode: string; id: string }> }
) {
  const { mode, id } = await params;
  const res = await fetch(`${BASE}/modes/${mode}/tasks/${id}/run`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: "{}",
    cache: "no-store",
  });
  const text = await res.text();
  return new NextResponse(text, { status: res.status, headers: { "Content-Type": "application/json" } });
}
