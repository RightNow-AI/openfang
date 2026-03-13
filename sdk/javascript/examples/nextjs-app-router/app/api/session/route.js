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