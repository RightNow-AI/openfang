/**
 * LEGACY COMPATIBILITY ROUTE — GET /api/session
 *
 * This route is NOT part of the current dashboard architecture.
 * It is retained for backward compatibility with the original SDK example
 * contract and any external integrations that depend on it.
 *
 * lib/auth.js and lib/session-store.js exist solely to support this route
 * and the /api/ai/chat/* family.  The current dashboard does not use
 * cookie-based session identity.
 *
 * Current dashboard docs: README.md § API Routes
 *
 * Do NOT add new UI features here.
 */
import { NextResponse } from "next/server";

import { applyIdentityCookie, resolveUserIdentity } from "../../../lib/auth";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

export async function GET(request) {
  try {
    const identity = await resolveUserIdentity(request);
    const response = NextResponse.json({
      user: {
        id: identity.userId,
        authProvider: identity.authProvider,
      },
    });

    return applyIdentityCookie(response, identity);
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : String(error) },
      { status: 500 },
    );
  }
}