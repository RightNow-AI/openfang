import { NextRequest, NextResponse } from "next/server";

const BASE = process.env.OPENFANG_BASE_URL ?? "http://127.0.0.1:50051";

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ mode: string }> }
) {
  const { mode } = await params;
  const recordId = req.nextUrl.searchParams.get("record_id") ?? "";
  const res = await fetch(`${BASE}/modes/${mode}/approvals?record_id=${encodeURIComponent(recordId)}`, { cache: "no-store" });
  const text = await res.text();
  return new NextResponse(text, { status: res.status, headers: { "Content-Type": "application/json" } });
}
