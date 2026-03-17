/**
 * Shared spawn-name validation.
 *
 * Used by both:
 *   - app/agent-catalog/AgentCatalogClient.js  (browser, client-side)
 *   - app/api/agents/spawn/route.js            (Node.js, server-side)
 *
 * Returns { name: string } on success or { error: string } on failure.
 * The returned `name` is trimmed — callers should use it instead of the raw value.
 *
 * Rules (must stay in sync with any backend daemon validation):
 *   - Required (non-empty after trim)
 *   - Max 64 characters
 *   - No control characters (0x00–0x1f)
 *   - No filesystem-unsafe characters: < > : " / \ | ? *
 *   - No leading or trailing dots (NTFS constraint)
 */
export function validateSpawnName(raw) {
  if (typeof raw !== 'string') return { error: 'Name must be a string.' };
  const name = raw.trim();
  if (!name) return { error: 'Name is required.' };
  if (name.length > 64) return { error: 'Name must be 64 characters or less.' };
  if (/[\x00-\x1f]/.test(name)) return { error: 'Name cannot contain control characters.' };
  if (/[<>:"/\\|?*]/.test(name)) return { error: 'Name contains invalid characters (< > : " / \\ | ? *).' };
  if (/^\.+$/.test(name) || name.startsWith('.') || name.endsWith('.')) {
    return { error: 'Name cannot start or end with a dot.' };
  }
  return { name };
}
