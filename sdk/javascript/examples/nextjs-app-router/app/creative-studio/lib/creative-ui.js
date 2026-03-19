// Shared UI helpers for Creative Studio

/** Empty wizard state — all fields initialised */
export function emptyWizardState() {
  return {
    step: 1,
    creation_type: '',
    goal: '',
    name: '',
    topic: '',
    offer: '',
    audience: '',
    platform: '',
    desired_outcome: '',
    notes: '',
    style_description: '',
    visual_keywords_raw: '',
    words_to_avoid_raw: '',
    reference_links_raw: '',
    aspect_ratio: '',
    duration: '',
    voice_tone: '',
    ai_choices: {
      prompt: 'prompt_auto',
      image: 'image_auto',
      video: 'video_auto',
      voice: 'voice_tts',
      script: 'script_auto',
    },
  };
}

/** Merge a starter template's defaults into the empty wizard state */
export function applyStarterDefaults(starter) {
  const base = emptyWizardState();
  const d = starter.wizard_defaults ?? {};
  return {
    ...base,
    creation_type: starter.creation_type,
    goal: starter.goal,
    name: starter.title,
    ...d,
  };
}

/** Whether a given creation_type needs image tools */
export function needsImage(type) {
  return type === 'image' || type === 'image+video';
}

/** Whether a given creation_type needs video tools */
export function needsVideo(type) {
  return type === 'video' || type === 'image+video';
}

/** Human-readable label for creation type */
export function creationTypeLabel(type) {
  return { image: 'Images', video: 'Videos', 'image+video': 'Images + Videos' }[type] ?? type;
}

/** Check if wizard state on step N is valid enough to advance */
export function canAdvance(state) {
  if (state.step === 1) return !!state.creation_type && !!state.goal;
  if (state.step === 2) return !!state.topic.trim();
  return true; // steps 3-5 are all optional / auto-recommend
}

/** Build plan steps from wizard state (client-side preview, server confirms) */
export function buildPlanPreview(state) {
  const steps = [];
  const needImg = needsImage(state.creation_type);
  const needVid = needsVideo(state.creation_type);

  steps.push({ id: 'hooks', label: 'Generate hooks and angles', requires_approval: false });
  steps.push({ id: 'prompts', label: 'Generate image prompts', requires_approval: false });
  if (needVid) steps.push({ id: 'script', label: 'Generate script', requires_approval: false });
  if (needImg)  steps.push({ id: 'images', label: 'Generate images', requires_approval: true,  note: 'Waits for your approval' });
  if (needVid)  steps.push({ id: 'voice',  label: 'Generate voice draft', requires_approval: true, note: 'Waits for your approval' });
  if (needVid)  steps.push({ id: 'video',  label: 'Generate video shots',  requires_approval: true, note: 'Waits for your approval' });
  if (needVid)  steps.push({ id: 'final',  label: 'Assemble final draft',   requires_approval: true, note: 'Waits for your approval' });

  return steps.map((s, i) => ({
    ...s,
    id: `${s.id}_${i}`,
    status: 'pending',
    approval_state: 'pending',
    description: s.note ?? (s.requires_approval ? 'This step pauses for your approval.' : 'Runs automatically.'),
  }));
}
