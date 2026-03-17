/**
 * lib/skills.js
 *
 * Shared normalizers for skill data returned by the daemon.
 * Used by server-side API routes AND the SkillsPage server component.
 *
 * Single source of truth for the field contract between daemon and UI.
 */

/**
 * Normalize a raw skill from the daemon into the list card shape.
 *
 * @param {object} raw
 * @param {number} i   fallback index for synthetic id
 * @returns SkillCard
 */
export function normalizeSkillCard(raw, i) {
  return {
    name: String(raw?.name ?? raw?.id ?? `skill-${i}`),
    description: String(raw?.description ?? ''),
    runtime: String(raw?.runtime ?? raw?.language ?? raw?.type ?? ''),
    installed: raw?.installed !== false,   // treat absent as true — daemon only lists installed
    enabled: raw?.enabled !== false,
    bundled: raw?.bundled ?? raw?.builtin ?? !raw?.custom ?? true,
    version: String(raw?.version ?? ''),
    tool_count: Number(raw?.tool_count ?? raw?.tools?.length ?? 0),
    used_by_count: Number(raw?.used_by_count ?? 0),  // injected by skill-usage helper
  };
}

/**
 * Normalize raw skill detail from daemon into the drawer shape.
 *
 * @param {object} raw
 * @param {string[]} used_by  agent names from skill-usage index
 * @returns SkillDetail
 */
export function normalizeSkillDetail(raw, used_by = []) {
  const tools = Array.isArray(raw?.tools)
    ? raw.tools.map(t => (typeof t === 'string' ? t : String(t?.name ?? t?.id ?? '')))
    : [];

  // Deduplicate agent names — callers should never pass duplicates but defend anyway
  const uniqueUsedBy = [...new Set(used_by)];

  return {
    name: String(raw?.name ?? raw?.id ?? ''),
    description: String(raw?.description ?? ''),
    runtime: String(raw?.runtime ?? raw?.language ?? raw?.type ?? ''),
    installed: raw?.installed !== false,
    enabled: raw?.enabled !== false,
    bundled: raw?.bundled ?? raw?.builtin ?? !raw?.custom ?? true,
    version: String(raw?.version ?? ''),
    tool_count: Number(raw?.tool_count ?? tools.length),
    source: String(raw?.source ?? raw?.repository ?? raw?.path ?? ''),
    entrypoint: String(raw?.entrypoint ?? raw?.entry_point ?? ''),
    prompt_context: String(raw?.prompt_context ?? raw?.system_prompt ?? ''),
    tools,
    used_by: uniqueUsedBy,
    used_by_count: uniqueUsedBy.length,
  };
}
