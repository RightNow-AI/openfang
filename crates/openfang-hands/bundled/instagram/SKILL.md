# Instagram Hand Skills

## Platform Overview

Instagram is a visual-first platform with feed posts, Stories, Reels, and DMs.

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

#### Get Instagram Business Account
```
GET /me/accounts
GET /{page_id}?fields=instagram_business_account
```

#### Media Operations
```
POST /{ig_business_account_id}/media
POST /{ig_business_account_id}/media_publish
GET /{media_id}/insights
GET /{media_id}/comments
```

#### Carousel Posts
```
POST /{ig_business_account_id}/media
{
  "caption": "...",
  "children": ["media_id_1", "media_id_2", "media_id_3"],
  "media_type": "CAROUSEL"
}
```

## Content Best Practices

### Image Requirements
- Format: JPG or PNG
- Max size: 8MB
- Min resolution: 1080×1080 (square), 1080×1350 (portrait)
- Color profile: sRGB

### Video Requirements
- Format: MP4 or MOV
- Max size: 100MB (Feed), 250MB (Reels)
- Max duration: 60 seconds (Feed), 90 seconds (Reels)
- Codec: H.264

### Caption Guidelines
1. **Hook** (first 125 characters - most important)
   - Stop the scroll
   - Ask a question
   - Make a bold statement
   
2. **Body**
   - Tell a story
   - Provide value
   - Be authentic
   
3. **Call-to-Action**
   - "Save this post"
   - "Share with someone who..."
   - "Comment your thoughts"
   
4. **Hashtags**
   - Mix: 3-5 niche + 3-5 medium + 2-3 broad
   - Use in comments or end of caption
   - Don't over hashtag

## Hashtag Strategy

### Size Categories
- **Large** (>1M posts): Generic reach (#love, #photo)
- **Medium** (100K-1M): Category (#portraitphotography)
- **Small** (10K-100K): Niche (#streetphotographynyc)
- **Micro** (<10K): Ultra-specific (#bnw_captures)

### Optimal Mix
- 1-2 large hashtags
- 3-5 medium hashtags
- 5-10 small hashtags
- Total: 10-20 per post

## Engagement Tactics

### Post Timing (General)
- Best times: 9AM, 12PM, 5PM local time
- Worst times: 3AM-6AM

### Stories
- Use stickers (questions, polls, quizzes)
- Post 3-7 stories per day when active
- Behind-the-scenes content works well

### Reels
- First frame is crucial - hook immediately
- Use trending audio when relevant
- Caption is important for discoverability

## Content Calendar Example

| Day | Content Type | Focus |
|-----|--------------|-------|
| Monday | Carousel | Educational |
| Tuesday | Single Image | Quote/Inspiration |
| Wednesday | Reel | Behind-the-scenes |
| Thursday | Carousel | Tips/How-to |
| Friday | Single Image | Personal/Team |
| Saturday | Reel | Trending audio |
| Sunday | Story | Engagement |

## Trending Formats

1. **GRWM** (Get Ready With Me)
2. **Day in my life**
3. **POV: You're a...**
4. **Duets/Reactions**
5. **Tutorial/How-to carousels**
6. **Before/After**
7. **Throwback**
8. **Team/Company culture**

## Analytics to Track

- Reach
- Impressions
- Engagement rate
- Follower growth
- Best performing content types
- Optimal posting times
- Hashtag performance
