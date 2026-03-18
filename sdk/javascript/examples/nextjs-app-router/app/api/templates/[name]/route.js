/**
 * GET /api/templates/[name]
 *
 * Proxy to daemon GET /api/templates/{name} — returns template manifest + raw TOML.
 *
 * Response: { name, manifest, manifest_toml, suggested_skills? }
 *
 * Enhancement (Phase 4):
 *   - If the daemon does not populate manifest.skills, this route extracts
 *     [[skills]] tables from manifest_toml and merges them in.
 *   - For legacy templates with no [[skills]] but with capabilities.tools, a
 *     `suggested_skills` array is derived (migration aid — not persisted).
 *
 * PUT /api/templates/[name]
 *
 * Proxy to daemon PUT /api/templates/{name} — updates template manifest.
 *
 * Body: { manifest_toml?: string, skills?: SkillBinding[] }
 *   When `skills` is provided it is validated before forwarding to the daemon.
 *
 * Response: daemon response or 400 validation error.
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';
import {
  extractSkillsFromToml,
  normalizeSkillBinding,
  deriveSuggestedSkills,
  validateSkillBindings,
} from '../../../../lib/agent-skills';
import { guardDevToken } from '../../../../lib/dev-token-guard';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(request, { params }) {
  try {
    const name = params?.name;
    if (!name) {
      return NextResponse.json({ error: 'Template name is required' }, { status: 400 });
    }
    const data = await api.get(`/api/templates/${encodeURIComponent(name)}`);

    // ── Phase 4: skill binding enrichment ──────────────────────────────────
    // If daemon-parsed manifest already has skills, pass through untouched.
    // Otherwise, try to extract [[skills]] tables from the raw TOML.
    if (!data.manifest?.skills?.length && data.manifest_toml) {
      const rawSkills = extractSkillsFromToml(data.manifest_toml);
      if (rawSkills.length > 0) {
        data.manifest = {
          ...(data.manifest ?? {}),
          skills: rawSkills.map(normalizeSkillBinding),
        };
      }
    }

    // For legacy templates (no [[skills]], but has capabilities.tools), derive
    // non-persisted suggestions as a migration aid — callers render these with
    // a "Suggested from tool references" label and must not auto-persist them.
    if (!data.manifest?.skills?.length && data.manifest?.capabilities?.tools?.length) {
      data.suggested_skills = deriveSuggestedSkills(data.manifest);
    }

    return NextResponse.json(data);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: err.status ?? 502 });
  }
}

export async function PUT(request, { params }) {
  const denied = guardDevToken(request);
  if (denied) return denied;

  const name = params?.name;
  if (!name) {
    return NextResponse.json({ error: 'Template name is required.' }, { status: 400 });
  }

  let body;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: 'Request body must be valid JSON.' }, { status: 400 });
  }

  // Validate the skills array if the caller is updating bindings
  if (body?.skills !== undefined) {
    if (!Array.isArray(body.skills)) {
      return NextResponse.json({ error: '`skills` must be an array.' }, { status: 400 });
    }
    const { ok, errors } = validateSkillBindings(body.skills);
    if (!ok) {
      return NextResponse.json(
        { error: 'Invalid skill bindings.', validation_errors: errors },
        { status: 400 }
      );
    }
  }

  try {
    const data = await api.put(`/api/templates/${encodeURIComponent(name)}`, body);
    return NextResponse.json(data);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: err?.status ?? 502 });
  }
}

