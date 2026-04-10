# Reddit Hand Skills

## Platform Overview

Reddit is a community-driven platform with thousands of niche communities (subreddits).

## API Reference

### Authentication
```bash
# Get access token
curl -X POST "https://www.reddit.com/api/v1/access_token" \
  -d "grant_type=password&username={user}&password={pass}" \
  -u "{client_id}:{client_secret}" \
  -H "User-Agent: script:my-app:v1.0"
```

### Key Endpoints

```
# Submit a post
POST https://oauth.reddit.com/api/submit

# Vote
POST https://oauth.reddit.com/api/vote

# Comment
POST https://oauth.reddit.com/api/comment

# Get user info
GET https://oauth.reddit.com/api/v1/me

# Get subreddit info
GET https://www.reddit.com/r/{subreddit}/about.json
```

## Subreddit Discovery

### Finding Relevant Subreddits
```bash
# Search for subreddits
curl -s "https://www.reddit.com/subreddits/search.json?q=AI" | jq '.data.children[].data'

# Get subreddit stats
curl -s "https://www.reddit.com/r/MachineLearning/about.json" | jq '.data'
```

### Subreddit Stats to Check
- `subscribers`: Total members
- `active_user_count`: Active users
- `public_description`: What the subreddit is about
- `rules`: Posting rules

## Post Types

### Self (Text) Post
```json
{
  "sr": "subreddit_name",
  "title": "Post Title",
  "text": "Post body content",
  "kind": "self"
}
```

### Link Post
```json
{
  "sr": "subreddit_name",
  "title": "Post Title",
  "url": "https://example.com",
  "kind": "link"
}
```

## Content Best Practices

### Title Guidelines
- **Do**:
  - Be specific and descriptive
  - Ask questions when appropriate
  - Use numbers (e.g., "5 ways to...")
  - Create intrigue
  
- **Don't**:
  - Use ALL CAPS
  - Over punctuate (!!!???)
  - Use clickbait
  - Be misleading

### Body Content
- Add value beyond the title
- Use markdown formatting
- Include sources if citing
- Break up long text with paragraphs

### Best Practices by Subreddit
- Read top posts to understand what works
- Note the tone (serious vs casual)
- Check if images/videos are common
- Look at post frequency by other users

## Engagement Tactics

### Commenting
- **Do**:
  - Add value to the discussion
  - Share personal experience
  - Ask thoughtful questions
  - Be helpful
  
- **Don't**:
  - Say just "great post!" or "+1"
  - Be dismissive
  - Argue aggressively
  - Derail the conversation

### Upvoting
- Upvote quality content
- Downvote spam/obvious violations
- Don't use automated voting scripts

### Building Karma
1. Start with smaller subreddits
2. Read and follow rules
3. Post quality content
4. Engage genuinely in comments
5. Be patient

## Subreddit Rules

### Common Rule Types
- No spam/self-promotion
- Be respectful
- No NSFW content
- Use appropriate post flair
- No misinformation

### Reddiquette
- Remember the human
- Behave like you would in real life
- Search before posting
- Read the community rules

## Content Calendar

| Day | Action |
|-----|--------|
| Mon | Post to primary subreddits |
| Tue | Engage with comments on posts |
| Wed | Post to secondary subreddits |
| Thu | Upvote and comment |
| Fri | Post discussion questions |
| Sat | Share curated content |
| Sun | Review and plan |

## Troubleshooting

### "You're doing that too much"
- Reddit has rate limits
- Wait 10 minutes between posts
- Take breaks

### "Account too new to post"
- Karma requirements vary by subreddit
- Lurk and comment first
- Build credibility

### "This post has been removed"
- Check subreddit rules
- May have triggered automod
- Appeal if you believe it was wrong

## Popular Subreddits by Niche

### Technology
- r/technology
- r/MachineLearning
- r/artificial
- r/programming
- r/startups

### Business
- r/Entrepreneur
- r/smallbusiness
- r/marketing
- r/sales

### Productivity
- r/productivity
- r/getdisciplined
- r/entrepreneur

## Analytics

Track per post:
- Upvotes/downvotes
- Comments
- Awards
- Click-through (for links)

Track overall:
- Total karma
- Post frequency
- Comment frequency
- Best performing content types
