---
name: researcher-hand-skill
version: "1.0.0"
description: "Expert knowledge for AI deep research — methodology, source evaluation, search optimization, cross-referencing, synthesis, and citation formats"
runtime: prompt_only
---

# Deep Research Expert Knowledge

## Research Methodology

### Research Process (5 phases)
1. **Define**: Clarify the question, identify what's known vs unknown, set scope
2. **Search**: Systematic multi-strategy search across diverse sources
3. **Evaluate**: Assess source quality, extract relevant data, note limitations
4. **Synthesize**: Combine findings into coherent answer, resolve contradictions
5. **Verify**: Cross-check critical claims, identify remaining uncertainties

### Question Types & Strategies
| Question Type | Strategy | Example |
|--------------|----------|---------|
| Factual | Find authoritative primary source | "What is the population of Tokyo?" |
| Comparative | Multi-source balanced analysis | "React vs Vue for large apps?" |
| Causal | Evidence chain + counterfactuals | "Why did Theranos fail?" |
| Predictive | Trend analysis + expert consensus | "Will quantum computing replace classical?" |
| How-to | Step-by-step from practitioners | "How to set up a Kubernetes cluster?" |
| Survey | Comprehensive landscape mapping | "What are the options for vector databases?" |
| Controversial | Multiple perspectives + primary sources | "Is remote work more productive?" |

### Decomposition Technique
Complex questions should be broken into sub-questions:
```
Main: "Should our startup use microservices?"
Sub-questions:
  1. What are microservices? (definitional)
  2. What are the benefits vs monolith? (comparative)
  3. What team size/stage is appropriate? (contextual)
  4. What are the operational costs? (factual)
  5. What do similar startups use? (case studies)
  6. What are the migration paths? (how-to)
```

---

## CRAAP Source Evaluation Framework

Evaluate each source on 5 dimensions: **Currency** (recent enough?), **Relevance** (addresses the question?), **Authority** (credible author/institution?), **Accuracy** (evidence-backed, verifiable?), **Purpose** (informational vs commercial/persuasive?).

Scoring: A = passes all 5, B = 4/5, C = 3/5 (use with caveats), D = 2/5 or fewer, F = unreliable (do not cite).
For tech topics: anything >2 years old may be outdated. Prefer .gov/.edu/reputable org domains.

---

## Search Query Optimization

### Query Construction Techniques

**Exact phrase**: `"specific phrase"` — use for names, quotes, error messages
**Site-specific**: `site:domain.com query` — search within a specific site
**Exclude**: `query -unwanted_term` — remove irrelevant results
**File type**: `filetype:pdf query` — find specific document types
**Recency**: `query after:2024-01-01` — recent results only
**OR operator**: `query (option1 OR option2)` — broaden search
**Wildcard**: `"how to * in python"` — fill-in-the-blank

### Multi-Strategy Search Pattern
For each research question, use at least 3 search strategies:
1. **Direct**: The question as-is
2. **Authoritative**: `site:gov OR site:edu OR site:org [topic]`
3. **Academic**: `[topic] research paper [year]` or `site:arxiv.org [topic]`
4. **Practical**: `[topic] guide` or `[topic] tutorial` or `[topic] how to`
5. **Data**: `[topic] statistics` or `[topic] data [year]`
6. **Contrarian**: `[topic] criticism` or `[topic] problems` or `[topic] myths`

### Source Discovery by Domain
| Domain | Best Sources | Search Pattern |
|--------|-------------|---------------|
| Technology | Official docs, GitHub, Stack Overflow, engineering blogs | `[tech] documentation`, `site:github.com [tech]` |
| Science | PubMed, arXiv, Nature, Science | `site:arxiv.org [topic]`, `[topic] systematic review` |
| Business | SEC filings, industry reports, HBR | `[company] 10-K`, `[industry] report [year]` |
| Medicine | PubMed, WHO, CDC, Cochrane | `site:pubmed.ncbi.nlm.nih.gov [topic]` |
| Legal | Court records, law reviews, statute databases | `[case] ruling`, `[law] analysis` |
| Statistics | Census, BLS, World Bank, OECD | `site:data.worldbank.org [metric]` |
| Current events | Reuters, AP, BBC, primary sources | `[event] statement`, `[event] official` |

---

## Cross-Referencing Techniques

### Verification Levels
```
Level 1: Single source (unverified)
  → Mark as "reported by [source]"

Level 2: Two independent sources agree (corroborated)
  → Mark as "confirmed by multiple sources"

Level 3: Primary source + secondary confirmation (verified)
  → Mark as "verified — primary source: [X]"

Level 4: Expert consensus (well-established)
  → Mark as "widely accepted" or "scientific consensus"
```

### Contradiction Resolution
When sources disagree:
1. Check which source is more authoritative (CRAAP scores)
2. Check which is more recent (newer may have updated info)
3. Check if they're measuring different things (apples vs oranges)
4. Check for known biases or conflicts of interest
5. Present both views with evidence for each
6. State which view the evidence better supports (if clear)
7. If genuinely uncertain, say so — don't force a conclusion

---

## Synthesis Patterns

### Narrative Synthesis
```
The evidence suggests [main finding].

[Source A] found that [finding 1], which is consistent with
[Source B]'s observation that [finding 2]. However, [Source C]
presents a contrasting view: [finding 3].

The weight of evidence favors [conclusion] because [reasoning].
A key limitation is [gap or uncertainty].
```

### Structured Synthesis
```
FINDING 1: [Claim]
  Evidence for: [Source A], [Source B] — [details]
  Evidence against: [Source C] — [details]
  Confidence: [high/medium/low]
  Reasoning: [why the evidence supports this finding]

FINDING 2: [Claim]
  ...
```

### Gap Analysis
After synthesis, explicitly note:
- What questions remain unanswered?
- What data would strengthen the conclusions?
- What are the limitations of the available sources?
- What follow-up research would be valuable?

---

## Citation Formats

Use the format matching the user's `citation_style` setting:
- **Inline URL**: `(https://url)` after the claim
- **Footnotes**: `[1]` inline, `[1] URL — "Title" by Author, Date` at bottom
- **APA**: `(Author, Year)` inline, full ref: `Author, A. (Year). Title. *Journal*, Vol(Issue), Pages. URL`
- **Numbered**: `[1]` inline, numbered list at end

---

## Output Templates

**Brief Report** sections: Question + metadata (date, source count, confidence) → Answer (2-3 paragraphs) → Key Evidence (bulleted findings with sources) → Caveats → Sources list.

**Detailed Report** sections: Executive Summary → Background → Methodology → Findings (one subsection per sub-question) → Analysis (synthesis, patterns) → Contradictions & Open Questions → Confidence Assessment → Full bibliography.

---

## Cognitive Bias Checklist

Before finalizing research, check for: **confirmation bias** (search for disconfirming evidence), **authority bias** (evaluate evidence, not prestige), **anchoring** (gather multiple sources before concluding), **selection bias** (vary search strategies), **recency bias** (include foundational sources), **framing effect** (look at raw data, not just interpretations).

---

## Domain-Specific Research Tips

### Technology Research
- Always check the official documentation first
- Compare documentation version with the latest release
- Stack Overflow answers may be outdated — check the date
- GitHub issues/discussions often have the most current information
- Benchmarks without methodology descriptions are unreliable

### Business Research
- SEC filings (10-K, 10-Q) are the most reliable public company data
- Press releases are marketing — verify claims independently
- Analyst reports may have conflicts of interest — check disclaimers
- Employee reviews (Glassdoor) provide internal perspective but are biased

### Scientific Research
- Systematic reviews and meta-analyses are strongest evidence
- Single studies should not be treated as definitive
- Check if findings have been replicated
- Preprints have not been peer-reviewed — note this caveat
- p-values and effect sizes both matter — not just "statistically significant"
