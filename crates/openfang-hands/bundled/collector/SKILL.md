---
name: collector-hand-skill
version: "1.0.0"
description: "Expert knowledge for AI intelligence collection — OSINT methodology, entity extraction, knowledge graphs, change detection, and sentiment analysis"
runtime: prompt_only
---

# Intelligence Collection Expert Knowledge

## OSINT Methodology

### Collection Cycle
1. **Planning**: Define target, scope, and collection requirements
2. **Collection**: Gather raw data from open sources
3. **Processing**: Extract entities, relationships, and data points
4. **Analysis**: Synthesize findings, identify patterns, detect changes
5. **Dissemination**: Generate reports, alerts, and updates
6. **Feedback**: Refine queries based on what worked and what didn't

### Source Categories (by reliability)
| Tier | Source Type | Reliability | Examples |
|------|-----------|-------------|---------|
| 1 | Official/Primary | Very High | Company filings, government data, press releases |
| 2 | Institutional | High | News agencies (Reuters, AP), research institutions |
| 3 | Professional | Medium-High | Industry publications, analyst reports, expert blogs |
| 4 | Community | Medium | Forums, social media, review sites |
| 5 | Anonymous/Unverified | Low | Anonymous posts, rumors, unattributed claims |

### Search Query Construction by Focus Area

**Market Intelligence**:
```
"[target] market share"
"[target] industry report [year]"
"[target] TAM SAM SOM"
"[target] growth rate"
"[target] market analysis"
"[target industry] trends [year]"
```

**Business Intelligence**:
```
"[company] revenue" OR "[company] earnings"
"[company] CEO" OR "[company] leadership team"
"[company] strategy" OR "[company] roadmap"
"[company] partnerships" OR "[company] acquisition"
"[company] annual report" OR "[company] 10-K"
site:sec.gov "[company]"
```

**Competitor Analysis**:
```
"[company] vs [competitor]"
"[company] alternative"
"[company] review" OR "[company] comparison"
"[company] pricing" site:g2.com OR site:capterra.com
"[company] customer reviews" site:trustpilot.com
"switch from [company] to"
```

**Person Tracking**:
```
"[person name]" "[company]"
"[person name]" interview OR podcast OR keynote
"[person name]" site:linkedin.com
"[person name]" publication OR paper
"[person name]" conference OR summit
```

**Technology Monitoring**:
```
"[technology] release" OR "[technology] update"
"[technology] benchmark [year]"
"[technology] adoption" OR "[technology] usage statistics"
"[technology] vs [alternative]"
"[technology]" site:github.com
"[technology] roadmap" OR "[technology] changelog"
```

---

## Entity Extraction Patterns

### Named Entity Types
1. **Person**: Name, title, organization, role
2. **Organization**: Company name, type, industry, location, size
3. **Product**: Product name, company, category, version
4. **Event**: Type, date, participants, location, significance
5. **Financial**: Amount, currency, type (funding, revenue, valuation)
6. **Technology**: Name, version, category, vendor
7. **Location**: City, state, country, region
8. **Date/Time**: Specific dates, time ranges, deadlines

### Extraction Heuristics
- **Person detection**: Title + Name pattern ("CEO John Smith"), bylines, quoted speakers
- **Organization detection**: Legal suffixes (Inc, LLC), "at [Company]", domain names
- **Financial detection**: Currency symbols, "raised $X", "valued at", "revenue of"
- **Event detection**: Date + verb ("launched on", "announced at", "acquired")
- **Technology detection**: CamelCase names, version numbers, "built with", "powered by"

---

## Knowledge Graph Best Practices

### Entity Schema
```json
{
  "entity_id": "unique_id",
  "name": "Entity Name",
  "type": "person|company|product|event|technology",
  "attributes": {
    "key": "value"
  },
  "sources": ["url1", "url2"],
  "first_seen": "timestamp",
  "last_seen": "timestamp",
  "confidence": "high|medium|low"
}
```

### Relation Schema
```json
{
  "source_entity": "entity_id_1",
  "relation": "works_at|founded|competes_with|...",
  "target_entity": "entity_id_2",
  "attributes": {
    "since": "date",
    "context": "description"
  },
  "source": "url",
  "confidence": "high|medium|low"
}
```

### Common Relations
| Relation | Between | Example |
|----------|---------|---------|
| works_at | Person → Company | "Jane Smith works at Acme" |
| founded | Person → Company | "John Doe founded StartupX" |
| invested_in | Company → Company | "VC Fund invested in StartupX" |
| competes_with | Company → Company | "Acme competes with BetaCo" |
| partnered_with | Company → Company | "Acme partnered with CloudY" |
| launched | Company → Product | "Acme launched ProductZ" |
| acquired | Company → Company | "BigCorp acquired StartupX" |
| uses | Company → Technology | "Acme uses Kubernetes" |
| mentioned_in | Entity → Source | "Acme mentioned in TechCrunch" |

---

## Change Detection Methodology

### Snapshot Comparison
1. Store the current state of all entities as a JSON snapshot
2. On next collection cycle, compare new state against previous snapshot
3. Classify changes:

| Change Type | Significance | Example |
|-------------|-------------|---------|
| Entity appeared | Varies | New competitor enters market |
| Entity disappeared | Important | Company goes quiet, product deprecated |
| Attribute changed | Critical-Minor | CEO changed (critical), address changed (minor) |
| New relation | Important | New partnership, acquisition, hiring |
| Relation removed | Important | Person left company, partnership ended |
| Sentiment shift | Important | Positive→Negative media coverage |

### Significance Scoring
```
CRITICAL (immediate alert):
  - Leadership change (CEO, CTO, board)
  - Acquisition or merger
  - Major funding round (>$10M)
  - Product discontinuation
  - Legal action or regulatory issue

IMPORTANT (include in next report):
  - New product launch
  - New partnership or integration
  - Hiring surge (>5 roles)
  - Pricing change
  - Competitor move
  - Major customer win/loss

MINOR (note in report):
  - Blog post or press mention
  - Minor update or patch
  - Social media activity spike
  - Conference appearance
  - Job posting (individual)
```

---

## Sentiment Analysis Heuristics

When `track_sentiment` is enabled, score each source: +2 (strong positive: awards, breakthroughs) → +1 (mild: steady growth) → 0 (neutral: factual reporting) → -1 (mild negative: increased competition) → -2 (strong negative: breach, lawsuit, layoffs).

Track rolling average over last 5 collection cycles to detect trends.

---

## Report Templates

### Intelligence Brief
Sections: metadata (date, cycle #, sources, new data points) → Priority Changes (CRITICAL/IMPORTANT items with sources) → Executive Summary (2-3 paragraphs) → Detailed Findings by category (findings with source links + confidence) → Entity Updates table (entity, change, previous, current, source) → Sentiment Trend table → Collection Metadata (queries, sources fetched, new/updated entities, next scheduled).

---

## Source Evaluation Checklist

Before including data in the knowledge graph, evaluate:

1. **Recency**: Published within relevant timeframe? Stale data can mislead.
2. **Primary vs Secondary**: Is this the original source, or citing someone else?
3. **Corroboration**: Do other independent sources confirm this?
4. **Bias check**: Does the source have a financial or political interest in this claim?
5. **Specificity**: Does it provide concrete data, or vague assertions?
6. **Track record**: Has this source been reliable in the past?

If a claim fails 3+ checks, downgrade its confidence to "low".
