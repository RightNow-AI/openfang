/**
 * POST /api/brand/autofill
 *
 * Asks an available agent to infer brand profile fields from a given website URL.
 * The agent uses its knowledge about the domain / URL pattern to suggest values.
 * Returns a partial patch of profile fields plus "found" / "missing" arrays.
 *
 * Body:     { website_url: string }
 * Response: { patch: Record<string, string>, found: string[], missing: string[] }
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const PROFILE_FIELDS = [
  'business_name', 'industry', 'business_model', 'location',
  'primary_offer', 'main_goal_90_days', 'ideal_customer',
  'brand_promise', 'core_offer', 'pricing_model', 'primary_cta',
];

function buildPrompt(website_url) {
  return `You are a brand research assistant. Given a website URL, infer as many brand profile fields as you can from your knowledge of the domain, industry, and any public information about the business.

Website URL: ${website_url}

Return ONLY a valid JSON object with these fields (omit any fields you cannot determine with reasonable confidence):
{
  "business_name": "The company or brand name",
  "industry": "The industry sector (e.g. 'Marketing Agency', 'E-commerce', 'SaaS')",
  "business_model": "one of: service, agency, consulting, ecommerce, saas, info-product, local-business, creator-business, other",
  "location": "City, state/country if known",
  "primary_offer": "Main product or service they sell",
  "ideal_customer": "Who they primarily serve",
  "brand_promise": "Core value proposition",
  "core_offer": "Specific offer or package (if known)",
  "pricing_model": "one of: one-time, retainer, subscription, project-based, high-ticket, custom",
  "primary_cta": "Primary call to action (e.g. 'Book a call', 'Start free trial')"
}

Return ONLY the JSON object. No preamble. No explanation. No markdown fences. Start with { and end with }.`;
}

export async function POST(request) {
  let body;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: 'Invalid JSON body' }, { status: 400 });
  }

  const website_url = String(body?.website_url ?? '').trim();
  if (!website_url) {
    return NextResponse.json({ error: 'website_url is required' }, { status: 400 });
  }

  // Basic URL guard — must look like a URL
  if (!website_url.startsWith('http://') && !website_url.startsWith('https://')) {
    return NextResponse.json({ error: 'website_url must start with http:// or https://' }, { status: 400 });
  }

  // ── 1. Fetch available agents ───────────────────────────────────────────
  let agents;
  try {
    const raw = await api.get('/api/agents');
    agents = Array.isArray(raw) ? raw : (raw?.agents ?? []);
  } catch (err) {
    return NextResponse.json({ error: `Cannot reach the backend: ${err.message}` }, { status: 503 });
  }

  if (!agents.length) {
    return NextResponse.json({ error: 'No agents loaded.' }, { status: 503 });
  }

  const pick =
    agents.find(a => (a.name ?? a.id ?? '').toLowerCase().includes('researcher')) ??
    agents.find(a => (a.name ?? a.id ?? '').toLowerCase().includes('analyst')) ??
    agents.find(a => (a.name ?? a.id ?? '').toLowerCase().includes('assistant')) ??
    agents[0];

  // ── 2. Call agent ──────────────────────────────────────────────────────
  let result;
  try {
    result = await api.post(`/api/agents/${pick.id}/message`, {
      message: buildPrompt(website_url),
    });
  } catch (err) {
    return NextResponse.json({ error: `Agent call failed: ${err.message}` }, { status: 502 });
  }

  const raw = String(result?.response ?? result?.message ?? result?.content ?? result?.text ?? '').trim();
  if (!raw) {
    return NextResponse.json({ error: 'Agent returned an empty response.' }, { status: 502 });
  }

  // ── 3. Parse JSON from agent response ──────────────────────────────────
  let patch = {};
  try {
    // Extract JSON even if the agent wrapped it in markdown fences
    const match = raw.match(/\{[\s\S]*\}/);
    if (match) patch = JSON.parse(match[0]);
  } catch {
    return NextResponse.json({ error: 'Could not parse the website data. Try entering details manually.' }, { status: 502 });
  }

  // Only keep known fields, sanitise values to strings
  const clean = {};
  for (const key of PROFILE_FIELDS) {
    if (patch[key] !== undefined && patch[key] !== null && String(patch[key]).trim()) {
      clean[key] = String(patch[key]).trim();
    }
  }

  const found   = Object.keys(clean);
  const missing = PROFILE_FIELDS.filter(k => !found.includes(k));

  if (!found.length) {
    return NextResponse.json({ error: 'We could not infer any details from that URL. Please fill in the fields manually.' }, { status: 422 });
  }

  return NextResponse.json({ patch: clean, found, missing });
}
