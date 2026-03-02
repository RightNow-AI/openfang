---
name: predictor-hand-skill
version: "1.0.0"
description: "Expert knowledge for AI forecasting — superforecasting principles, signal taxonomy, confidence calibration, reasoning chains, and accuracy tracking"
runtime: prompt_only
---

# Forecasting Expert Knowledge

## Superforecasting Principles

Based on research by Philip Tetlock and the Good Judgment Project:

1. **Triage**: Focus on questions that are hard enough to be interesting but not so hard they're unknowable
2. **Break problems apart**: Decompose big questions into smaller, researchable sub-questions (Fermi estimation)
3. **Balance inside and outside views**: Use both specific evidence AND base rates from reference classes
4. **Update incrementally**: Adjust predictions in small steps as new evidence arrives (Bayesian updating)
5. **Look for clashing forces**: Identify factors pulling in opposite directions
6. **Distinguish signal from noise**: Weight signals by their reliability and relevance
7. **Calibrate**: Your 70% predictions should come true ~70% of the time
8. **Post-mortem**: Analyze why predictions went wrong, not just celebrate the right ones
9. **Avoid the narrative trap**: A compelling story is not the same as a likely outcome
10. **Collaborate**: Aggregate views from diverse perspectives

---

## Signal Taxonomy

### Signal Types
| Type | Description | Weight | Example |
|------|-----------|--------|---------|
| Leading indicator | Predicts future movement | High | Job postings surge → company expanding |
| Lagging indicator | Confirms past movement | Medium | Quarterly earnings → business health |
| Base rate | Historical frequency | High | "80% of startups fail within 5 years" |
| Expert opinion | Informed prediction | Medium | Analyst forecast, CEO statement |
| Data point | Factual measurement | High | Revenue figure, user count, benchmark |
| Anomaly | Deviation from pattern | High | Unusual trading volume, sudden hiring freeze |
| Structural change | Systemic shift | Very High | New regulation, technology breakthrough |
| Sentiment shift | Collective mood change | Medium | Media tone change, social media trend |

### Signal Strength Assessment
```
STRONG signal (high predictive value):
  - Multiple independent sources confirm
  - Quantitative data (not just opinions)
  - Leading indicator with historical track record
  - Structural change with clear causal mechanism

MODERATE signal (some predictive value):
  - Single authoritative source
  - Expert opinion from domain specialist
  - Historical pattern that may or may not repeat
  - Lagging indicator (confirms direction)

WEAK signal (limited predictive value):
  - Social media buzz without substance
  - Single anecdote or case study
  - Rumor or unconfirmed report
  - Opinion from non-specialist
```

---

## Confidence Calibration

### Probability Scale
Use concrete probabilities: 95% (almost certain), 80% (likely), 60% (slightly more likely), 50% (toss-up), 30% (unlikely but plausible), 10% (extremely unlikely), 5% (almost impossible). Never use 0% or 100%.

### Calibration Rules
1. NEVER use 0% or 100% — nothing is absolutely certain
2. If you haven't done research, default to the base rate (outside view)
3. Your first estimate should be the reference class base rate
4. Adjust from the base rate using specific evidence (inside view)
5. Typical adjustment: ±5-15% per strong signal, ±2-5% per moderate signal
6. If your gut says 80% but your analysis says 55%, trust the analysis

### Brier Score
The gold standard for measuring prediction accuracy:
```
Brier Score = (predicted_probability - actual_outcome)^2

actual_outcome = 1 if prediction came true, 0 if not

Perfect score: 0.0 (you're always right with perfect confidence)
Coin flip: 0.25 (saying 50% on everything)
Terrible: 1.0 (100% confident, always wrong)

Good forecaster: < 0.15
Average forecaster: 0.20-0.30
Bad forecaster: > 0.35
```

---

## Domain-Specific Source Guide

| Domain | Key Sources | Best Signals |
|--------|------------|-------------|
| Technology | GitHub issues, NPM/PyPI downloads, Stack Overflow surveys, Crunchbase, job postings | Adoption curves, funding rounds, hiring surges |
| Finance | FRED, SEC filings, earnings calls, Bloomberg, VIX, Fed minutes | Economic indicators, earnings revisions, central bank signals |
| Geopolitics | Government statements, RAND/Brookings, polls, WTO/SIPRI data | Policy shifts, election data, trade/military movements |
| Climate | IPCC, IEA, COP agreements, CDP disclosures, BloombergNEF | Scientific consensus, energy transition metrics, policy commitments |

---

## Reasoning Chain Construction

### Template
```
PREDICTION: [Specific, falsifiable claim]

1. REFERENCE CLASS (Outside View)
   Base rate: [What % of similar events occur?]
   Reference examples: [3-5 historical analogues]

2. SPECIFIC EVIDENCE (Inside View)
   Signals FOR (+):
   a. [Signal] — strength: [strong/moderate/weak] — adjustment: +X%
   b. [Signal] — strength: [strong/moderate/weak] — adjustment: +X%

   Signals AGAINST (-):
   a. [Signal] — strength: [strong/moderate/weak] — adjustment: -X%
   b. [Signal] — strength: [strong/moderate/weak] — adjustment: -X%

3. SYNTHESIS
   Starting probability (base rate): X%
   Net adjustment: +/-Y%
   Final probability: Z%

4. KEY ASSUMPTIONS
   - [Assumption 1]: If wrong, probability shifts to [W%]
   - [Assumption 2]: If wrong, probability shifts to [V%]

5. RESOLUTION
   Date: [When can this be resolved?]
   Criteria: [Exactly how to determine if correct]
   Data source: [Where to check the outcome]
```

---

## Prediction Tracking & Scoring

### Prediction Ledger Format
```json
{
  "id": "pred_001",
  "created": "2025-01-15",
  "prediction": "OpenAI will release GPT-5 before July 2025",
  "confidence": 0.65,
  "domain": "tech",
  "time_horizon": "2025-07-01",
  "reasoning_chain": "...",
  "key_signals": ["leaked roadmap", "compute scaling", "hiring patterns"],
  "status": "active|resolved|expired",
  "resolution": {
    "date": "2025-06-30",
    "outcome": true,
    "evidence": "Released June 15, 2025",
    "brier_score": 0.1225
  },
  "updates": [
    {"date": "2025-03-01", "new_confidence": 0.75, "reason": "New evidence: leaked demo"}
  ]
}
```

### Accuracy Report Template
```
ACCURACY DASHBOARD
==================
Total predictions:     N
Resolved predictions:  N (N correct, N incorrect, N partial)
Active predictions:    N
Expired (unresolvable):N

Overall accuracy:      X%
Brier score:           0.XX

Calibration:
  Predicted 90%+ → Actual: X% (N predictions)
  Predicted 70-89% → Actual: X% (N predictions)
  Predicted 50-69% → Actual: X% (N predictions)
  Predicted 30-49% → Actual: X% (N predictions)
  Predicted <30% → Actual: X% (N predictions)

Strengths: [domains/types where you perform well]
Weaknesses: [domains/types where you perform poorly]
```

---

## Cognitive Bias Checklist

Before finalizing, check: **anchoring** (start from base rate, not first number seen), **availability** (check actual frequency, not memorability), **confirmation** (steel-man the opposite), **narrative** (boring predictions are often more accurate), **overconfidence** (if never wrong at this confidence, you're overconfident), **scope insensitivity** (be specific about magnitudes), **recency** (check longer horizons, mean reversion), **status quo** (consider structural breaks).

### Contrarian Mode
When enabled, for each consensus prediction:
1. Identify what the consensus view is
2. Search for evidence the consensus is wrong
3. Consider: "What would have to be true for the opposite to happen?"
4. If credible contrarian evidence exists, include a contrarian prediction
5. Always label contrarian predictions clearly with the consensus for comparison
