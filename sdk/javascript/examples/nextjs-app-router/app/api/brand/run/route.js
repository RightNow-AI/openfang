/**
 * POST /api/brand/run
 *
 * Runs a brand task using an available OpenFang agent.
 * Builds a structured prompt from the brand context + task type,
 * forwards it to an agent, and returns the output.
 *
 * Body:     { task_type, brand_profile, agent_id? }
 * Response: { output_type, title, content, task_type, agent_id, duration_ms }
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

// ── Prompt builders ────────────────────────────────────────────────────────

function fmt(arr, sep = ', ') {
  return (arr || []).filter(Boolean).join(sep) || 'Not provided';
}

const TASK_CONFIG = {
  analyze_business: {
    output_type: 'brand_brief',
    title: 'Brand Brief',
    build_prompt: (p) => `You are an expert Brand Strategist producing a professional brand brief.

BUSINESS CONTEXT:
- Business name: ${p.business_name || 'Not provided'}
- Website: ${p.website_url || 'Not provided'}
- Industry: ${p.industry || 'Not provided'}
- Business model: ${p.business_model || 'Not provided'}
- Primary offer: ${p.primary_offer || 'Not provided'}
- 90-day goal: ${p.main_goal_90_days || 'Not provided'}
- Ideal customer: ${p.ideal_customer || 'Not provided'}
- Competitors: ${(p.top_competitors || []).filter(c => c.name).map(c => c.name + (c.url ? ` (${c.url})` : '')).join(', ') || 'Not provided'}
- Brand traits: ${fmt(p.brand_traits)}
- Proof assets: ${fmt(p.proof_assets)}

Produce a brand brief with these clearly labelled sections:

## Business Summary
(2-3 sentences on what this business does and who it serves)

## Offer Summary
(Clear description of the primary offer and its core value proposition)

## Audience Summary
(Who they serve — specific characteristics, context, and buying situation)

## Positioning Hypothesis
(What makes this business distinctly different — 1-2 direct sentences)

## Message Opportunities
(At least 3 specific messaging angles they should pursue — be concrete, not generic)

## Missing Information
(At least 3 gaps in the provided context that would strengthen the brief)

## Recommended Next Actions
(At least 3 specific actions the client should take next)

Be specific and direct. No marketing fluff.`,
  },

  research_competitors: {
    output_type: 'competitor_matrix',
    title: 'Competitor Analysis',
    build_prompt: (p) => `You are an expert Market Researcher producing a competitive analysis.

BUSINESS CONTEXT:
- Business: ${p.business_name || 'Not provided'} — ${p.industry || 'unknown industry'}
- Primary offer: ${p.primary_offer || 'Not provided'}
- Ideal customer: ${p.ideal_customer || 'Not provided'}
- Desired customer outcomes: ${fmt(p.desired_outcomes, '; ')}
- Top customer objections: ${fmt(p.top_objections, '; ')}

COMPETITORS:
${(p.top_competitors || []).filter(c => c.name).map((c, i) => `${i + 1}. ${c.name}${c.url ? ` — ${c.url}` : ''}${c.notes ? ` — ${c.notes}` : ''}`).join('\n') || `No competitors specified — identify and analyze 3 likely competitors in the ${p.industry || 'specified'} space`}

Produce a competitor matrix with these sections:

## Competitor Profiles
(For each competitor: messaging style, offer positioning, target audience, apparent strengths and weaknesses)

## Messaging Patterns
(Common language, angles, and themes across competitors)

## Offer Comparison
(How each competitor's offer differs — pricing model, delivery, positioning)

## Market Gaps
(Specific positioning gaps and underserved audience needs you identified)

## Angle Opportunities
(At least 3 ways this business can differentiate based on competitive gaps)

## Recommended Positioning
(Where this business should plant its flag based on the competitive landscape)

Be evidence-based and specific.`,
  },

  build_voice_guide: {
    output_type: 'voice_guide',
    title: 'Brand Voice Guide',
    build_prompt: (p) => `You are an expert Brand Voice Trainer creating a reusable voice system.

BRAND INPUTS:
- Business: ${p.business_name || 'Not provided'} — ${p.industry || 'unknown industry'}
- Brand promise: ${p.brand_promise || 'Not provided'}
- Traits to embody: ${fmt(p.brand_traits)}
- Traits to avoid: ${fmt(p.traits_to_avoid)}
- Examples they like: ${(p.liked_examples || []).filter(e => e.value).map(e => e.value + (e.reason_liked ? ` (liked because: ${e.reason_liked})` : '')).join('\n') || 'Not provided'}
- Examples they dislike: ${(p.disliked_examples || []).filter(e => e.value).map(e => e.value + (e.reason_disliked ? ` (disliked because: ${e.reason_disliked})` : '')).join('\n') || 'Not provided'}
- Taboo words: ${fmt(p.taboo_words)}
- Approved phrases: ${fmt(p.approved_words)}
- Voice notes: ${p.voice_notes || 'None'}

Produce a practical brand voice guide with these sections:

## Voice Summary
(2-3 sentences describing this brand's voice and personality)

## Core Tone Rules
(At least 5 specific rules defining how this brand speaks — concrete, not abstract)

## Tone Rules to Avoid
(At least 5 specific patterns to never use — include examples of what NOT to write)

## Approved Phrases and Patterns
(Phrases and structures that fit this voice)

## Forbidden Phrases
(Words and phrases explicitly banned — include brief reason why)

## Headline Rules
(Specific patterns for headlines in this voice — include 3 example headlines)

## CTA Rules
(How to write calls to action — include 3 example CTAs)

## Example Rewrites
(3 before/after rewrites: generic version → this brand's voice version)

This guide must be usable immediately by a copywriter or AI agent without further instruction.`,
  },

  create_customer_avatar: {
    output_type: 'customer_avatar',
    title: 'Customer Avatar',
    build_prompt: (p) => `You are an expert Brand Strategist creating a detailed customer avatar.

AUDIENCE DATA:
- Ideal customer: ${p.ideal_customer || 'Not provided'}
- Top pain points: ${fmt(p.top_pain_points, '; ')}
- Desired outcomes: ${fmt(p.desired_outcomes, '; ')}
- Top objections to buying: ${fmt(p.top_objections, '; ')}
- Awareness level: ${p.customer_awareness_level || 'Not specified'}
- Acquisition channels: ${fmt(p.current_acquisition_channels)}

BUSINESS CONTEXT:
- Business: ${p.business_name || 'Not provided'}
- Primary offer: ${p.primary_offer || 'Not provided'}
- Industry: ${p.industry || 'Not provided'}

Create a detailed avatar with these sections:

## Avatar Identity
(Name, age range, role, life situation — be specific)

## Psychographic Profile
(Core values, beliefs, lifestyle characteristics relevant to this purchase)

## Core Problem
(The specific problem they're experiencing — be vivid and situational)

## Desired Result
(Exactly what success looks like — specific and emotional)

## Buying Triggers
(At least 4 specific things that move them from considering to buying)

## Key Objections and Rebuttals
(For each objection listed, write a specific honest rebuttal)

## Message Hooks
(At least 5 phrases that would stop this person mid-scroll)

## Proof They Need
(What evidence would make them trust and buy)

## Best CTAs for This Person
(3 calls to action calibrated to their position in the buying journey)

Make this avatar specific enough that a copywriter could write directly to this person tomorrow.`,
  },

  draft_outreach_email_sequence: {
    output_type: 'email_sequence',
    title: 'Outreach Email Sequence',
    build_prompt: (p) => `You are an expert Email Operator and Copywriter drafting a complete outreach sequence.

BUSINESS CONTEXT:
- Business: ${p.business_name || 'Not provided'} — ${p.industry || 'unknown industry'}
- Core offer: ${p.core_offer || p.primary_offer || 'Not provided'}
- Pricing model: ${p.pricing_model || 'Not specified'}
- Primary CTA: ${p.primary_cta || 'Not provided'}
- Ideal customer: ${p.ideal_customer || 'Not provided'}
- Key objections: ${fmt(p.top_objections, '; ')}
- Brand traits: ${fmt(p.brand_traits)}
- Traits to avoid: ${fmt(p.traits_to_avoid)}
- Proof assets: ${fmt(p.proof_assets, '; ')}
- Brand promise: ${p.brand_promise || 'Not provided'}

Draft a 3-email outreach sequence. For each email provide:
- Subject line + one alternative
- Preview text (40-80 chars)
- Full email body
- CTA with [LINK] placeholder
- Personalization tokens to replace
- Send timing note

---

## Email 1 — Opener
(Goal: earn attention with a relevant value hook — NOT a generic intro)

## Email 2 — Value + Social Proof
(Goal: deepen interest, address the most common objection, add proof)

## Email 3 — Decision + CTA
(Goal: remove friction, make the action feel obvious)

---

## Sequence Notes
- Ideal audience segment
- Personalization guidance
- What to A/B test first
- Recommended send cadence

Voice: ${fmt(p.brand_traits)} — avoid: ${fmt(p.traits_to_avoid)}.
These emails must be ready for review or send with minimal editing.`,
  },
};

// ── Handler ───────────────────────────────────────────────────────────────

export async function POST(request) {
  let body;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: 'Request body must be valid JSON.' }, { status: 400 });
  }

  const { task_type, brand_profile, agent_id } = body ?? {};

  if (!task_type) {
    return NextResponse.json({ error: 'task_type is required.' }, { status: 400 });
  }

  const taskConfig = TASK_CONFIG[task_type];
  if (!taskConfig) {
    return NextResponse.json({ error: `Unknown task_type: ${task_type}` }, { status: 400 });
  }

  if (!brand_profile || typeof brand_profile !== 'object') {
    return NextResponse.json({ error: 'brand_profile is required.' }, { status: 400 });
  }

  const message = taskConfig.build_prompt(brand_profile);

  // Resolve which agent to use
  let targetAgentId = agent_id;
  if (!targetAgentId) {
    try {
      const agents = await api.get('/api/agents');
      const list = Array.isArray(agents)
        ? agents
        : (agents?.publicAgents ?? agents?.agents ?? []);
      if (list.length > 0) {
        targetAgentId = list[0].id;
      }
    } catch {
      return NextResponse.json(
        { error: 'Daemon unreachable. Start the daemon and ensure an agent is configured.' },
        { status: 503 },
      );
    }
  }

  if (!targetAgentId) {
    return NextResponse.json(
      { error: 'No agent available. Configure at least one agent in the daemon.' },
      { status: 503 },
    );
  }

  const started = Date.now();
  try {
    const data = await api.post(`/api/agents/${targetAgentId}/message`, { message });
    const duration_ms = Date.now() - started;
    const content = data.response ?? data.reply ?? String(data);

    return NextResponse.json({
      output_type: taskConfig.output_type,
      title: taskConfig.title,
      content,
      task_type,
      agent_id: targetAgentId,
      duration_ms,
    });
  } catch (err) {
    const errMsg = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: `Agent run failed: ${errMsg}` }, { status: 502 });
  }
}
