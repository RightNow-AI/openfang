---
name: twitter-hand-skill
version: "1.0.0"
description: "Expert knowledge for AI Twitter/X management — API v2 reference, content strategy, engagement playbook, safety, and performance tracking"
runtime: prompt_only
---

# Twitter/X Management Expert Knowledge

## Twitter API v2 Reference

### Authentication
Twitter API v2 uses OAuth 2.0 Bearer Token for app-level access and OAuth 1.0a for user-level actions.

**Bearer Token** (read-only access + tweet creation):
```
Authorization: Bearer $TWITTER_BEARER_TOKEN
```

**Environment variable**: `TWITTER_BEARER_TOKEN`

### Core Endpoints

**Get authenticated user info**:
```bash
curl -s -H "Authorization: Bearer $TWITTER_BEARER_TOKEN" \
  "https://api.twitter.com/2/users/me"
```
Response: `{"data": {"id": "123", "name": "User", "username": "user"}}`

**Post a tweet**:
```bash
curl -s -X POST "https://api.twitter.com/2/tweets" \
  -H "Authorization: Bearer $TWITTER_BEARER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"text": "Hello world!"}'
```
Response: `{"data": {"id": "tweet_id", "text": "Hello world!"}}`

**Post a reply**:
```bash
curl -s -X POST "https://api.twitter.com/2/tweets" \
  -H "Authorization: Bearer $TWITTER_BEARER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"text": "Great point!", "reply": {"in_reply_to_tweet_id": "PARENT_TWEET_ID"}}'
```

**Post a thread** (chain of replies to yourself):
1. Post first tweet → get `tweet_id`
2. Post second tweet with `reply.in_reply_to_tweet_id` = first tweet_id
3. Repeat for each tweet in thread

**Delete a tweet**:
```bash
curl -s -X DELETE "https://api.twitter.com/2/tweets/TWEET_ID" \
  -H "Authorization: Bearer $TWITTER_BEARER_TOKEN"
```

**Like a tweet**:
```bash
curl -s -X POST "https://api.twitter.com/2/users/USER_ID/likes" \
  -H "Authorization: Bearer $TWITTER_BEARER_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"tweet_id": "TARGET_TWEET_ID"}'
```

**Get mentions**:
```bash
curl -s -H "Authorization: Bearer $TWITTER_BEARER_TOKEN" \
  "https://api.twitter.com/2/users/USER_ID/mentions?max_results=10&tweet.fields=public_metrics,created_at,author_id"
```

**Search recent tweets**:
```bash
curl -s -H "Authorization: Bearer $TWITTER_BEARER_TOKEN" \
  "https://api.twitter.com/2/tweets/search/recent?query=QUERY&max_results=10&tweet.fields=public_metrics"
```

**Get tweet metrics**:
```bash
curl -s -H "Authorization: Bearer $TWITTER_BEARER_TOKEN" \
  "https://api.twitter.com/2/tweets?ids=ID1,ID2,ID3&tweet.fields=public_metrics"
```
Response includes: `retweet_count`, `reply_count`, `like_count`, `quote_count`, `bookmark_count`, `impression_count`

### Rate Limits
| Endpoint | Limit | Window |
|----------|-------|--------|
| POST /tweets | 300 tweets | 3 hours |
| DELETE /tweets | 50 deletes | 15 minutes |
| POST /likes | 50 likes | 15 minutes |
| GET /mentions | 180 requests | 15 minutes |
| GET /search/recent | 180 requests | 15 minutes |

Always check response headers:
- `x-rate-limit-limit`: Total requests allowed
- `x-rate-limit-remaining`: Requests remaining
- `x-rate-limit-reset`: Unix timestamp when limit resets

---

## Content Strategy Framework

### Content Pillars
Define 3-5 core topics ("pillars") that all content revolves around:
```
Example for a tech founder:
  Pillar 1: AI & Machine Learning (40% of content)
  Pillar 2: Startup Building (30% of content)
  Pillar 3: Engineering Culture (20% of content)
  Pillar 4: Personal Growth (10% of content)
```

### Content Mix (7 types)
| Type | Frequency | Purpose | Template |
|------|-----------|---------|----------|
| Hot take | 2-3/week | Engagement | "Unpopular opinion: [contrarian view]" |
| Thread | 1-2/week | Authority | "I spent X hours researching Y. Here's what I found:" |
| Tip/How-to | 2-3/week | Value | "How to [solve problem] in [N] steps:" |
| Question | 1-2/week | Engagement | "[Interesting question]? I'll go first:" |
| Curated share | 1-2/week | Curation | "This [article/tool/repo] is a game changer for [audience]:" |
| Story | 1/week | Connection | "3 years ago I [relatable experience]. Here's what happened:" |
| Data/Stat | 1/week | Authority | "[Surprising statistic]. Here's why it matters:" |

### Optimal Posting Times (UTC-based, adjust to audience timezone)
| Day | Best Times | Why |
|-----|-----------|-----|
| Monday | 8-10 AM | Start of work week, checking feeds |
| Tuesday | 10 AM, 1 PM | Peak engagement day |
| Wednesday | 9 AM, 12 PM | Mid-week focus |
| Thursday | 10 AM, 2 PM | Second-highest engagement day |
| Friday | 9-11 AM | Morning only, engagement drops PM |
| Saturday | 10 AM | Casual browsing |
| Sunday | 4-6 PM | Pre-work-week planning |

---

## Tweet Writing Best Practices

### The Hook (first line is everything)
Hooks that work:
- **Contrarian**: "Most people think X. They're wrong."
- **Number**: "I analyzed 500 [things]. Here's what I found:"
- **Question**: "Why do 90% of [things] fail?"
- **Story**: "In 2019, I almost [dramatic thing]."
- **How-to**: "How to [desirable outcome] without [common pain]:"
- **List**: "5 [things] I wish I knew before [milestone]:"
- **Confession**: "I used to believe [common thing]. Then I learned..."

### Writing Rules
1. **One idea per tweet** — don't try to cover everything
2. **Front-load value** — the hook must deliver or promise value
3. **Use line breaks** — no wall of text, 1-2 sentences per line
4. **280 character limit** — every word must earn its place
5. **Active voice** — "We shipped X" not "X was shipped by us"
6. **Specific > vague** — "3x faster" not "much faster"
7. **End with a call to action** — "Agree? RT" or "What would you add?"

### Thread Structure
```
Tweet 1 (HOOK): Compelling opening that makes people click "Show this thread"
  - Must stand alone as a great tweet
  - End with "A thread:" or "Here's what I found:"

Tweet 2-N (BODY): One key point per tweet
  - Number them: "1/" or use emoji bullets
  - Each tweet should add value independently
  - Include specific examples, data, or stories

Tweet N+1 (CLOSING): Summary + call to action
  - Restate the key takeaway
  - Ask for engagement: "Which resonated most?"
  - Self-reference: "If this was useful, follow @handle for more"
```

### Hashtag Strategy
- **0-2 hashtags** per tweet (more looks spammy)
- Use hashtags for discovery, not decoration
- Mix broad (#AI) and specific (#LangChain)
- Never use hashtags in threads (except maybe tweet 1)
- Research trending hashtags in your niche before using them

---

## Engagement Playbook

### Replying to Mentions
Rules:
1. **Respond within 2 hours** during engagement_hours
2. **Add value** — don't just say "thanks!" — expand on their point
3. **Ask a follow-up question** — drives conversation
4. **Be genuine** — match their energy and tone
5. **Never argue** — if someone is hostile, ignore or block

Reply templates:
- Agreement: "Great point! I'd also add [related insight]"
- Question: "Interesting question. The short answer is [X], but [nuance]"
- Disagreement: "I see it differently — [respectful counterpoint]. What's your experience?"
- Gratitude: "Appreciate you sharing this! [Specific thing you liked about their tweet]"

### When NOT to Engage
- Trolls or obviously bad-faith arguments
- Political flame wars (unless that's your content pillar)
- Personal attacks (block immediately)
- Spam or bot accounts
- Tweets that could create legal liability

### Auto-Like Strategy
Like tweets from:
1. People who regularly engage with your content (reciprocity)
2. Influencers in your niche (visibility)
3. Thoughtful content related to your pillars (curation signal)
4. Replies to your tweets (encourages more replies)

Do NOT auto-like:
- Controversial or political content
- Content you haven't actually read
- Spam or low-quality threads
- Competitor criticism (looks petty)

---

## Content Calendar Template

```
WEEK OF [DATE]

Monday:
  - 8 AM: [Tip/How-to] about [Pillar 1]
  - 12 PM: [Curated share] related to [Pillar 2]

Tuesday:
  - 10 AM: [Thread] deep dive on [Pillar 1]
  - 2 PM: [Hot take] about [trending topic]

Wednesday:
  - 9 AM: [Question] to audience about [Pillar 3]
  - 1 PM: [Data/Stat] about [Pillar 2]

Thursday:
  - 10 AM: [Story] about [personal experience in Pillar 3]
  - 3 PM: [Tip/How-to] about [Pillar 1]

Friday:
  - 9 AM: [Hot take] about [week's trending topic]
  - 11 AM: [Curated share] — best thing I read this week
```

---

## Performance Metrics

### Key Metrics
| Metric | What It Measures | Good Benchmark |
|--------|-----------------|----------------|
| Impressions | How many people saw the tweet | Varies by follower count |
| Engagement rate | (likes+RTs+replies)/impressions | >2% is good, >5% is great |
| Reply rate | replies/impressions | >0.5% is good |
| Retweet rate | RTs/impressions | >1% is good |
| Profile visits | People checking your profile after tweet | Track trend |
| Follower growth | Net new followers per period | Track trend |

### Engagement Rate Formula
```
engagement_rate = (likes + retweets + replies + quotes) / impressions * 100

Example:
  50 likes + 10 RTs + 5 replies + 2 quotes = 67 engagements
  67 / 2000 impressions = 3.35% engagement rate
```

### Content Performance Analysis
Track which content types and topics perform best:
```
| Content Type | Avg Impressions | Avg Engagement Rate | Best Performing |
|-------------|-----------------|--------------------|--------------------|
| Hot take | 2500 | 4.2% | "Unpopular opinion: ..." |
| Thread | 5000 | 3.1% | "I analyzed 500 ..." |
| Tip | 1800 | 5.5% | "How to ... in 3 steps" |
```

Use this data to optimize future content mix.

---

## Brand Voice Guide

### Voice Dimensions
| Dimension | Range | Description |
|-----------|-------|-------------|
| Formal ↔ Casual | 1-5 | 1=corporate, 5=texting a friend |
| Serious ↔ Humorous | 1-5 | 1=all business, 5=comedy account |
| Reserved ↔ Bold | 1-5 | 1=diplomatic, 5=no-filter |
| General ↔ Technical | 1-5 | 1=anyone can understand, 5=deep expert |

### Consistency Rules
- Use the same voice across ALL tweets (hot takes and how-tos)
- Develop 3-5 "signature phrases" you reuse naturally
- If the brand voice says "casual," don't suddenly write a formal thread
- Read tweets aloud — does it sound like the same person?

---

## Safety & Compliance

### Content Guidelines
NEVER post:
- Discriminatory content (race, gender, religion, sexuality, disability)
- Defamatory claims about real people or companies
- Private or confidential information
- Threats, harassment, or incitement to violence
- Impersonation of other accounts
- Misleading claims presented as fact
- Content that violates Twitter Terms of Service

### Approval Mode Queue Format
```json
[
  {
    "id": "q_001",
    "content": "Tweet text here",
    "type": "hot_take",
    "pillar": "AI",
    "scheduled_for": "2025-01-15T10:00:00Z",
    "created": "2025-01-14T20:00:00Z",
    "status": "pending",
    "notes": "Based on trending discussion about LLM pricing"
  }
]
```

Preview file for human review:
```markdown
# Tweet Queue Preview
Generated: YYYY-MM-DD

## Pending Tweets (N total)

### 1. [Hot Take] — Scheduled: Mon 10 AM
> Tweet text here

**Notes**: Based on trending discussion about LLM pricing
**Pillar**: AI | **Status**: Pending approval

---

### 2. [Thread] — Scheduled: Tue 10 AM
> Tweet 1/5: Hook text here
> Tweet 2/5: Point one
> ...

**Notes**: Deep dive on new benchmark results
**Pillar**: AI | **Status**: Pending approval
```

### Risk Assessment
Before posting, evaluate each tweet:
- Could this be misinterpreted? → Rephrase for clarity
- Does this punch down? → Don't post
- Would you be comfortable seeing this attributed to the user in a news article? → If no, don't post
- Is this verifiably true? → If not sure, add hedging language or don't post
