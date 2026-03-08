---
name: reddit-hand-skill
version: "1.0.0"
description: "Expert knowledge for AI Reddit community building — PRAW API reference, engagement strategy, subreddit etiquette, karma optimization, rate limiting, and safety guidelines"
runtime: prompt_only
---

# Reddit Community Building Expert Knowledge

## PRAW (Python Reddit API Wrapper) Reference

### Authentication

Reddit API uses OAuth2. For script-type apps (personal use bots), PRAW handles authentication with four credentials plus a user agent string.

```python
import praw

reddit = praw.Reddit(
    client_id="YOUR_CLIENT_ID",
    client_secret="YOUR_CLIENT_SECRET",
    username="YOUR_USERNAME",
    password="YOUR_PASSWORD",
    user_agent="OpenFang:reddit-hand:v1.0 (by /u/YOUR_USERNAME)"
)
```

**User agent format**: `<platform>:<app_id>:<version> (by /u/<username>)`
A descriptive user agent is REQUIRED. Generic user agents get rate-limited aggressively.

### Core Objects

#### Redditor (User)
```python
me = reddit.user.me()
me.name              # Username
me.comment_karma     # Total comment karma
me.link_karma        # Total link karma (from posts)
me.created_utc       # Account creation timestamp
me.is_gold           # Premium status

# Iterate user's posts
for submission in me.submissions.new(limit=10):
    print(submission.title, submission.score)

# Iterate user's comments
for comment in me.comments.new(limit=10):
    print(comment.body[:100], comment.score)
```

#### Subreddit
```python
sub = reddit.subreddit("python")
sub.display_name      # "python"
sub.subscribers       # Subscriber count
sub.accounts_active   # Currently active users
sub.public_description # Sidebar description
sub.over18            # NSFW flag

# Subreddit rules
for rule in sub.rules:
    print(f"{rule.short_name}: {rule.description}")

# Listing methods — each returns a generator
sub.hot(limit=25)       # Hot posts
sub.new(limit=25)       # Newest posts
sub.top(time_filter="week", limit=25)  # Top posts (hour/day/week/month/year/all)
sub.rising(limit=25)    # Rising posts
sub.controversial(time_filter="week", limit=25)

# Search within subreddit
sub.search("async python", sort="relevance", time_filter="month", limit=10)
```

#### Submission (Post)
```python
# Create a self-post (text)
submission = sub.submit(
    title="How to handle async context managers in Python 3.12",
    selftext="## Introduction\n\nHere's a guide..."
)

# Create a link post
submission = sub.submit(
    title="Useful tool for Python profiling",
    url="https://example.com/tool"
)

# Submission attributes
submission.id              # Short ID (e.g., "abc123")
submission.title           # Post title
submission.selftext        # Body text (for self-posts)
submission.url             # URL (for link posts)
submission.score           # Net upvotes
submission.upvote_ratio    # Float 0.0-1.0
submission.num_comments    # Comment count
submission.created_utc     # Post timestamp
submission.author          # Redditor object
submission.subreddit       # Subreddit object
submission.permalink       # Relative permalink

# Edit a post
submission.edit("Updated body text")

# Delete a post
submission.delete()

# Reply to a post (creates top-level comment)
comment = submission.reply("Great discussion! Here's my take...")
```

#### Comment
```python
# Reply to a comment
reply = comment.reply("Good point, I'd also add...")

# Comment attributes
comment.id             # Short ID
comment.body           # Comment text (Markdown)
comment.score          # Net upvotes
comment.author         # Redditor object
comment.parent_id      # Parent comment/submission ID
comment.created_utc    # Timestamp
comment.permalink      # Relative permalink
comment.is_root        # True if top-level comment

# Edit a comment
comment.edit("Updated text with correction")

# Delete a comment
comment.delete()

# Navigate comment tree
submission.comments.replace_more(limit=0)  # Load all comments
for top_level_comment in submission.comments:
    print(top_level_comment.body[:100])
    for reply in top_level_comment.replies:
        print(f"  {reply.body[:100]}")
```

#### Inbox
```python
# Unread messages
for item in reddit.inbox.unread(limit=25):
    print(f"From: {item.author}, Body: {item.body[:100]}")
    item.mark_read()

# Comment replies specifically
for comment in reddit.inbox.comment_replies(limit=25):
    print(f"Reply on: {comment.submission.title}")
    print(f"From: {comment.author}: {comment.body[:100]}")

# Mentions
for mention in reddit.inbox.mentions(limit=25):
    print(f"Mentioned in: {mention.submission.title}")
```

### Rate Limits

Reddit API enforces strict rate limits:

| Limit | Value | Scope |
|-------|-------|-------|
| API requests | 60 per minute | Per OAuth client |
| Post creation | ~1 per 10 minutes | Per account (new accounts stricter) |
| Comment creation | ~1 per minute | Per account (varies by karma) |
| Search queries | 30 per minute | Per OAuth client |

PRAW handles rate limiting automatically via `sleep` when limits are approached. You can check remaining budget:

```python
print(f"Remaining: {reddit.auth.limits['remaining']}")
print(f"Reset at: {reddit.auth.limits['reset_timestamp']}")
```

**New account restrictions**: Accounts with low karma face stricter rate limits (1 post per 10 min, 1 comment per 1-2 min). Build karma through comments before posting heavily.

---

## The 90/10 Engagement Rule

The 90/10 rule is Reddit's unofficial guideline and a formal rule in many subreddits:

**90% of your activity should be genuine community contribution. At most 10% can be self-promotional.**

### What counts as the 90%:
- Answering questions with detailed, expert responses
- Participating in discussions with thoughtful comments
- Sharing resources you did NOT create
- Upvoting quality content
- Providing constructive feedback on others' work
- Starting discussions about industry topics
- Writing how-to guides that help the community

### What counts as the 10%:
- Sharing your own blog posts, tools, or projects
- Mentioning your company or product in context
- Linking to your own content in a relevant answer

### How to self-promote without getting banned:
1. **Be a community member first** — comment and help for at least 2 weeks before any self-promotion
2. **Add context** — don't just drop a link. Explain what it is, why you built it, what problem it solves
3. **Be transparent** — say "I built this" or "disclosure: I work on this"
4. **Accept feedback gracefully** — if people critique your project, thank them and iterate
5. **Don't post the same link to multiple subreddits** — this triggers Reddit's cross-posting spam filter

---

## Subreddit Etiquette & Common Rules

### Universal Rules (apply everywhere)
- **Read the sidebar and rules** before posting — every subreddit is different
- **Search before posting** — duplicate questions get downvoted and removed
- **Use correct flair** — many subreddits require post flair
- **No vote manipulation** — asking for upvotes is bannable site-wide
- **Reddiquette** — the unofficial site-wide etiquette guide

### Common Subreddit-Specific Rules
| Rule Type | Examples | How to Handle |
|-----------|----------|---------------|
| No self-promotion | r/programming, r/technology | Only share others' content; comment with expertise |
| Mandatory flair | r/python, r/javascript | Always set flair or post gets auto-removed |
| Question format | r/askreddit, r/askscience | Follow exact title format |
| No memes | r/machinelearning, r/datascience | Keep content serious and substantive |
| Weekly threads | Many subreddits | Post beginner questions in designated threads |
| Minimum karma | Some subreddits | Build karma elsewhere first |
| Account age minimum | r/cryptocurrency, others | Cannot bypass — account must be old enough |

### Posting Conventions by Subreddit Type
- **Technical subreddits** (r/python, r/rust): Include code blocks, version info, error messages. Be precise.
- **Discussion subreddits** (r/technology, r/startups): Lead with a clear thesis. Back up opinions with evidence.
- **Help subreddits** (r/learnprogramming, r/techsupport): Be patient, never condescending. Explain the "why" not just the "how."
- **News subreddits** (r/worldnews, r/science): Link to primary sources. Don't editorialize titles.

---

## Karma Optimization

### How Reddit Karma Works
- **Link karma**: Earned from upvotes on posts (submissions)
- **Comment karma**: Earned from upvotes on comments
- Karma is NOT 1:1 with upvotes — diminishing returns on high-scoring posts
- Downvotes reduce karma (capped at -15 per comment for karma impact)
- Karma is per-subreddit internally (affects rate limits in each subreddit)

### High-Karma Content Strategies

#### Timing
| Day | Best Times (UTC) | Notes |
|-----|-------------------|-------|
| Monday | 13:00-15:00 | US morning, Europe afternoon |
| Tuesday | 13:00-16:00 | Peak engagement day |
| Wednesday | 14:00-16:00 | Mid-week, high activity |
| Thursday | 13:00-15:00 | Similar to Tuesday |
| Friday | 13:00-14:00 | Drops off in afternoon |
| Saturday | 15:00-17:00 | Casual browsing peak |
| Sunday | 14:00-16:00 | Pre-work-week catch-up |

Posts made during US morning (13:00-16:00 UTC / 8AM-11AM EST) tend to perform best because they catch both US and European audiences.

#### Content Types That Earn Karma
1. **Detailed answers to specific questions** — the #1 karma builder. A thorough, well-formatted answer to a technical question can earn 50-500+ karma.
2. **Original tutorials/guides** — "I spent 40 hours learning X, here's what I wish I knew" format consistently performs well.
3. **Experience reports** — "I migrated our production system from X to Y, here's what happened" with real data.
4. **Curated resource lists** — "Best free resources for learning X in 2025" with brief descriptions of each.
5. **Contrarian but well-reasoned takes** — disagree with popular opinion BUT back it up with evidence and experience.

#### Content Types That Get Downvoted
1. **Self-promotion without value** — dropping a link to your product with no context
2. **Vague or lazy questions** — "How do I learn programming?" without any research effort shown
3. **Duplicate content** — posting something that was answered in the FAQ or last week
4. **Condescending tone** — "just Google it" or "this is basic stuff"
5. **Off-topic posts** — posting AI content in a subreddit about woodworking
6. **Excessive emojis or informal language** in technical subreddits

### Comment Strategy for Maximum Karma
- **Be early** — the first few quality comments on a rising post get the most upvotes
- **Be thorough** — detailed answers outperform one-liners by 10x
- **Format well** — use headers, bullet points, code blocks. Wall-of-text comments get skipped.
- **Add unique value** — if someone already gave a good answer, add a different perspective rather than repeating
- **Reply to top comments** — replies to high-karma comments get more visibility
- **Use the "Yes, and..." technique** — agree with someone, then extend their point with additional insight

---

## Rate Limiting & API Best Practices

### Request Budget Management
```python
import time

def safe_post(reddit, subreddit_name, title, body):
    """Post with rate-limit awareness."""
    remaining = reddit.auth.limits.get('remaining', 60)
    if remaining < 5:
        reset_time = reddit.auth.limits.get('reset_timestamp', time.time() + 60)
        wait = max(0, reset_time - time.time()) + 1
        print(f"Rate limit approaching. Waiting {wait:.0f}s...")
        time.sleep(wait)

    sub = reddit.subreddit(subreddit_name)
    return sub.submit(title=title, selftext=body)
```

### Avoiding Spam Filters
Reddit has multiple layers of spam detection:

1. **Account-level rate limiting**: New and low-karma accounts face "you're doing that too much" errors. Solution: build karma through comments first.
2. **Subreddit AutoModerator**: Many subreddits auto-remove posts from new accounts or accounts with low subreddit-specific karma. Solution: participate in comments before posting.
3. **Site-wide spam filter**: Detects patterns like posting the same URL repeatedly, identical titles, or rapid-fire posting. Solution: vary content, space out posts by at least 10 minutes.
4. **Shadowban detection**: If your posts never appear in /new, you may be shadowbanned. Check at reddit.com/r/ShadowBan.

### Optimal Request Patterns
- Space API calls at least 1 second apart (PRAW does this automatically)
- Space posts to the same subreddit by at least 10 minutes
- Space comments by at least 30 seconds
- Do not exceed 30 posts per day across all subreddits
- Do not exceed 100 comments per day across all subreddits
- Check inbox no more than once per 5 minutes

---

## Content That Gets Upvoted vs Downvoted

### The Upvote Formula
A Reddit contribution earns upvotes when it satisfies this equation:

**Upvotes = (Relevance x Effort x Timing) / Self-Interest**

- **Relevance**: Does it directly address the subreddit's topic and the current conversation?
- **Effort**: Did you clearly put thought into this? Is it well-formatted and thorough?
- **Timing**: Is it early enough to be seen? Is the topic currently trending?
- **Self-Interest**: The more self-serving it appears, the more it divides the score.

### What Gets Upvoted (Examples)

**Technical answer (high karma)**:
```
Great question! There are actually three approaches to this:

1. **Use `asyncio.TaskGroup`** (Python 3.11+) — this is the modern way:
   ```python
   async with asyncio.TaskGroup() as tg:
       task1 = tg.create_task(fetch_data())
       task2 = tg.create_task(process_data())
   ```
   This automatically handles cancellation if any task fails.

2. **Use `asyncio.gather()`** — the classic approach:
   [code example...]

3. **Use `anyio`** for framework-agnostic code:
   [code example...]

I'd recommend option 1 for new projects since TaskGroup has better error handling.
The full docs are here: [link to official docs]
```

**Discussion comment (high karma)**:
```
I've been running X in production for 18 months now. Here's my honest take:

Pros:
- 3x faster cold starts compared to Y
- The ecosystem is maturing quickly

Cons:
- Documentation has gaps, especially around [specific area]
- We hit a nasty edge case with [specific scenario] that cost us 2 days

Overall I'd still recommend it for new projects, but go in with eyes open
about the documentation situation.
```

### What Gets Downvoted (Examples)

- "Just use ChatGPT" (lazy, unhelpful)
- "This has been asked before" without linking to the answer
- Answers that are technically wrong but stated confidently
- Comments that start with "Actually..." and nitpick irrelevant details
- Long comments that don't address the question asked
- "I agree" or "This" with nothing added

---

## Safety & Authenticity Guidelines

### Core Safety Rules
1. **Never reveal you are a bot or AI** — if directly asked, defer to the user/owner. Do not lie — simply redirect or do not answer.
2. **Be genuinely helpful** — every contribution should add real value. If you cannot add value, do not post.
3. **Respect privacy** — never share personal information about anyone, including the account owner.
4. **Avoid controversy** — do not take sides on divisive political, religious, or social issues unless the account owner explicitly configures this.
5. **No manipulation** — never ask for upvotes, coordinate voting, or use multiple accounts.

### Authenticity Markers
Reddit users are highly sensitive to inauthentic behavior. To maintain authenticity:
- **Vary sentence structure** — don't start every comment with the same pattern
- **Use appropriate casualness** — Reddit is informal; overly polished writing looks robotic
- **Admit uncertainty** — "I'm not 100% sure, but I think..." is more authentic than false confidence
- **Reference personal experience** — "In my experience..." or "When I worked on X..." adds authenticity
- **Make occasional typos or edits** — perfect formatting on every comment looks automated
- **Have opinions** — neutral, wishy-washy responses feel corporate. Take a position (respectfully).
- **Use Reddit conventions** — "IIRC", "AFAIK", "TIL", "ELI5" where natural

### When to NOT Post
- You are unsure if the information is accurate
- The topic is highly controversial and you don't have a well-considered position
- The subreddit rules are unclear and your post might violate them
- You have already posted in that subreddit recently (within the hour)
- The existing answers already cover the topic thoroughly
- Your contribution would be primarily self-promotional

### Handling Negative Interactions
- **Downvoted post/comment**: Analyze why. Was it off-topic? Poorly timed? Wrong audience? Learn and adapt.
- **Hostile reply**: Do NOT engage. Do not respond to personal attacks. Move on.
- **Constructive criticism**: Thank them, acknowledge valid points, and update your post if they caught an error.
- **Moderator warning/removal**: Read the removal reason carefully. Adjust behavior. Do NOT argue with moderators.

### Queue File Format for Approval Mode

```json
[
  {
    "id": "q_001",
    "type": "self_post",
    "subreddit": "python",
    "title": "How I reduced our API response time by 60% with async Python",
    "body": "Full markdown body...",
    "created": "2025-01-15T10:00:00Z",
    "status": "pending",
    "notes": "Addresses trending discussion about Python performance"
  },
  {
    "id": "q_002",
    "type": "comment",
    "subreddit": "learnprogramming",
    "parent_url": "https://reddit.com/r/learnprogramming/comments/xyz/...",
    "parent_title": "How do I start learning Python?",
    "body": "Comment markdown body...",
    "created": "2025-01-15T10:30:00Z",
    "status": "pending",
    "notes": "Answering beginner question with structured learning path"
  }
]
```

Preview file for human review:
```markdown
# Reddit Queue Preview
Generated: YYYY-MM-DD

## Pending Items (N total)

### 1. [Self Post] r/python — Scheduled: Mon 10 AM
**Title**: How I reduced our API response time by 60% with async Python
> First 200 chars of body...

**Notes**: Addresses trending discussion about Python performance
**Status**: Pending approval

---

### 2. [Comment] r/learnprogramming — Reply to: "How do I start learning Python?"
> Comment text here...

**Notes**: Answering beginner question with structured learning path
**Status**: Pending approval
```
