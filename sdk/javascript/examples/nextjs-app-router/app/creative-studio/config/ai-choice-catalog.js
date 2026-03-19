/**
 * AI tool catalog — each entry maps to a user-selectable "lane".
 * Treat image and video as entirely separate lanes with separate approval states.
 *
 * @type {import('./creative-types').CreativeAiChoice[]}
 */
export const AI_CHOICE_CATALOG = [
  // ── Prompt / idea generation ─────────────────────────────────────────────
  {
    id: 'prompt_openai',
    label: 'OpenAI (GPT)',
    description: 'Great all-rounder for writing hooks, prompts, and briefs.',
    best_for: 'Hooks, prompts, creative briefs',
    cost_tier: 'medium',
    speed_label: 'fast',
    category: 'prompt',
    requires_approval: false,
    auto_recommend: false,
  },
  {
    id: 'prompt_anthropic',
    label: 'Anthropic (Claude)',
    description: 'Excellent for longer scripts and careful creative reasoning.',
    best_for: 'Scripts, brand copy, nuanced tone',
    cost_tier: 'medium',
    speed_label: 'fast',
    category: 'prompt',
    requires_approval: false,
    auto_recommend: false,
  },
  {
    id: 'prompt_internal',
    label: 'OpenFang Prompt Agent',
    description: 'Built-in prompt engineer — uses your existing setup.',
    best_for: 'Quick prompts, tight budgets',
    cost_tier: 'free',
    speed_label: 'fast',
    category: 'prompt',
    requires_approval: false,
    auto_recommend: false,
  },
  {
    id: 'prompt_auto',
    label: 'Auto-recommend',
    description: "We'll pick the best AI tool for prompts based on your project.",
    best_for: "When you are not sure",
    cost_tier: 'low',
    speed_label: 'fast',
    category: 'prompt',
    requires_approval: false,
    auto_recommend: true,
  },

  // ── Image generation ─────────────────────────────────────────────────────
  {
    id: 'image_openai',
    label: 'OpenAI Images (DALL-E)',
    description: 'Reliable image generation with good prompt adherence.',
    best_for: 'Ads, product shots, illustrations',
    cost_tier: 'medium',
    speed_label: 'fast',
    category: 'image',
    requires_approval: true,
    auto_recommend: false,
  },
  {
    id: 'image_midjourney',
    label: 'Midjourney-style workflow',
    description: 'High-quality, artistic outputs. Best for brand and social.',
    best_for: 'Brand visuals, lifestyle, editorial',
    cost_tier: 'medium',
    speed_label: 'medium',
    category: 'image',
    requires_approval: true,
    auto_recommend: false,
  },
  {
    id: 'image_flux',
    label: 'Flux / open model',
    description: 'Fast, low-cost open image model via your configured provider.',
    best_for: 'High-volume image sets, experiments',
    cost_tier: 'low',
    speed_label: 'fast',
    category: 'image',
    requires_approval: true,
    auto_recommend: false,
  },
  {
    id: 'image_auto',
    label: 'Auto-recommend',
    description: "We'll choose the image AI based on your style and budget.",
    best_for: "When you are not sure",
    cost_tier: 'low',
    speed_label: 'fast',
    category: 'image',
    requires_approval: true,
    auto_recommend: true,
  },

  // ── Video generation ─────────────────────────────────────────────────────
  {
    id: 'video_runway',
    label: 'Runway-style workflow',
    description: 'Text-to-video and image-to-video. Strong cinematic results.',
    best_for: 'Short ads, cinematic clips',
    cost_tier: 'high',
    speed_label: 'slow',
    category: 'video',
    requires_approval: true,
    auto_recommend: false,
  },
  {
    id: 'video_kling',
    label: 'Kling-style workflow',
    description: 'High-quality motion video generation.',
    best_for: 'Product demos, social videos',
    cost_tier: 'high',
    speed_label: 'slow',
    category: 'video',
    requires_approval: true,
    auto_recommend: false,
  },
  {
    id: 'video_pika',
    label: 'Pika-style workflow',
    description: 'Quick, social-first video clips.',
    best_for: 'TikTok, Reels, quick turnaround',
    cost_tier: 'medium',
    speed_label: 'medium',
    category: 'video',
    requires_approval: true,
    auto_recommend: false,
  },
  {
    id: 'video_img2vid',
    label: 'Image-to-video',
    description: 'Animate existing images. Lower cost, good for simple motion.',
    best_for: 'Animating product shots or illustrations',
    cost_tier: 'medium',
    speed_label: 'medium',
    category: 'video',
    requires_approval: true,
    auto_recommend: false,
  },
  {
    id: 'video_auto',
    label: 'Auto-recommend',
    description: "We'll pick the video AI based on your goal and budget.",
    best_for: "When you are not sure",
    cost_tier: 'medium',
    speed_label: 'medium',
    category: 'video',
    requires_approval: true,
    auto_recommend: true,
  },

  // ── Voice / sound ────────────────────────────────────────────────────────
  {
    id: 'voice_elevenlabs',
    label: 'ElevenLabs-style voice',
    description: 'Realistic AI voiceover. Best for ads and explainers.',
    best_for: 'Narration, ads, explainers',
    cost_tier: 'medium',
    speed_label: 'fast',
    category: 'voice',
    requires_approval: true,
    auto_recommend: true,
  },
  {
    id: 'voice_tts',
    label: 'Text-to-speech',
    description: 'Standard TTS — fast and free for drafts.',
    best_for: 'Drafts, tests, internal review',
    cost_tier: 'free',
    speed_label: 'fast',
    category: 'voice',
    requires_approval: false,
    auto_recommend: false,
  },
  {
    id: 'voice_music',
    label: 'Music / sound placeholder',
    description: 'Background music placeholder for your video draft.',
    best_for: 'Storyboards, video drafts',
    cost_tier: 'free',
    speed_label: 'fast',
    category: 'voice',
    requires_approval: false,
    auto_recommend: false,
  },
  {
    id: 'voice_none',
    label: 'No voice / sound',
    description: 'Skip voice and sound entirely.',
    best_for: 'Image-only projects',
    cost_tier: 'free',
    speed_label: 'fast',
    category: 'voice',
    requires_approval: false,
    auto_recommend: false,
  },

  // ── Script generation ────────────────────────────────────────────────────
  {
    id: 'script_ai',
    label: 'AI script writer',
    description: 'Full script generation: hook, body, CTA, shot directions.',
    best_for: 'Video ads, explainers, lessons',
    cost_tier: 'low',
    speed_label: 'fast',
    category: 'script',
    requires_approval: false,
    auto_recommend: false,
  },
  {
    id: 'script_prompt_only',
    label: 'Use prompt only',
    description: 'Skip formal script — drive video directly from prompts.',
    best_for: 'Short clips, image-heavy videos',
    cost_tier: 'free',
    speed_label: 'fast',
    category: 'script',
    requires_approval: false,
    auto_recommend: false,
  },
  {
    id: 'script_auto',
    label: 'Auto-recommend',
    description: "We'll decide whether you need a full script.",
    best_for: "When you are not sure",
    cost_tier: 'free',
    speed_label: 'fast',
    category: 'script',
    requires_approval: false,
    auto_recommend: true,
  },
];

/** Get choices for a specific category */
export function getChoicesForCategory(category) {
  return AI_CHOICE_CATALOG.filter(c => c.category === category);
}

/** Find a single choice by id */
export function getChoiceById(id) {
  return AI_CHOICE_CATALOG.find(c => c.id === id) ?? null;
}

/** Get the auto-recommend entry for a category */
export function getAutoRecommend(category) {
  return AI_CHOICE_CATALOG.find(c => c.category === category && c.auto_recommend) ?? null;
}

export const COST_TIER_LABELS = {
  free: 'Free',
  low: 'Low cost',
  medium: 'Medium cost',
  high: 'Higher cost',
};

export const SPEED_LABELS = {
  fast: 'Fast',
  medium: 'A few minutes',
  slow: 'Can take a while',
};
