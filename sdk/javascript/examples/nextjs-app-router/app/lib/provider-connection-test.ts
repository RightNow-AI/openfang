import { getProviderMeta, normalizeProviderId } from './provider-directory';

type ProviderConnectionResult = {
  ok: boolean;
  message: string;
  status?: number;
  metadata?: Record<string, unknown>;
};

function trimList(values: unknown[], mapper: (value: unknown) => string | null, limit = 3) {
  const mapped = values.map(mapper).filter((value): value is string => Boolean(value));
  return mapped.slice(0, limit);
}

function openAiLikeMetadata(payload: unknown, endpoint: string) {
  const models = Array.isArray((payload as { data?: unknown[] } | null)?.data)
    ? (payload as { data: unknown[] }).data
    : [];

  return {
    endpoint,
    modelCount: models.length,
    sampleModels: trimList(models, (model) => {
      if (!model || typeof model !== 'object') return null;
      const record = model as Record<string, unknown>;
      return typeof record.id === 'string' ? record.id : null;
    }),
  };
}

function anthropicMetadata(payload: unknown, endpoint: string) {
  const models = Array.isArray((payload as { data?: unknown[] } | null)?.data)
    ? (payload as { data: unknown[] }).data
    : [];

  return {
    endpoint,
    modelCount: models.length,
    sampleModels: trimList(models, (model) => {
      if (!model || typeof model !== 'object') return null;
      const record = model as Record<string, unknown>;
      return typeof record.id === 'string' ? record.id : null;
    }),
  };
}

function geminiMetadata(payload: unknown, endpoint: string) {
  const models = Array.isArray((payload as { models?: unknown[] } | null)?.models)
    ? (payload as { models: unknown[] }).models
    : [];

  return {
    endpoint,
    modelCount: models.length,
    sampleModels: trimList(models, (model) => {
      if (!model || typeof model !== 'object') return null;
      const record = model as Record<string, unknown>;
      return typeof record.name === 'string' ? record.name.replace(/^models\//, '') : null;
    }),
  };
}

function elevenLabsMetadata(payload: unknown, endpoint: string) {
  const models = Array.isArray(payload)
    ? payload
    : Array.isArray((payload as { models?: unknown[] } | null)?.models)
    ? (payload as { models: unknown[] }).models
    : [];

  return {
    endpoint,
    modelCount: models.length,
    sampleModels: trimList(models, (model) => {
      if (!model || typeof model !== 'object') return null;
      const record = model as Record<string, unknown>;
      if (typeof record.name === 'string') return record.name;
      if (typeof record.model_id === 'string') return record.model_id;
      return null;
    }),
  };
}

function runwayMetadata(payload: unknown, endpoint: string) {
  const tasks = Array.isArray((payload as { data?: unknown[] } | null)?.data)
    ? (payload as { data: unknown[] }).data
    : Array.isArray(payload)
    ? payload
    : [];

  return {
    endpoint,
    taskCount: tasks.length,
  };
}

function successMessage(providerName: string, metadata?: Record<string, unknown>) {
  if (!metadata) {
    return `${providerName} connection succeeded.`;
  }

  if (typeof metadata.modelCount === 'number') {
    return `${providerName} connection succeeded. ${metadata.modelCount} model${metadata.modelCount === 1 ? '' : 's'} available.`;
  }

  if (typeof metadata.taskCount === 'number') {
    return `${providerName} connection succeeded. ${metadata.taskCount} recent task${metadata.taskCount === 1 ? '' : 's'} visible.`;
  }

  return `${providerName} connection succeeded.`;
}

async function requestJson(url: string, init: RequestInit) {
  const response = await fetch(url, {
    cache: 'no-store',
    ...init,
  });

  const text = await response.text().catch(() => '');
  let payload: unknown = null;
  try {
    payload = text ? JSON.parse(text) : null;
  } catch {
    payload = text;
  }

  return { response, payload };
}

function responseMessage(payload: unknown, fallback: string) {
  if (payload && typeof payload === 'object') {
    const record = payload as Record<string, unknown>;
    const error = record.error;
    if (typeof error === 'string' && error.trim()) return error;
    if (error && typeof error === 'object') {
      const nested = error as Record<string, unknown>;
      if (typeof nested.message === 'string' && nested.message.trim()) return nested.message;
    }
    if (typeof record.message === 'string' && record.message.trim()) return record.message;
    if (typeof record.detail === 'string' && record.detail.trim()) return record.detail;
  }

  if (typeof payload === 'string' && payload.trim()) {
    return payload;
  }

  return fallback;
}

export async function testProviderConnection(providerId: string, apiKey: string): Promise<ProviderConnectionResult> {
  const normalizedProviderId = normalizeProviderId(providerId);
  if (!normalizedProviderId) {
    return { ok: false, message: `Unknown provider: ${providerId}` };
  }

  const provider = getProviderMeta(normalizedProviderId);
  if (!provider?.requiresApiKey) {
    return { ok: true, message: `${provider?.name ?? providerId} does not require an API key.` };
  }

  try {
    if (normalizedProviderId === 'anthropic') {
      const endpoint = `${provider.defaultUrl}/v1/models`;
      const { response, payload } = await requestJson(endpoint, {
        method: 'GET',
        headers: {
          'x-api-key': apiKey,
          'anthropic-version': '2023-06-01',
        },
      });

      const metadata = anthropicMetadata(payload, endpoint);
      return response.ok
        ? { ok: true, message: successMessage(provider.name, metadata), status: response.status, metadata }
        : { ok: false, message: responseMessage(payload, `${provider.name} rejected the key.`), status: response.status };
    }

    if (normalizedProviderId === 'gemini') {
      const endpoint = `${provider.defaultUrl}/v1beta/models`;
      const { response, payload } = await requestJson(`${endpoint}?key=${encodeURIComponent(apiKey)}`, {
        method: 'GET',
      });

      const metadata = geminiMetadata(payload, endpoint);
      return response.ok
        ? { ok: true, message: successMessage(provider.name, metadata), status: response.status, metadata }
        : { ok: false, message: responseMessage(payload, `${provider.name} rejected the key.`), status: response.status };
    }

    if (normalizedProviderId === 'elevenlabs') {
      const endpoint = `${provider.defaultUrl}/models`;
      const { response, payload } = await requestJson(endpoint, {
        method: 'GET',
        headers: {
          'xi-api-key': apiKey,
        },
      });

      const metadata = elevenLabsMetadata(payload, endpoint);
      return response.ok
        ? { ok: true, message: successMessage(provider.name, metadata), status: response.status, metadata }
        : { ok: false, message: responseMessage(payload, `${provider.name} rejected the key.`), status: response.status };
    }

    if (normalizedProviderId === 'runway') {
      const endpoint = `${provider.defaultUrl}/tasks`;
      const { response, payload } = await requestJson(endpoint, {
        method: 'GET',
        headers: {
          Authorization: `Bearer ${apiKey}`,
        },
      });

      const metadata = runwayMetadata(payload, endpoint);
      return response.ok
        ? { ok: true, message: successMessage(provider.name, metadata), status: response.status, metadata }
        : { ok: false, message: responseMessage(payload, `${provider.name} rejected the key.`), status: response.status };
    }

    const endpoint = `${provider.defaultUrl}/models`;
    const { response, payload } = await requestJson(endpoint, {
      method: 'GET',
      headers: {
        Authorization: `Bearer ${apiKey}`,
      },
    });

    const metadata = openAiLikeMetadata(payload, endpoint);
    return response.ok
      ? { ok: true, message: successMessage(provider.name, metadata), status: response.status, metadata }
      : { ok: false, message: responseMessage(payload, `${provider.name} rejected the key.`), status: response.status };
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : 'Connection test failed.',
    };
  }
}