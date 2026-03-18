/**
 * lib/dev-token-guard.js
 *
 * Minimal pre-production guard for state-changing API routes.
 *
 * Motivation:
 *   The dashboard has no authentication layer by default.  In a local dev
 *   setup that is fine.  But any deployment — even a test server reachable
 *   on a private network — can silently accept writes from any requester.
 *   This guard provides the smallest concrete safeguard that prevents
 *   accidental open-write exposure without blocking local development.
 *
 * How it works:
 *   When the environment variable OPENFANG_REQUIRE_DEV_TOKEN is set to a
 *   non-empty string, every request that reaches a guarded route must supply
 *   the same value in the X-Dev-Token request header.  Requests without a
 *   matching token receive a 401 response before any handler logic runs.
 *
 *   When OPENFANG_REQUIRE_DEV_TOKEN is unset or empty the guard is a no-op,
 *   so local development remains fully usable without configuration.
 *
 * Security note:
 *   This is a SINGLE SHARED SECRET, not a per-user auth system.  It protects
 *   against accidental exposure, not against a determined adversary with
 *   network access.  Replace with a real authentication layer (OAuth, API
 *   keys per user, JWT) before exposing the dashboard to untrusted networks
 *   or the public internet.
 *
 * Usage in a Next.js route handler:
 *
 *   import { guardDevToken } from '../../../lib/dev-token-guard';
 *
 *   export async function POST(request) {
 *     const denied = guardDevToken(request);
 *     if (denied) return denied;   // ← returns NextResponse 401 if blocked
 *     // ... rest of handler
 *   }
 *
 * @module dev-token-guard
 */
import { NextResponse } from 'next/server';

/**
 * Check the dev-token guard for a request.
 *
 * @param {Request} request  - The incoming Next.js request object
 * @returns {null | NextResponse}  null if the request is allowed through,
 *                                 or a NextResponse 401 if it must be denied.
 */
export function guardDevToken(request) {
  const requiredToken = process.env.OPENFANG_REQUIRE_DEV_TOKEN;

  // Guard is disabled \u2014 dev-friendly default
  if (!requiredToken) return null;

  const provided = request.headers.get('x-dev-token') ?? '';

  if (provided === requiredToken) return null; // authorized

  return NextResponse.json(
    {
      error: 'Unauthorized. Set the X-Dev-Token header to the value of OPENFANG_REQUIRE_DEV_TOKEN.',
      code: 'DEV_TOKEN_REQUIRED',
    },
    { status: 401 },
  );
}
