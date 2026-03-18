import { NextRequest, NextResponse } from "next/server";

const BASE = process.env.OPENFANG_BASE_URL ?? "http://127.0.0.1:50051";

export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ mode: string; id: string }> }
) {
  const { mode, id } = await params;
  const res = await fetch(`${BASE}/modes/${mode}/records/${id}`, { cache: "no-store" });
  const text = await res.text();
  return new NextResponse(text, { status: res.status, headers: { "Content-Type": "application/json" } });
}

export async function PUT(
  req: NextRequest,
  { params }: { params: Promise<{ mode: string; id: string }> }
) {
  const { mode, id } = await params;
  const body = await req.json();
  const res = await fetch(`${BASE}/modes/${mode}/records/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    cache: "no-store",
  });
  const text = await res.text();
  return new NextResponse(text, { status: res.status, headers: { "Content-Type": "application/json" } });
}
