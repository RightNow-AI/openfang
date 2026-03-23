export type ProviderMeta = {
  id: string;
  name: string;
  icon: string;
  description: string;
  keyUrl: string | null;
  keyPrefix: string | null;
  requiresApiKey: boolean;
  defaultUrl: string | null;
};

export const GLOBAL_PROVIDER_CREDENTIAL_WORKSPACE_ID = 'agent-catalog-global';

export const PROVIDER_DIRECTORY: ProviderMeta[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    icon: '🟢',
    description: 'GPT, image, and speech APIs.',
    keyUrl: 'https://platform.openai.com/api-keys',
    keyPrefix: 'sk-',
    requiresApiKey: true,
    defaultUrl: 'https://api.openai.com/v1',
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    icon: '🔴',
    description: 'Claude reasoning and long-form generation.',
    keyUrl: 'https://console.anthropic.com/settings/keys',
    keyPrefix: 'sk-ant-',
    requiresApiKey: true,
    defaultUrl: 'https://api.anthropic.com',
  },
  {
    id: 'gemini',
    name: 'Google Gemini',
    icon: '🔵',
    description: 'Gemini multimodal generation and image workflows.',
    keyUrl: 'https://aistudio.google.com/app/apikey',
    keyPrefix: 'AIza',
    requiresApiKey: true,
    defaultUrl: 'https://generativelanguage.googleapis.com',
  },
  {
    id: 'groq',
    name: 'Groq',
    icon: '⚡',
    description: 'Fast inference with OpenAI-compatible APIs.',
    keyUrl: 'https://console.groq.com/keys',
    keyPrefix: 'gsk_',
    requiresApiKey: true,
    defaultUrl: 'https://api.groq.com/openai/v1',
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    icon: '🌐',
    description: 'Unified model access through one key.',
    keyUrl: 'https://openrouter.ai/keys',
    keyPrefix: 'sk-or-',
    requiresApiKey: true,
    defaultUrl: 'https://openrouter.ai/api/v1',
  },
  {
    id: 'xai',
    name: 'xAI Grok',
    icon: '🤖',
    description: 'xAI-hosted Grok models.',
    keyUrl: 'https://console.x.ai/',
    keyPrefix: 'xai-',
    requiresApiKey: true,
    defaultUrl: 'https://api.x.ai/v1',
  },
  {
    id: 'minimax',
    name: 'MiniMax',
    icon: '🧠',
    description: 'MiniMax multimodal APIs.',
    keyUrl: 'https://www.minimaxi.com/',
    keyPrefix: null,
    requiresApiKey: true,
    defaultUrl: 'https://api.minimax.io/v1',
  },
  {
    id: 'runway',
    name: 'Runway',
    icon: '🎬',
    description: 'Video and media generation workflows.',
    keyUrl: 'https://app.runwayml.com/account/api-keys',
    keyPrefix: null,
    requiresApiKey: true,
    defaultUrl: 'https://api.dev.runwayml.com/v1',
  },
  {
    id: 'elevenlabs',
    name: 'ElevenLabs',
    icon: '🎙️',
    description: 'Speech synthesis and voice cloning APIs.',
    keyUrl: 'https://elevenlabs.io/app/settings/api-keys',
    keyPrefix: null,
    requiresApiKey: true,
    defaultUrl: 'https://api.elevenlabs.io/v1',
  },
  {
    id: 'ollama',
    name: 'Ollama (local)',
    icon: '🏠',
    description: 'Local runtime, no API key required.',
    keyUrl: null,
    keyPrefix: null,
    requiresApiKey: false,
    defaultUrl: 'http://127.0.0.1:11434/v1',
  },
];

const PROVIDER_ALIAS_MAP: Record<string, string> = {
  anthropic: 'anthropic',
  claude: 'anthropic',
  openai: 'openai',
  gpt: 'openai',
  gemini: 'gemini',
  google: 'gemini',
  'google gemini': 'gemini',
  groq: 'groq',
  openrouter: 'openrouter',
  xai: 'xai',
  grok: 'xai',
  minimax: 'minimax',
  runway: 'runway',
  runwayml: 'runway',
  'runway + ffmpeg': 'runway',
  elevenlabs: 'elevenlabs',
  'elevenlabs-style voice': 'elevenlabs',
  ollama: 'ollama',
};

export const STUDIO_VOICE_PROVIDER_IDS = ['elevenlabs', 'openai'] as const;
export const STUDIO_VISUAL_PROVIDER_IDS = ['runway', 'gemini', 'openai'] as const;

function normalizeProviderKey(value: string) {
  return value.toLowerCase().trim().replace(/[^a-z0-9]+/g, ' ').trim();
}

export function normalizeProviderId(value: unknown) {
  if (typeof value !== 'string') {
    return null;
  }

  const normalized = normalizeProviderKey(value);
  return PROVIDER_ALIAS_MAP[normalized] ?? null;
}

export function getProviderMeta(value: unknown) {
  const providerId = normalizeProviderId(value);
  if (!providerId) {
    return null;
  }

  return PROVIDER_DIRECTORY.find((provider) => provider.id === providerId) ?? null;
}

export function getProvidersByIds(providerIds: readonly string[]) {
  return providerIds
    .map((providerId) => PROVIDER_DIRECTORY.find((provider) => provider.id === providerId) ?? null)
    .filter((provider): provider is ProviderMeta => Boolean(provider));
}