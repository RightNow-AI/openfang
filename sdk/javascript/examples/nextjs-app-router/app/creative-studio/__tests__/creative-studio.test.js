/**
 * Tests for Creative Studio wizard state helpers and AI choice catalog
 */
import { describe, it, expect } from 'vitest';
import {
  emptyWizardState,
  applyStarterDefaults,
  canAdvance,
  needsImage,
  needsVideo,
  buildPlanPreview,
  creationTypeLabel,
} from '../lib/creative-ui';
import {
  AI_CHOICE_CATALOG,
  getChoicesForCategory,
  getChoiceById,
  getAutoRecommend,
} from '../config/ai-choice-catalog';
import { CREATIVE_STARTERS } from '../config/creative-starters';

// ── emptyWizardState ───────────────────────────────────────────────────────

describe('emptyWizardState', () => {
  it('starts at step 1', () => {
    expect(emptyWizardState().step).toBe(1);
  });

  it('defaults all AI choices to auto-recommend or sensible fallback', () => {
    const state = emptyWizardState();
    expect(state.ai_choices.prompt).toBe('prompt_auto');
    expect(state.ai_choices.image).toBe('image_auto');
    expect(state.ai_choices.video).toBe('video_auto');
  });

  it('initialises all required string fields as empty strings', () => {
    const s = emptyWizardState();
    expect(s.topic).toBe('');
    expect(s.creation_type).toBe('');
    expect(s.goal).toBe('');
  });
});

// ── applyStarterDefaults ───────────────────────────────────────────────────

describe('applyStarterDefaults', () => {
  it('copies creation_type and goal from starter', () => {
    const starter = CREATIVE_STARTERS[0]; // ad-image-pack → image / ad
    const state = applyStarterDefaults(starter);
    expect(state.creation_type).toBe('image');
    expect(state.goal).toBe('ad');
  });

  it('copies wizard_defaults fields into the state', () => {
    const starter = CREATIVE_STARTERS.find(s => s.id === 'short-video-ad');
    const state = applyStarterDefaults(starter);
    expect(state.duration).toBe('15-30 seconds');
  });

  it('sets name from starter title', () => {
    const starter = CREATIVE_STARTERS[0];
    const state = applyStarterDefaults(starter);
    expect(state.name).toBe(starter.title);
  });
});

// ── canAdvance ─────────────────────────────────────────────────────────────

describe('canAdvance', () => {
  it('blocks step 1 if creation_type is missing', () => {
    const s = { ...emptyWizardState(), step: 1, goal: 'ad' };
    expect(canAdvance(s)).toBe(false);
  });

  it('blocks step 1 if goal is missing', () => {
    const s = { ...emptyWizardState(), step: 1, creation_type: 'image' };
    expect(canAdvance(s)).toBe(false);
  });

  it('allows step 1 when both fields set', () => {
    const s = { ...emptyWizardState(), step: 1, creation_type: 'image', goal: 'ad' };
    expect(canAdvance(s)).toBe(true);
  });

  it('blocks step 2 if topic is empty', () => {
    const s = { ...emptyWizardState(), step: 2, topic: '' };
    expect(canAdvance(s)).toBe(false);
  });

  it('allows step 2 with a topic', () => {
    const s = { ...emptyWizardState(), step: 2, topic: 'My product' };
    expect(canAdvance(s)).toBe(true);
  });

  it('always allows advance from steps 3+', () => {
    for (const step of [3, 4, 5]) {
      expect(canAdvance({ ...emptyWizardState(), step })).toBe(true);
    }
  });
});

// ── needsImage / needsVideo ────────────────────────────────────────────────

describe('needsImage / needsVideo', () => {
  it('image type — needs image, no video', () => {
    expect(needsImage('image')).toBe(true);
    expect(needsVideo('image')).toBe(false);
  });

  it('video type — needs video, no image', () => {
    expect(needsImage('video')).toBe(false);
    expect(needsVideo('video')).toBe(true);
  });

  it('image+video — needs both', () => {
    expect(needsImage('image+video')).toBe(true);
    expect(needsVideo('image+video')).toBe(true);
  });
});

// ── buildPlanPreview ───────────────────────────────────────────────────────

describe('buildPlanPreview', () => {
  it('image-only plan has image step but no video step', () => {
    const s = { ...emptyWizardState(), creation_type: 'image' };
    const steps = buildPlanPreview(s);
    const labels = steps.map(s => s.label);
    expect(labels.some(l => l.includes('image'))).toBe(true);
    expect(labels.some(l => l.includes('video'))).toBe(false);
  });

  it('video-only plan includes script and video steps', () => {
    const s = { ...emptyWizardState(), creation_type: 'video' };
    const steps = buildPlanPreview(s);
    const labels = steps.map(s => s.label);
    expect(labels.some(l => l.includes('script'))).toBe(true);
    expect(labels.some(l => l.includes('video'))).toBe(true);
  });

  it('image+video plan has both image and video steps', () => {
    const s = { ...emptyWizardState(), creation_type: 'image+video' };
    const steps = buildPlanPreview(s);
    const labels = steps.map(s => s.label);
    expect(labels.some(l => l.includes('image'))).toBe(true);
    expect(labels.some(l => l.includes('video'))).toBe(true);
  });

  it('approval-gated steps have requires_approval = true', () => {
    const s = { ...emptyWizardState(), creation_type: 'image' };
    const steps = buildPlanPreview(s);
    const imageStep = steps.find(st => st.id.startsWith('images'));
    expect(imageStep?.requires_approval).toBe(true);
  });

  it('prompt step does not require approval', () => {
    const s = { ...emptyWizardState(), creation_type: 'image' };
    const steps = buildPlanPreview(s);
    const promptStep = steps.find(st => st.label.includes('prompt'));
    expect(promptStep?.requires_approval).toBe(false);
  });
});

// ── creationTypeLabel ──────────────────────────────────────────────────────

describe('creationTypeLabel', () => {
  it('maps types to human labels', () => {
    expect(creationTypeLabel('image')).toBe('Images');
    expect(creationTypeLabel('video')).toBe('Videos');
    expect(creationTypeLabel('image+video')).toBe('Images + Videos');
  });
});

// ── AI choice catalog ──────────────────────────────────────────────────────

describe('AI_CHOICE_CATALOG', () => {
  it('contains entries for all five categories', () => {
    const cats = new Set(AI_CHOICE_CATALOG.map(c => c.category));
    expect(cats.has('prompt')).toBe(true);
    expect(cats.has('image')).toBe(true);
    expect(cats.has('video')).toBe(true);
    expect(cats.has('voice')).toBe(true);
    expect(cats.has('script')).toBe(true);
  });

  it('each entry has required fields', () => {
    for (const c of AI_CHOICE_CATALOG) {
      expect(c.id).toBeTruthy();
      expect(c.label).toBeTruthy();
      expect(c.category).toBeTruthy();
      expect(typeof c.requires_approval).toBe('boolean');
    }
  });

  it('each category has exactly one auto_recommend entry', () => {
    const categories = ['prompt', 'image', 'video', 'voice', 'script'];
    for (const cat of categories) {
      const autos = AI_CHOICE_CATALOG.filter(c => c.category === cat && c.auto_recommend);
      expect(autos).toHaveLength(1);
    }
  });
});

describe('getChoicesForCategory', () => {
  it('returns only entries for the requested category', () => {
    const imageChoices = getChoicesForCategory('image');
    expect(imageChoices.every(c => c.category === 'image')).toBe(true);
    expect(imageChoices.length).toBeGreaterThan(0);
  });
});

describe('getChoiceById', () => {
  it('finds a known entry', () => {
    const c = getChoiceById('image_openai');
    expect(c?.label).toBeTruthy();
  });

  it('returns null for unknown id', () => {
    expect(getChoiceById('does_not_exist')).toBeNull();
  });
});

describe('getAutoRecommend', () => {
  it('returns the auto-recommend entry for each category', () => {
    for (const cat of ['prompt', 'image', 'video']) {
      const c = getAutoRecommend(cat);
      expect(c?.auto_recommend).toBe(true);
      expect(c?.category).toBe(cat);
    }
  });
});

// ── CREATIVE_STARTERS ──────────────────────────────────────────────────────

describe('CREATIVE_STARTERS', () => {
  it('has 6 starter templates', () => {
    expect(CREATIVE_STARTERS).toHaveLength(6);
  });

  it('each starter has required fields', () => {
    for (const s of CREATIVE_STARTERS) {
      expect(s.id).toBeTruthy();
      expect(s.title).toBeTruthy();
      expect(['image', 'video', 'image+video']).toContain(s.creation_type);
      expect(s.ai_categories_needed.length).toBeGreaterThan(0);
    }
  });

  it('video starters require video AI category', () => {
    const videoStarters = CREATIVE_STARTERS.filter(s =>
      s.creation_type === 'video' || s.creation_type === 'image+video'
    );
    for (const s of videoStarters) {
      expect(s.ai_categories_needed).toContain('video');
    }
  });
});
