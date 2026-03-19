// ── Prompt Helper Library ─────────────────────────────────────────────────────
// Source: Outskill Prompt Library (business planning, branding, sales, email,
// content marketing, social media, market research, customer feedback).
//
// All templateText strings use {fieldKey} placeholders that PromptFillForm
// fills automatically — users never edit brackets by hand.

/** @type {PromptTemplate[]} */
export const PROMPT_LIBRARY = [
  // ── Business Planning ───────────────────────────────────────────────────────
  {
    id: 'swot-analysis',
    title: 'Create a SWOT analysis',
    category: 'business_planning',
    description: 'Find strengths, weaknesses, opportunities, and threats for your business.',
    difficulty: 'Beginner',
    icon: '📊',
    fields: [
      { key: 'company',   label: 'Business name',      placeholder: 'Northstar Studio',  required: true  },
      { key: 'strength',  label: 'One key strength',   placeholder: 'Strong word-of-mouth referrals', required: false },
      { key: 'weakness',  label: 'One key weakness',   placeholder: 'Low online presence', required: false },
    ],
    templateText:
      'Please create a SWOT analysis for my business called {company}. ' +
      'My main strength is: {strength}. My main weakness is: {weakness}. ' +
      'Include specific, practical suggestions for how I can build on the strength and address the weakness.',
    examples: ['I run a local bakery with strong regulars but no social media presence.'],
  },
  {
    id: 'business-plan',
    title: 'Write a business plan',
    category: 'business_planning',
    description: 'Get a full business plan draft including goals, strategy, and financials.',
    difficulty: 'Intermediate',
    icon: '📋',
    fields: [
      { key: 'company',   label: 'Business name',       placeholder: 'Maple & Co.',             required: true  },
      { key: 'product',   label: 'What you sell',       placeholder: 'organic dog treats',      required: true  },
      { key: 'audience',  label: 'Who buys from you',   placeholder: 'pet owners aged 25–45',   required: true  },
      { key: 'goal',      label: 'Main goal this year', placeholder: 'reach $50k in revenue',   required: false },
    ],
    templateText:
      'Write a professional business plan for {company}, which sells {product} to {audience}. ' +
      'Our main goal this year is to {goal}. ' +
      'Include: company overview, market opportunity, marketing strategy, financial projections, and key risks.',
    examples: [],
  },
  {
    id: 'elevator-pitch',
    title: 'Write an elevator pitch',
    category: 'business_planning',
    description: 'A short, compelling pitch you can say in 30–60 seconds.',
    difficulty: 'Beginner',
    icon: '🎤',
    fields: [
      { key: 'company',   label: 'Business name',        placeholder: 'Bloom Financial',            required: true  },
      { key: 'product',   label: 'What you offer',       placeholder: 'tax planning for freelancers', required: true  },
      { key: 'audience',  label: 'Who it is for',        placeholder: 'self-employed creatives',     required: true  },
      { key: 'benefit',   label: 'The main benefit',     placeholder: 'save hundreds on taxes',      required: true  },
    ],
    templateText:
      'Write a clear and compelling 60-second elevator pitch for {company}. ' +
      'We offer {product} to {audience}. ' +
      'The main benefit we deliver is: {benefit}. ' +
      'Make it conversational, memorable, and end with a clear next step.',
    examples: [],
  },

  // ── Branding ────────────────────────────────────────────────────────────────
  {
    id: 'brand-slogan',
    title: 'Write a brand slogan',
    category: 'branding',
    description: 'A short tagline that captures what your brand stands for.',
    difficulty: 'Beginner',
    icon: '✨',
    fields: [
      { key: 'company',   label: 'Business name',      placeholder: 'Cinder Candles',          required: true  },
      { key: 'value',     label: 'Core brand value',   placeholder: 'warmth and calm',         required: true  },
      { key: 'audience',  label: 'Who you serve',      placeholder: 'people who love cozy homes', required: false },
    ],
    templateText:
      'Create 5 brand slogan options for {company}. ' +
      'Our core value is {value} and our audience is {audience}. ' +
      'Each slogan should be under 8 words, memorable, and reflect our brand personality.',
    examples: [],
  },
  {
    id: 'brand-identity',
    title: 'Build a brand identity',
    category: 'branding',
    description: 'Get clear direction on your brand voice, visual style, and messaging.',
    difficulty: 'Intermediate',
    icon: '🎨',
    fields: [
      { key: 'company',   label: 'Business name',       placeholder: 'Verdant Wellness',       required: true  },
      { key: 'industry',  label: 'Your industry',       placeholder: 'health and wellness',    required: true  },
      { key: 'audience',  label: 'Your audience',       placeholder: 'busy professionals',     required: true  },
      { key: 'value1',    label: 'Brand value #1',      placeholder: 'simplicity',             required: true  },
      { key: 'value2',    label: 'Brand value #2',      placeholder: 'sustainability',         required: false },
    ],
    templateText:
      'Create a detailed brand identity guide for {company}, a {industry} business. ' +
      'Our audience is {audience}. Our core values are {value1} and {value2}. ' +
      'Include: brand voice and tone, visual style recommendations, key messaging pillars, and 3 example social-media captions.',
    examples: [],
  },

  // ── Sales ───────────────────────────────────────────────────────────────────
  {
    id: 'sales-proposal',
    title: 'Write a sales proposal',
    category: 'sales',
    description: 'A persuasive proposal that shows clients exactly why they should choose you.',
    difficulty: 'Intermediate',
    icon: '🤝',
    fields: [
      { key: 'product',    label: 'What you are selling',   placeholder: 'website redesign',            required: true  },
      { key: 'audience',   label: 'Who you are pitching',   placeholder: 'small restaurant owners',     required: true  },
      { key: 'painpoint',  label: 'Their main problem',     placeholder: 'new customers cannot find them online', required: true  },
      { key: 'benefit',    label: 'Key benefit you offer',  placeholder: 'mobile-friendly site in 2 weeks', required: true  },
    ],
    templateText:
      'Write a persuasive sales proposal for selling {product} to {audience}. ' +
      'Their main pain point is: {painpoint}. ' +
      'The key benefit we deliver is: {benefit}. ' +
      'Include an opening hook, the problem statement, our solution, pricing outline, and a strong call to action.',
    examples: [],
  },
  {
    id: 'sales-script',
    title: 'Write a sales script',
    category: 'sales',
    description: 'A step-by-step call or meeting script that handles objections.',
    difficulty: 'Intermediate',
    icon: '📞',
    fields: [
      { key: 'product',     label: 'What you sell',          placeholder: 'payroll software',            required: true  },
      { key: 'audience',    label: 'Who you are calling',    placeholder: 'small business owners',       required: true  },
      { key: 'objection1',  label: 'Common objection #1',    placeholder: 'we already have a system',    required: false },
      { key: 'objection2',  label: 'Common objection #2',    placeholder: 'it is too expensive',         required: false },
    ],
    templateText:
      'Write a sales call script for selling {product} to {audience}. ' +
      'Address these common objections: (1) {objection1} and (2) {objection2}. ' +
      'The script should include: opening, discovery questions, value pitch, objection handling, and a close.',
    examples: [],
  },

  // ── Email ───────────────────────────────────────────────────────────────────
  {
    id: 'follow-up-email',
    title: 'Write a follow-up email',
    category: 'email',
    description: 'A warm, professional email that keeps the conversation going.',
    difficulty: 'Beginner',
    icon: '📧',
    fields: [
      { key: 'recipient',  label: 'Who you are emailing',   placeholder: 'Sarah at Brightline Co.',   required: true  },
      { key: 'context',    label: 'What happened before',   placeholder: 'we met at a networking event', required: true  },
      { key: 'goal',       label: 'What you want next',     placeholder: 'schedule a 20-minute call',  required: true  },
    ],
    templateText:
      'Write a warm and professional follow-up email to {recipient}. ' +
      'Context: {context}. ' +
      'My goal for this email is to {goal}. ' +
      'Keep it under 150 words, friendly, and end with a clear single ask.',
    examples: [],
  },
  {
    id: 'cold-outreach-email',
    title: 'Write a cold outreach email',
    category: 'email',
    description: "Grab a stranger's attention and start a business conversation.",
    difficulty: 'Intermediate',
    icon: '✉️',
    fields: [
      { key: 'company',    label: 'Your business name',       placeholder: 'Apex Analytics',              required: true  },
      { key: 'product',    label: 'What you offer',           placeholder: 'AI-powered sales reporting',  required: true  },
      { key: 'recipient',  label: 'Who you are contacting',   placeholder: 'VP of Sales at mid-size tech companies', required: true },
      { key: 'benefit',    label: 'One clear benefit',        placeholder: 'cut reporting time in half',  required: true  },
    ],
    templateText:
      'Write a compelling cold outreach email from {company} to {recipient}. ' +
      'We offer {product}. The most relevant benefit for them is: {benefit}. ' +
      'Keep it under 100 words. Subject line included. End with one clear call to action.',
    examples: [],
  },

  // ── Content Marketing ───────────────────────────────────────────────────────
  {
    id: 'blog-outline',
    title: 'Write a blog outline',
    category: 'content_marketing',
    description: 'A structured outline for a blog post that ranks and gets read.',
    difficulty: 'Beginner',
    icon: '📝',
    fields: [
      { key: 'topic',     label: 'Blog topic',           placeholder: 'how to price your freelance services', required: true  },
      { key: 'audience',  label: 'Who will read it',     placeholder: 'new freelancers',                      required: true  },
      { key: 'goal',      label: 'Goal of the post',     placeholder: 'get readers to book a discovery call', required: false },
    ],
    templateText:
      'Create a detailed blog post outline for the topic: "{topic}". ' +
      'The audience is {audience}. The goal of the post is to {goal}. ' +
      'Include: a headline, 5–7 section headers with short descriptions, a suggested intro hook, and a CTA.',
    examples: [],
  },
  {
    id: 'product-description',
    title: 'Write a product description',
    category: 'content_marketing',
    description: 'Compelling copy that turns browsers into buyers.',
    difficulty: 'Beginner',
    icon: '🛍️',
    fields: [
      { key: 'product',   label: 'Product name',         placeholder: 'The Clarity Planner',              required: true  },
      { key: 'features',  label: 'Top 3 features',       placeholder: 'weekly spreads, habit tracker, reflection prompts', required: true },
      { key: 'audience',  label: 'Who it is for',        placeholder: 'busy professionals',               required: true  },
      { key: 'benefit',   label: 'The main benefit',     placeholder: 'feel in control of your week',     required: true  },
    ],
    templateText:
      'Write a compelling product description for {product}. ' +
      'Key features: {features}. ' +
      'This product is for {audience} who want to {benefit}. ' +
      'Write 3 versions: short (25 words), medium (75 words), long (150 words).',
    examples: [],
  },
  {
    id: 'webinar-outline',
    title: 'Make a webinar outline',
    category: 'content_marketing',
    description: 'A structured run-of-show for a live or recorded webinar.',
    difficulty: 'Intermediate',
    icon: '🎥',
    fields: [
      { key: 'topic',       label: 'Webinar topic',       placeholder: 'how to get your first 100 customers', required: true  },
      { key: 'audience',    label: 'Who will attend',     placeholder: 'early-stage founders',                required: true  },
      { key: 'duration',    label: 'Length (in minutes)', placeholder: '45',                                  required: false },
      { key: 'cta',         label: 'End goal / CTA',      placeholder: 'sign up for a free trial',            required: false },
    ],
    templateText:
      'Create a {duration}-minute webinar outline on the topic: "{topic}". ' +
      'The audience is {audience}. The CTA at the end is: {cta}. ' +
      'Include: title suggestions, opening hook, 4–5 main sections with timing, Q&A slot, and closing CTA.',
    examples: [],
  },

  // ── Social Media ────────────────────────────────────────────────────────────
  {
    id: 'social-media-calendar',
    title: 'Build a social media content calendar',
    category: 'social_media',
    description: 'A week-by-week plan of what to post and when.',
    difficulty: 'Beginner',
    icon: '📅',
    fields: [
      { key: 'company',    label: 'Business name',       placeholder: 'Luna Bakes',                    required: true  },
      { key: 'platform',   label: 'Main platform',       placeholder: 'Instagram',                     required: true  },
      { key: 'audience',   label: 'Your audience',       placeholder: 'local food lovers',             required: true  },
      { key: 'duration',   label: 'Planning horizon',    placeholder: '4 weeks',                       required: false },
    ],
    templateText:
      'Create a {duration} social media content calendar for {company} on {platform}. ' +
      'Our audience is {audience}. ' +
      'Include: post type (educational, promotional, behind-the-scenes, engagement), suggested caption style, best posting times, and 3 example caption drafts.',
    examples: [],
  },
  {
    id: 'social-ad-copy',
    title: 'Write social media ad copy',
    category: 'social_media',
    description: 'Short, punchy ad text that stops the scroll.',
    difficulty: 'Beginner',
    icon: '📣',
    fields: [
      { key: 'product',    label: 'What you are advertising', placeholder: 'online yoga course',      required: true  },
      { key: 'platform',   label: 'Ad platform',             placeholder: 'Instagram',               required: true  },
      { key: 'audience',   label: 'Target audience',         placeholder: 'women aged 30–50',        required: true  },
      { key: 'cta',        label: 'Call to action',          placeholder: 'sign up for free',        required: true  },
    ],
    templateText:
      'Write 3 versions of ad copy for {platform} promoting {product} to {audience}. ' +
      'The CTA is: {cta}. ' +
      'Version 1: curiosity hook. Version 2: social proof / benefit-led. Version 3: urgent / limited-time. ' +
      'Keep each under 40 words. Include a suggested headline for each.',
    examples: [],
  },

  // ── Market Research ─────────────────────────────────────────────────────────
  {
    id: 'competitor-research',
    title: 'Research competitors',
    category: 'market_research',
    description: 'Understand what your competitors offer and where the gaps are.',
    difficulty: 'Beginner',
    icon: '🔍',
    fields: [
      { key: 'industry',   label: 'Your industry',       placeholder: 'online fitness coaching',       required: true  },
      { key: 'product',    label: 'What you sell',       placeholder: 'personalised workout plans',    required: true  },
      { key: 'audience',   label: 'Your target market',  placeholder: 'men aged 25–45 wanting to lose weight', required: true },
    ],
    templateText:
      'Analyse the competitive landscape for a business in the {industry} industry that sells {product} to {audience}. ' +
      'Include: 5 typical competitors and what they offer, common pricing models, key marketing messages they use, ' +
      'gaps in the market, and 3 ways I could differentiate my offer.',
    examples: [],
  },
  {
    id: 'target-market',
    title: 'Identify your target market',
    category: 'market_research',
    description: 'Get a clear picture of exactly who your best customer is.',
    difficulty: 'Beginner',
    icon: '🎯',
    fields: [
      { key: 'product',   label: 'What you sell',          placeholder: 'sustainable baby clothing',    required: true  },
      { key: 'industry',  label: 'Your industry',          placeholder: 'children\'s fashion',          required: true  },
      { key: 'avoid',     label: 'Who it is NOT for',      placeholder: 'fast-fashion shoppers',        required: false },
    ],
    templateText:
      'Help me identify the ideal target market for {product} in the {industry} industry. ' +
      'This product is not aimed at {avoid}. ' +
      'Provide: demographic profile, psychographic profile (values, lifestyle, motivations), key pain points it solves, ' +
      'best channels to reach them, and how to speak their language in marketing copy.',
    examples: [],
  },

  // ── Customer Feedback ───────────────────────────────────────────────────────
  {
    id: 'customer-survey',
    title: 'Build a customer survey',
    category: 'customer_feedback',
    description: 'A short survey that gets you honest, useful answers.',
    difficulty: 'Beginner',
    icon: '📋',
    fields: [
      { key: 'company',   label: 'Business name',       placeholder: 'Riviera Spa',                     required: true  },
      { key: 'goal',      label: 'What you want to learn', placeholder: 'why clients do not rebook',    required: true  },
      { key: 'audience',  label: 'Who takes the survey', placeholder: 'customers who visited once',     required: true  },
    ],
    templateText:
      'Create a customer survey for {company} that will be sent to {audience}. ' +
      'The goal is to understand: {goal}. ' +
      'Write 8–10 questions using a mix of: rating scale (1–5), multiple choice, and one open-ended question. ' +
      'Keep language friendly and simple. Include an intro sentence explaining why we are asking.',
    examples: [],
  },

  // ── Partnerships ────────────────────────────────────────────────────────────
  {
    id: 'partnership-pitch',
    title: 'Write a partnership pitch',
    category: 'partnerships',
    description: 'A short pitch email or message to propose working together.',
    difficulty: 'Intermediate',
    icon: '🤝',
    fields: [
      { key: 'company',    label: 'Your business name',     placeholder: 'Petal & Pine Florist',        required: true  },
      { key: 'partner',    label: 'Potential partner',      placeholder: 'local wedding venue',         required: true  },
      { key: 'benefit',    label: 'What is in it for them', placeholder: 'exclusive floral packages for their clients', required: true },
    ],
    templateText:
      'Write a concise partnership pitch from {company} to {partner}. ' +
      'The value we offer them is: {benefit}. ' +
      'Include: a personal opening line, a clear explanation of the partnership idea, the mutual benefit, and a simple next step.',
    examples: [],
  },

  // ── Product Marketing ───────────────────────────────────────────────────────
  {
    id: 'product-launch',
    title: 'Plan a product launch',
    category: 'product_marketing',
    description: 'A step-by-step launch plan to generate buzz and early sales.',
    difficulty: 'Intermediate',
    icon: '🚀',
    fields: [
      { key: 'product',   label: 'Product name',         placeholder: 'TaskFlow Pro',                  required: true  },
      { key: 'audience',  label: 'Target audience',      placeholder: 'freelance designers',           required: true  },
      { key: 'date',      label: 'Launch date',          placeholder: 'in 6 weeks',                    required: false },
      { key: 'goal',      label: 'Launch goal',          placeholder: '200 sales in the first month',  required: false },
    ],
    templateText:
      'Create a product launch plan for {product}, targeting {audience}. ' +
      'Launch is {date}. Our goal is {goal}. ' +
      'Include: pre-launch hype strategy (4 weeks), launch week activities, email sequence (3 emails), ' +
      'social media plan, and post-launch follow-up.',
    examples: [],
  },
];

// ── Quick-start cards (Layer 1) ───────────────────────────────────────────────
// These 8 appear as big beginner cards at the top of the helper dock.

export const QUICK_START_CARDS = [
  { templateId: 'swot-analysis',          label: 'Start a business plan',      icon: '📊' },
  { templateId: 'follow-up-email',        label: 'Write a sales email',        icon: '📧' },
  { templateId: 'competitor-research',    label: 'Research competitors',        icon: '🔍' },
  { templateId: 'social-media-calendar',  label: 'Create social posts',         icon: '📅' },
  { templateId: 'webinar-outline',        label: 'Make a webinar outline',      icon: '🎥' },
  { templateId: 'customer-survey',        label: 'Build a customer survey',     icon: '📋' },
  { templateId: 'product-description',   label: 'Write a product description', icon: '🛍️' },
  { templateId: 'brand-slogan',           label: 'Create a brand slogan',       icon: '✨' },
];

// ── Category config ───────────────────────────────────────────────────────────

export const CATEGORIES = [
  { id: 'business_planning',  label: 'Business planning'   },
  { id: 'branding',           label: 'Branding'             },
  { id: 'sales',              label: 'Sales'                },
  { id: 'email',              label: 'Email'                },
  { id: 'content_marketing',  label: 'Content marketing'   },
  { id: 'social_media',       label: 'Social media'        },
  { id: 'market_research',    label: 'Market research'     },
  { id: 'customer_feedback',  label: 'Customer feedback'   },
  { id: 'partnerships',       label: 'Partnerships'        },
  { id: 'product_marketing',  label: 'Product marketing'   },
];

// ── Plain-English dictionary ───────────────────────────────────────────────────

export const DICTIONARY = [
  { term: 'target audience',  plainEnglish: 'The people you most want to reach'                       },
  { term: 'CTA',              plainEnglish: 'The one action you want them to take (e.g. "book a call")' },
  { term: 'USP',              plainEnglish: 'What makes you different from everyone else'               },
  { term: 'ROI',              plainEnglish: 'What you got back compared to what you spent'              },
  { term: 'A/B test',         plainEnglish: 'Trying two versions to see which one works better'         },
  { term: 'lead',             plainEnglish: 'Someone who might become a customer'                       },
  { term: 'conversion',       plainEnglish: 'When someone takes the action you wanted'                  },
  { term: 'pain point',       plainEnglish: 'A specific problem your customer has'                      },
  { term: 'value proposition',plainEnglish: 'The clear reason why someone should pick you'              },
  { term: 'funnel',           plainEnglish: 'The steps someone takes from discovering you to buying'    },
  { term: 'KPI',              plainEnglish: 'A number you track to see if you are succeeding'           },
  { term: 'organic',          plainEnglish: 'Free content (not paid ads)'                               },
  { term: 'engagement',       plainEnglish: 'Likes, comments, shares — people reacting to your content' },
  { term: 'niche',            plainEnglish: 'A specific, focused market or audience'                    },
  { term: 'SWOT',             plainEnglish: 'Strengths, Weaknesses, Opportunities, Threats — a planning tool' },
];
