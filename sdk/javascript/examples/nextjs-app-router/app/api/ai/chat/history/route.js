/**
 * LEGACY COMPATIBILITY ROUTE — GET /api/ai/chat/history
 *
 * This route is NOT part of the current dashboard architecture.
 * It is retained for backward compatibility with the original SDK example
 * contract and any external integrations that depend on it.
 *
 * Current dashboard docs: README.md § API Routes
 *
 * Do NOT add new UI features here. If the compatibility layer is no longer
 * needed, remove this file and lib/session-store.js together.
 */
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