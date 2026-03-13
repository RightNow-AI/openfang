import { NextResponse } from "next/server";

import { applyIdentityCookie, resolveUserIdentity } from "../../../../lib/auth";
import { sendMessage } from "../../../../lib/openfang-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

export async function POST(request) {
  try {
    const identity = await resolveUserIdentity(request);
    const body = await request.json();
    const message = String(body.message || "").trim();

    if (!message) {
      return NextResponse.json(
        { error: "message is required" },
        { status: 400 },
      );
    }

    const result = await sendMessage(
      identity.userId,
      message,
      body.metadata && typeof body.metadata === "object" ? body.metadata : undefined,
    );

    return applyIdentityCookie(NextResponse.json(result), identity);
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : String(error) },
      { status: 500 },
    );
  }
}
