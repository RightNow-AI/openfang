---
name: strategist-hand-skill
version: "1.0.0"
description: "Expert knowledge for content strategy — frameworks, editorial calendars, content briefs, audits, competitive analysis, brand voice, and multi-channel planning"
runtime: prompt_only
---

# Content Strategy Expert Knowledge

## Content Strategy Frameworks

### Hero-Hub-Help Model (Google/YouTube)

Structure content into three tiers based on effort, reach, and frequency:

```
HERO (1-2x per quarter)
  Big, high-production pieces designed for broad reach.
  Examples: original research reports, viral campaigns, keynote content, launch events.
  Goal: mass awareness, brand moments, PR pickup.

HUB (1-2x per week)
  Recurring series or themed content your audience returns for.
  Examples: weekly newsletter, podcast episodes, "Friday Tips" thread series.
  Goal: build habit, grow subscribers, deepen engagement.

HELP (daily / evergreen)
  Search-driven, utility content answering real audience questions.
  Examples: how-to guides, FAQs, tutorials, comparison pages, templates.
  Goal: capture search traffic, solve problems, build trust.
```

**Calendar allocation**: ~10% Hero, ~30% Hub, ~60% Help (adjust by strategy_focus).

### PESO Model (Paid, Earned, Shared, Owned)

Map every content piece to a media type to ensure diversified distribution:

| Media Type | Definition | Examples | Metrics |
|-----------|-----------|----------|---------|
| **Paid** | Content promoted with budget | Sponsored posts, PPC, paid social, native ads | CPA, ROAS, CTR |
| **Earned** | Coverage from third parties | Press mentions, guest posts, backlinks, reviews | Domain authority, referral traffic |
| **Shared** | Social distribution by others | Retweets, shares, UGC, community posts | Share count, virality coefficient |
| **Owned** | Your controlled channels | Blog, newsletter, website, app | Traffic, subscribers, time on page |

**Strategy rule**: Every content piece should have a primary PESO channel and at least one secondary.

### Content Pillars Framework

Define 3-5 recurring themes that anchor all content production:

```
Step 1: Identify brand expertise areas (what you know deeply)
Step 2: Map to audience pain points (what they need)
Step 3: Intersection = Content Pillars

Example for a B2B SaaS company:
  Pillar 1: Product education (how-tos, tutorials, feature deep dives)
  Pillar 2: Industry trends (market analysis, predictions, data)
  Pillar 3: Customer success (case studies, ROI stories, testimonials)
  Pillar 4: Thought leadership (founder POV, contrarian takes, frameworks)
  Pillar 5: Culture & team (hiring, values, behind-the-scenes)
```

**Rule**: Every planned content piece must map to exactly one pillar. If it does not fit, it is off-strategy.

---

## Editorial Calendar Template

### Weekly Calendar (Markdown Table)

```markdown
# Editorial Calendar: Week of [YYYY-MM-DD]
**Strategy Focus**: [brand_awareness / lead_gen / engagement / thought_leadership]
**Content Pillars**: [Pillar 1], [Pillar 2], [Pillar 3]

| Day | Channel | Pillar | Topic | Format | Buyer Stage | Owner | Status |
|-----|---------|--------|-------|--------|-------------|-------|--------|
| Mon | Blog | Product Education | [title] | How-to guide (1500w) | Awareness | [name] | Draft |
| Mon | Twitter | Thought Leadership | [title] | Thread (5 tweets) | Awareness | [name] | Planned |
| Tue | LinkedIn | Customer Success | [title] | Case study post | Consideration | [name] | Planned |
| Wed | Newsletter | Industry Trends | [title] | Curated digest | Awareness | [name] | Planned |
| Thu | Blog | Thought Leadership | [title] | Opinion piece (1000w) | Awareness | [name] | Planned |
| Thu | Twitter | Product Education | [title] | Tip tweet | Consideration | [name] | Planned |
| Fri | LinkedIn | Culture & Team | [title] | Behind-the-scenes | Retention | [name] | Planned |

## Notes
- [Any seasonal events, product launches, or external deadlines to account for]
- [Content dependencies — e.g., case study needs customer approval]
```

### Monthly Calendar (Summary View)

```markdown
# Monthly Content Plan: [Month YYYY]
**Theme**: [overarching monthly theme]

| Week | Theme | Hero/Hub/Help | Key Pieces | Channels |
|------|-------|--------------|------------|----------|
| W1 | [sub-theme] | Hub + Help | Blog guide, 3 tweets, 1 LI post | Blog, Twitter, LinkedIn |
| W2 | [sub-theme] | Help | 2 how-tos, newsletter, 5 tweets | Blog, Email, Twitter |
| W3 | [sub-theme] | Hub + Help | Podcast ep, blog recap, thread | Podcast, Blog, Twitter |
| W4 | [sub-theme] | Hero + Help | Research report, launch post, PR | Blog, All social, Email |
```

---

## Content Brief Template

```markdown
# Content Brief

## Metadata
- **Title**: [working title]
- **Slug**: [url-friendly-slug]
- **Pillar**: [content pillar]
- **Channel**: [primary distribution channel]
- **Format**: [blog post / thread / video / newsletter / podcast / infographic]
- **Buyer Stage**: [Awareness / Consideration / Decision / Retention]
- **Priority**: [P1 / P2 / P3]
- **Due Date**: [YYYY-MM-DD]

## Strategic Alignment
- **Objective**: [specific goal — e.g., "Drive 500 visits to pricing page"]
- **Strategy Focus**: [how this serves the overall strategy_focus]
- **Success Metrics**: [KPIs for this piece]

## Audience
- **Primary Segment**: [who exactly]
- **Pain Point Addressed**: [specific problem]
- **Desired Action**: [what the reader should do after consuming this]

## SEO & Discovery
- **Primary Keyword**: [keyword] — [monthly search volume if known]
- **Secondary Keywords**: [kw1], [kw2], [kw3]
- **Long-tail Variations**: [phrase1], [phrase2]
- **Search Intent**: [informational / navigational / commercial / transactional]

## Key Messages
1. [Core takeaway the reader must remember]
2. [Supporting point with evidence]
3. [Supporting point with evidence]

## Outline
1. **Hook** — [compelling opening approach: question, statistic, story, bold claim]
2. **Context** — [why this matters now]
3. **[Section 1]** — [key points to cover]
4. **[Section 2]** — [key points to cover]
5. **[Section 3]** — [key points to cover]
6. **CTA** — [specific call-to-action aligned with buyer stage]

## Specifications
- **Word Count**: [min]-[max]
- **Tone**: [per brand voice — e.g., "authoritative but conversational"]
- **Visuals**: [required images, charts, screenshots, diagrams]
- **Internal Links**: [related content URLs to link to]
- **External Sources**: [authoritative references to cite]

## Distribution Plan
- **Primary**: [main channel + posting details]
- **Repurpose**: [channel] as [format] by [date]
- **Promotion**: [paid boost? email blast? community share?]

## Competitive Context
- **Competitor coverage**: [how competitors have covered this topic]
- **Our angle**: [what makes our take different or better]
```

---

## Content Audit Methodology

### Audit Inventory Checklist

For each existing content piece, capture:
```
- URL / location
- Title
- Publish date
- Last updated date
- Content pillar (mapped)
- Format (blog, video, etc.)
- Channel (where it lives)
- Word count / length
- Buyer journey stage
- Primary keyword
- Current ranking (if known)
```

### Scoring Rubric (1-5 scale)

| Criterion | 1 (Poor) | 3 (Adequate) | 5 (Excellent) |
|----------|----------|--------------|---------------|
| **Relevance** | Outdated or off-topic | Mostly current, minor gaps | Fully current, directly on-topic |
| **Quality** | Thin, no depth, errors | Solid but generic | Original insights, well-researched |
| **SEO Readiness** | No keywords, poor structure | Keywords present, basic structure | Optimized headings, meta, internal links |
| **CTA Strength** | No CTA or irrelevant CTA | Generic CTA present | Compelling, stage-appropriate CTA |
| **Channel Fit** | Wrong format for channel | Acceptable but not optimized | Native to channel, follows best practices |

**Content Health Score** = Average of all five criteria (1.0 - 5.0).

### Audit Actions by Score

```
4.0 - 5.0  KEEP     — High-performing, maintain and promote
3.0 - 3.9  UPDATE   — Refresh data, improve SEO, strengthen CTA
2.0 - 2.9  REWRITE  — Salvageable topic, needs major revision
1.0 - 1.9  RETIRE   — Remove or consolidate into better content
```

---

## Competitive Content Analysis Framework

### Data Collection Matrix

For each competitor, capture:

```
Competitor: [name]
Website: [url]
Active Channels: [blog, twitter, linkedin, youtube, podcast, newsletter]

Content Inventory:
  Blog frequency: [posts/week]
  Newsletter frequency: [sends/week]
  Social frequency: [posts/day per channel]
  Content formats: [list formats used]

Top-Performing Content:
  1. [title] — [why it works: shareability, SEO rank, engagement]
  2. [title] — [why it works]
  3. [title] — [why it works]

Content Pillars:
  1. [pillar] — [% of their content]
  2. [pillar] — [% of their content]

Strengths: [what they do well]
Weaknesses: [gaps, missed topics, poor formats]
Opportunities: [topics we can own that they ignore]
```

### Competitive Gap Analysis

```
| Topic / Keyword | Us | Competitor A | Competitor B | Opportunity |
|----------------|-----|-------------|-------------|-------------|
| [topic 1] | No content | Strong guide | Weak post | HIGH — create definitive guide |
| [topic 2] | Blog post | No content | Thread | MED — expand and own |
| [topic 3] | Strong guide | Strong guide | Strong guide | LOW — saturated |
```

---

## Content Gap Analysis Techniques

### Buyer Journey Gap Analysis

Map existing content to each stage and identify holes:

```
AWARENESS (top of funnel)
  What we have: [list]
  What's missing: [list]
  Priority gaps: [list]

CONSIDERATION (middle of funnel)
  What we have: [list]
  What's missing: [list]
  Priority gaps: [list]

DECISION (bottom of funnel)
  What we have: [list]
  What's missing: [list]
  Priority gaps: [list]

RETENTION (post-purchase)
  What we have: [list]
  What's missing: [list]
  Priority gaps: [list]
```

### Format Gap Analysis

Check coverage across content formats:

```
| Format | Have? | Count | Quality | Priority to Add |
|--------|-------|-------|---------|-----------------|
| Long-form blog | Yes | 12 | Good | Maintain |
| How-to guides | Yes | 3 | Fair | Expand |
| Case studies | No | 0 | N/A | HIGH |
| Video | No | 0 | N/A | Medium |
| Infographics | No | 0 | N/A | Low |
| Podcast | No | 0 | N/A | Low |
| Templates/Tools | No | 0 | N/A | HIGH |
| Comparison pages | Yes | 1 | Poor | Rewrite |
```

### Keyword Gap Analysis

Identify keywords competitors rank for that you do not:
1. List competitor top-ranking keywords (from web research)
2. Cross-reference with your existing content keywords
3. Prioritize by: search volume, difficulty, buyer intent, strategic fit

---

## Brand Voice Development Guide

### Voice Attributes Framework

Define brand voice with four attribute pairs (spectrum):

```
Formal ←————————→ Casual
  Where do you sit? [1-10 scale]

Serious ←————————→ Playful
  Where do you sit? [1-10 scale]

Authoritative ←————————→ Approachable
  Where do you sit? [1-10 scale]

Technical ←————————→ Simple
  Where do you sit? [1-10 scale]
```

### Voice Documentation Template

```
BRAND VOICE: [one-line summary, e.g., "Confident expert who explains complex topics simply"]

WE ARE:
- [trait 1] — example: "Direct — we get to the point without filler"
- [trait 2] — example: "Evidence-based — we cite sources and use data"
- [trait 3] — example: "Accessible — no jargon without explanation"

WE ARE NOT:
- [anti-trait 1] — example: "Not salesy — we educate, not pitch"
- [anti-trait 2] — example: "Not condescending — we respect the reader's intelligence"
- [anti-trait 3] — example: "Not generic — every piece has a distinct point of view"

VOCABULARY:
  Preferred terms: [list words you use]
  Avoided terms: [list words you never use]

EXAMPLE SENTENCES:
  On-brand: "[example sentence in your voice]"
  Off-brand: "[same idea written in a way you would reject]"
```

---

## Multi-Channel Content Repurposing Strategies

### Repurposing Matrix

From one pillar piece, derive content for every active channel:

```
SOURCE: Long-form blog post (1500+ words)

  → Twitter: 5-tweet thread summarizing key points
  → LinkedIn: 300-word professional insight post
  → Newsletter: Curated excerpt + link + commentary
  → YouTube/Video: 3-5 min explainer script
  → Podcast: Talking points for discussion episode
  → Instagram: Quote card + carousel of key stats
  → SlideShare: 10-slide visual summary
  → Reddit/Community: Discussion post with key finding
```

### Repurposing Rules

1. **Adapt, do not copy** — each channel has native conventions; rewrite for the platform
2. **Lead with the strongest insight** — different channels reward different hooks
3. **Stagger releases** — do not publish everywhere simultaneously; create a 3-5 day drip
4. **Link back** — repurposed content should drive traffic to the original owned asset
5. **Track per channel** — measure performance of each repurposed piece independently

---

## Content Performance KPIs by Channel

### Blog / Website

| KPI | Definition | Benchmark Range |
|-----|-----------|----------------|
| Organic traffic | Sessions from search engines | Track month-over-month growth |
| Time on page | Average reading duration | 2-4 min for 1000-word posts |
| Bounce rate | Single-page sessions | 40-60% is typical for blog |
| Scroll depth | % of page viewed | 50%+ for engaged readers |
| Conversion rate | CTA clicks / page views | 1-3% for blog CTAs |
| Backlinks earned | External sites linking to piece | 5+ for pillar content |

### Email / Newsletter

| KPI | Definition | Benchmark Range |
|-----|-----------|----------------|
| Open rate | Opens / delivered | 20-30% (varies by industry) |
| Click rate | Clicks / delivered | 2-5% |
| Unsubscribe rate | Unsubs / delivered | < 0.5% per send |
| List growth rate | Net new subscribers / month | 2-5% monthly |
| Forward rate | Forwards / delivered | 0.5-1% |

### Social Media (Twitter, LinkedIn, etc.)

| KPI | Definition | Benchmark Range |
|-----|-----------|----------------|
| Engagement rate | (likes + replies + shares) / impressions | 1-3% organic |
| Follower growth | Net new followers / month | Track trend, not absolute |
| Click-through rate | Link clicks / impressions | 0.5-2% |
| Share rate | Shares / impressions | 0.1-0.5% |
| Reply rate | Replies / impressions | Higher = better engagement |

### Content ROI Formula

```
Content ROI = (Revenue attributed to content - Content production cost) / Content production cost x 100

For non-revenue goals, use proxy metrics:
  Brand Awareness ROI = (Impressions x Estimated CPM value) / Production cost
  Lead Gen ROI = (Leads generated x Average lead value) / Production cost
  Engagement ROI = (Engaged users x Estimated engagement value) / Production cost
```
