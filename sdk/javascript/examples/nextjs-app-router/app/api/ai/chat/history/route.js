import { NextResponse } from "next/server";

import { applyIdentityCookie, resolveUserIdentity } from "../../../../../lib/auth";
import { listConversationMessages } from "../../../../../lib/session-store";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

export async function GET(request) {
  try {
    const identity = await resolveUserIdentity(request);
    const limit = Number.parseInt(
      request.nextUrl.searchParams.get("limit") || "20",
      10,
    );
    const messages = await listConversationMessages(identity.userId, { limit });

    const response = NextResponse.json({
      user: { id: identity.userId },
      messages,
    });

    return applyIdentityCookie(response, identity);
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : String(error) },
      { status: 500 },
    );
  }
}