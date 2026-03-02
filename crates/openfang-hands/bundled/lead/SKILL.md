---
name: lead-hand-skill
version: "1.0.0"
description: "Expert knowledge for AI lead generation — web research, enrichment, scoring, deduplication, and report generation"
runtime: prompt_only
---

# Lead Generation Expert Knowledge

## Ideal Customer Profile (ICP) Construction

A good ICP answers these questions:
1. **Industry**: What vertical does your ideal customer operate in?
2. **Company size**: How many employees? What revenue range?
3. **Geography**: Where are they located?
4. **Technology**: What tech stack do they use?
5. **Budget signals**: Are they funded? Growing? Hiring?
6. **Decision-maker**: Who has buying authority? (title, seniority)
7. **Pain points**: What problems does your product solve for them?

### Company Size Categories
| Category | Employees | Typical Budget | Sales Cycle |
|----------|-----------|---------------|-------------|
| Startup | 1-50 | $1K-$25K/yr | 1-4 weeks |
| SMB | 50-500 | $25K-$250K/yr | 1-3 months |
| Enterprise | 500+ | $250K+/yr | 3-12 months |

---

## Web Research Techniques for Lead Discovery

### Search Query Patterns
```
# Find companies in a vertical
"[industry] companies" site:crunchbase.com
"top [industry] startups [year]"
"[industry] companies [city/region]"

# Find decision-makers
"[title]" "[company]" site:linkedin.com
"[company] team" OR "[company] about us" OR "[company] leadership"

# Growth signals (high-intent leads)
"[company] hiring [role]" — indicates budget and growth
"[company] series [A/B/C]" — recently funded
"[company] expansion" OR "[company] new office"
"[company] product launch [year]"

# Technology signals
"[company] uses [technology]" OR "[company] built with [technology]"
site:stackshare.io "[company]"
site:builtwith.com "[company]"
```

### Source Quality Ranking
1. **Company website** (About/Team pages) — most reliable for personnel
2. **Crunchbase** — funding, company details, leadership
3. **LinkedIn** (public profiles) — titles, tenure, connections
4. **Press releases** — announcements, partnerships, funding
5. **Job boards** — hiring signals, tech stack requirements
6. **Industry directories** — comprehensive company lists
7. **News articles** — recent activity, reputation
8. **Social media** — engagement, company culture

---

## Lead Enrichment Patterns

### Basic Enrichment (always available)
- Full name (first + last)
- Job title
- Company name
- Company website URL

### Standard Enrichment
- Company employee count (from About page, Crunchbase, or LinkedIn)
- Company industry classification
- Company founding year
- Technology stack (from job postings, StackShare, BuiltWith)
- Social profiles (LinkedIn URL, Twitter handle)
- Company description (from meta tags or About page)

### Deep Enrichment
- Recent funding rounds (amount, investors, date)
- Recent news mentions (last 90 days)
- Key competitors
- Estimated revenue range
- Recent job postings (growth signals)
- Company blog/content activity (engagement level)
- Executive team changes

### Email Pattern Discovery
Common formats: `firstname@`, `firstname.lastname@`, `flastname@`, `firstnamel@` — try in that order.
NEVER send unsolicited emails. Patterns are for reference only.

---

## Lead Scoring Framework

### Scoring Rubric (0-100)
```
ICP Match (30 points max):
  Industry match:     +10
  Company size match: +5
  Geography match:    +5
  Role/title match:   +10

Growth Signals (20 points max):
  Recent funding:     +8
  Actively hiring:    +6
  Product launch:     +3
  Press coverage:     +3

Enrichment Quality (20 points max):
  Email found:        +5
  LinkedIn found:     +5
  Full company data:  +5
  Tech stack known:   +5

Recency (15 points max):
  Active this month:  +15
  Active this quarter:+10
  Active this year:   +5
  No recent activity: +0

Accessibility (15 points max):
  Direct contact:     +15
  Company contact:    +10
  Social only:        +5
  No contact info:    +0
```

### Score Interpretation
| Score | Grade | Action |
|-------|-------|--------|
| 80-100 | A | Hot lead — prioritize outreach |
| 60-79 | B | Warm lead — nurture |
| 40-59 | C | Cool lead — enrich further |
| 0-39 | D | Cold lead — deprioritize |

---

## Deduplication Strategies

### Matching Algorithm
1. **Exact match**: Normalize company name (lowercase, strip Inc/LLC/Ltd) + person name
2. **Fuzzy match**: Levenshtein distance < 2 on company name + same person
3. **Domain match**: Same company website domain = same company
4. **Cross-source merge**: Same person at same company from different sources → merge enrichment data

### Normalization Rules
```
Company name:
  - Strip legal suffixes: Inc, LLC, Ltd, Corp, Co, GmbH, AG, SA
  - Lowercase
  - Remove "The" prefix
  - Collapse whitespace

Person name:
  - Lowercase
  - Remove middle names/initials
  - Handle "Bob" = "Robert", "Mike" = "Michael" (common nicknames)
```

---

## Output Format Templates

Support CSV, JSON, and Markdown table output. All formats include these fields:
`name, title, company, company_url, linkedin, industry, company_size, employee_count, score, discovered, enrichment (funding, hiring, tech_stack, recent_news), notes`

Use the format the user requests. Default to Markdown table for reports, JSON for programmatic use, CSV for spreadsheet import.

---

## Compliance & Ethics

### DO
- Use only publicly available information
- Respect robots.txt and rate limits
- Include data provenance (where each piece of info came from)
- Allow users to export and delete their lead data
- Clearly mark confidence levels on enriched data

### DO NOT
- Scrape behind login walls or paywalls
- Fabricate any lead data (even "likely" email addresses without evidence)
- Store sensitive personal data (SSN, financial info, health data)
- Send unsolicited communications on behalf of the user
- Bypass anti-scraping measures (CAPTCHAs, rate limits)
- Collect data on individuals who have opted out of data collection

### Data Retention
- Keep lead data in local files only — never exfiltrate
- Mark stale leads (>90 days without activity) for review
- Provide clear data export in all supported formats
