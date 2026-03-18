import { NextRequest, NextResponse } from "next/server";

const BASE = process.env.OPENFANG_BASE_URL ?? "http://127.0.0.1:50051";

type Props = {
  params: Promise<{ id: string }>;
};

export async function POST(_: NextRequest, { params }: Props) {
  const { id } = await params;
  const res = await fetch(`${BASE}/tasks/${id}/run`, {
    method: "POST",
    cache: "no-store",
  });
  const text = await res.text();
  return new NextResponse(text, {
    status: res.status,
    headers: { "Content-Type": "application/json" },
  });
}
