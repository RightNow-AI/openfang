# Facebook Hand Skills

## Platform Overview

Facebook is a versatile platform with Pages, Groups, Live, Stories, and Messenger.

## API Reference

### Endpoint Base
```
https://graph.facebook.com/v18.0
```

### Authentication
```
Authorization: Bearer {ACCESS_TOKEN}
```

### Key Endpoints

#### Page Operations
```
GET /{page_id}
GET /{page_id}/feed
POST /{page_id}/feed
GET /{page_id}/conversations
GET /{page_id}/insights
```

#### Publishing
```
POST /{page_id}/feed                    # Text/link post
POST /{page_id}/photos                 # Image post
POST /{page_id}/videos                 # Video post
POST /{page_id}/live_videos            # Go live
```

#### Engagement
```
GET /{post_id}/comments
POST /{post_id}/comments
POST /{post_id}/likes
```

## Content Best Practices

### Post Length
- **Short** (<40 chars): Best for engagement
- **Medium** (40-200 chars): Good for reach
- **Long** (200+ chars): For detailed posts, stories

### Link Posts
- Include commentary with links
- Ask a question about the link
- Give your take on the content
- Don't just drop a link

### Image Posts
- Use high-quality images (1200×630 recommended)
- Single images work best
- Carousels for multi-image stories
- Add text overlay for impact

### Video Posts
- Native videos get more reach than links
- Keep videos under 3 minutes for best engagement
- Caption is crucial (many watch without sound)
- Upload directly when possible

## Post Timing

### Best Times (General)
- Weekdays: 9AM, 12PM, 3PM
- Weekends: 12PM-1PM

### Worst Times
- Late night (11PM-5AM)
- Early morning (6AM-8AM)

## Engagement Tactics

### Comment Strategy
- Respond within 1 hour
- Ask follow-up questions
- Thank users for engagement
- Pin best comments

### Message Strategy
- Set up quick replies
- Use Messenger bots for common questions
- Be human in responses

### Poll/Question Strategy
- Ask opinions
- Use either/or questions
- Keep it simple
- 2-4 options max

## Content Mix

| Type | Percentage | Purpose |
|------|-----------|---------|
| Links + commentary | 30% | Drive traffic |
| Native videos | 20% | Reach + engagement |
| Images | 20% | Visual appeal |
| Questions/Polls | 15% | Engagement |
| Curated content | 10% | Value + variety |
| Behind-the-scenes | 5% | Humanize |

## Facebook Stories

- Appear at top of News Feed
- Last 24 hours
- Can include stickers, polls, questions
- Use for:
  - Behind-the-scenes
  - Quick tips
  - Polls
  - Links (swipe up if available)

## Facebook Live

### When to Go Live
- Product launches
- Q&A sessions
- Events
- Announcements

### Best Practices
- Promote 24 hours before
- Go live for 10-30 minutes
- Interact with viewers
- Save and repost as regular video

## Analytics to Track

### Key Metrics
- **Reach**: How many saw the post
- **Engagement**: Likes, comments, shares
- **Clicks**: Link clicks, other clicks
- **Video views**: For video content
- **Page likes**: Follower growth

### Best Performers
- Track by content type
- Track by posting time
- Track by topic/category

## Facebook Groups

If managing a Group:
- Post 1-2x daily
- Ask questions
- Share valuable content
- Engage in comments
- Welcome new members

## Troubleshooting

### "Page access token expired"
- Regenerate via Graph API Explorer
- Exchange for long-lived token

### "Permission denied"
- Check app permissions
- Ensure Page is published
- Verify admin role

### Low reach
- Facebook algorithm favors:
  - Native content
  - Recent posts
  - Engagement
  - Video (especially Live)
- Consider boosting posts
