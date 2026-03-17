/**
 * Lightweight fire-and-forget telemetry.
 *
 * Events are posted to POST /api/telemetry via navigator.sendBeacon.
 * Failures are silently swallowed — telemetry must never surface errors.
 *
 * Usage:
 *   import { track } from '../lib/telemetry';
 *   track('spawn_started', { template: 'researcher' });
 */

const TELEMETRY_URL = '/api/telemetry';

export function track(event, props = {}) {
  if (typeof window === 'undefined') return; // SSR guard
  try {
    const payload = JSON.stringify({ event, props, ts: Date.now() });
    if (navigator.sendBeacon) {
      navigator.sendBeacon(
        TELEMETRY_URL,
        new Blob([payload], { type: 'application/json' }),
      );
    }
  } catch {
    // intentionally silent
  }
}
