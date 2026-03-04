---
name: linkedin-hand-skill
version: "1.0.0"
description: "Expert knowledge for LinkedIn content management — API v2 reference, content strategy, engagement playbook, algorithm insights, and professional networking"
runtime: prompt_only
---

# LinkedIn Management Expert Knowledge

## LinkedIn API v2 Reference

### Authentication
LinkedIn API uses OAuth 2.0 Bearer Tokens for all API access.

**Bearer Token** (read/write access):
```
Authorization: Bearer $LINKEDIN_ACCESS_TOKEN
```

**Environment variable**: `LINKEDIN_ACCESS_TOKEN`

### Required Scopes
- `openid` — OpenID Connect
- `profile` — Read basic profile
- `email` — Read email address
- `w_member_social` — Create/delete posts and comments

### Core Endpoints

**Get authenticated user profile**:
```bash
curl -s -H "Authorization: Bearer $LINKEDIN_ACCESS_TOKEN" \
  "https://api.linkedin.com/v2/userinfo"
```
Response: `{"sub": "URN_ID", "name": "Full Name", "email": "user@example.com"}`

**Get member URN** (needed for posting):
```bash
curl -s -H "Authorization: Bearer $LINKEDIN_ACCESS_TOKEN" \
  "https://api.linkedin.com/v2/userinfo" | python3 -c "import sys,json; print(json.load(sys.stdin)['sub'])"
```

**Create a text post (UGC Post API)**:
```bash
curl -s -X POST "https://api.linkedin.com/v2/ugcPosts" \
  -H "Authorization: Bearer $LINKEDIN_ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -H "X-Restli-Protocol-Version: 2.0.0" \
  -d '{
    "author": "urn:li:person:YOUR_MEMBER_URN",
    "lifecycleState": "PUBLISHED",
    "specificContent": {
      "com.linkedin.ugc.ShareContent": {
        "shareCommentary": { "text": "Your post content here" },
        "shareMediaCategory": "NONE"
      }
    },
    "visibility": { "com.linkedin.ugc.MemberNetworkVisibility": "PUBLIC" }
  }'
```

**Create a post with link/article**:
```bash
curl -s -X POST "https://api.linkedin.com/v2/ugcPosts" \
  -H "Authorization: Bearer $LINKEDIN_ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -H "X-Restli-Protocol-Version: 2.0.0" \
  -d '{
    "author": "urn:li:person:YOUR_MEMBER_URN",
    "lifecycleState": "PUBLISHED",
    "specificContent": {
      "com.linkedin.ugc.ShareContent": {
        "shareCommentary": { "text": "Check out this article" },
        "shareMediaCategory": "ARTICLE",
        "media": [{
          "status": "READY",
          "originalUrl": "https://example.com/article"
        }]
      }
    },
    "visibility": { "com.linkedin.ugc.MemberNetworkVisibility": "PUBLIC" }
  }'
```

**Delete a post**:
```bash
curl -s -X DELETE "https://api.linkedin.com/v2/ugcPosts/POST_URN" \
  -H "Authorization: Bearer $LINKEDIN_ACCESS_TOKEN"
```

**Get post engagement stats**:
```bash
curl -s -H "Authorization: Bearer $LINKEDIN_ACCESS_TOKEN" \
  "https://api.linkedin.com/v2/socialActions/POST_URN"
```

### Image Upload Flow
1. Register upload:
```bash
curl -s -X POST "https://api.linkedin.com/v2/assets?action=registerUpload" \
  -H "Authorization: Bearer $LINKEDIN_ACCESS_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "registerUploadRequest": {
      "recipes": ["urn:li:digitalmediaRecipe:feedshare-image"],
      "owner": "urn:li:person:YOUR_MEMBER_URN",
      "serviceRelationships": [{"identifier": "urn:li:userGeneratedContent", "relationshipType": "OWNER"}]
    }
  }'
```
2. Upload binary to the `uploadUrl` from response
3. Use the `asset` URN in your post's media array

### Rate Limits
- **Posts per day**: 100 (company pages), ~25 recommended for personal
- **API calls**: 100 requests per day per member for most endpoints
- **Throttling**: 429 status code — back off and retry with exponential delay
- **Token expiry**: Access tokens expire after 60 days — refresh before expiry

## Content Strategy for LinkedIn

### Post Formats That Perform Best
1. **Text-only posts** — Highest organic reach (no outbound links)
2. **Document/Carousel posts** — High engagement, swipeable slides
3. **Polls** — Algorithm-boosted, drives comments
4. **Image posts** — Good engagement with relevant visuals
5. **Video** — Native video preferred over YouTube links
6. **Articles** — Long-form, lower initial reach but evergreen

### The LinkedIn Algorithm (How Feed Works)
1. **First hour is critical** — post gets shown to ~10% of connections
2. **Engagement velocity** determines wider distribution
3. **Comments > Reactions > Shares** in algorithm weight
4. **Dwell time** matters — longer posts that people read signal quality
5. **External links reduce reach** — put links in first comment instead
6. **Posting frequency**: 1-2x/day max, 3-5x/week optimal
7. **Best times**: Tue-Thu, 7-8 AM or 12-1 PM (audience timezone)

### Post Structure (The Hook-Body-CTA Pattern)
```
[Hook — first 2 lines visible before "...see more"]

[Body — the value, insight, or story]

[CTA — engagement ask]
```

### Hook Formulas
1. **The Contrarian**: "Everyone says [X]. I disagree. Here's why:"
2. **The Story**: "3 years ago, I [made a mistake]. Here's what I learned:"
3. **The Data**: "[Specific number/stat] changed how I think about [topic]."
4. **The List**: "[N] lessons from [experience] that most people miss:"
5. **The Question**: "What if [common practice] is actually holding you back?"
6. **The Confession**: "I used to [common behavior]. Then I realized..."

### Formatting Rules
- **Line breaks are your friend** — one idea per line
- **Use emojis as bullets** sparingly (→, ✅, 🔑, 📌)
- **Bold with asterisks** not supported — use ALL CAPS for emphasis (sparingly)
- **Max length**: 3,000 characters, but 1,200-1,500 is sweet spot
- **Hashtags**: 3-5 max, at the end of the post
- **No hashtag walls** — use specific ones (#ProductManagement not #business)

### Content Pillars for Thought Leadership
1. **Industry Insights** — trends, analysis, predictions
2. **Lessons Learned** — failures, pivots, retrospectives
3. **How-To/Tactical** — frameworks, templates, processes
4. **Behind the Scenes** — build-in-public, day-in-the-life
5. **Curated Commentary** — react to news with unique angle

## Engagement Playbook

### Commenting Strategy
- Comment on posts from people in your target audience
- Add genuine value — don't just say "Great post!"
- Ask thoughtful follow-up questions
- Share relevant experience or data points
- Comment within first hour of their post for visibility

### Connection Growth
- Send personalized connection requests (not default message)
- Engage with someone's content 2-3 times before connecting
- Accept all relevant industry connections
- Follow-up new connections with a non-salesy message

### Response Protocol
- Reply to every comment on your posts within 2 hours
- Ask follow-up questions to keep threads going
- Pin the best comments to keep discussion visible
- Thank people who share your posts

## Safety & Professional Guidelines

### Never Post
- Confidential company information
- Negative comments about employers/colleagues
- Unverified claims or statistics
- Content that could be seen as discriminatory
- Overly promotional/salesy content (keep to 10% max)

### Approval Queue Behavior
When `approval_mode` is enabled (default):
1. Draft the post content
2. Save to approval queue with `event_publish`
3. Wait for user approval before posting via API
4. Log the approved post to knowledge graph

### Professional Tone Checklist
- ✅ Would you say this in a conference talk?
- ✅ Does it provide genuine value to the reader?
- ✅ Is it backed by experience or data?
- ✅ Would your CEO/manager be comfortable seeing this?
- ❌ Is it a humble-brag disguised as advice?
- ❌ Does it punch down or mock others?

## Dashboard Metrics

### Key Metrics to Track
| Metric | Description | Target |
|--------|-------------|--------|
| `posts_published` | Total posts created via API | Track weekly cadence |
| `articles_written` | Long-form articles published | 1-2/month |
| `engagement_rate` | (Likes + Comments + Shares) / Impressions | > 2% is good |
| `connections_made` | New connections this period | Steady growth |

### Engagement Benchmarks
- **Impressions per post**: 500-2,000 (personal), 200-1,000 (company page)
- **Engagement rate**: 2-5% is good, >5% is excellent
- **Comment-to-like ratio**: >10% indicates quality engagement
- **Profile views**: Track weekly trend, should correlate with posting
