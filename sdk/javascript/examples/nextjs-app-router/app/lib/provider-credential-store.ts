import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { decryptSecret, encryptSecret } from './crypto-vault';
import { GLOBAL_PROVIDER_CREDENTIAL_WORKSPACE_ID, normalizeProviderId } from './provider-directory';

type ProviderCredentialRecord = {
  workspaceId: string;
  providerId: string;
  ciphertext: string;
  nonce: string;
  authTag: string;
  last4: string;
  keyVersion: number;
  createdAt: string;
  updatedAt: string;
};

type ProviderCredentialStore = {
  version: number;
  credentials: Record<string, ProviderCredentialRecord>;
};

const DATA_DIR = path.join(process.cwd(), '.data');
const STORE_PATH = path.join(DATA_DIR, 'openfang-provider-credentials.json');

let writeQueue: Promise<unknown> = Promise.resolve();

function emptyStore(): ProviderCredentialStore {
  return {
    version: 1,
    credentials: {},
  };
}

function normalizeId(value: string) {
  return String(value || '').trim();
}

function normalizeProviderKey(providerId: string) {
  return normalizeProviderId(providerId) ?? normalizeId(providerId);
}

function credentialKey(workspaceId: string, providerId: string) {
  return `${workspaceId}::${providerId}`;
}

async function ensureDataDir() {
  await mkdir(DATA_DIR, { recursive: true });
}

async function readStore(): Promise<ProviderCredentialStore> {
  await ensureDataDir();

  try {
    const raw = await readFile(STORE_PATH, 'utf8');
    const parsed = JSON.parse(raw) as Partial<ProviderCredentialStore>;
    return {
      version: Number(parsed.version || 1),
      credentials: parsed.credentials && typeof parsed.credentials === 'object' ? parsed.credentials : {},
    };
  } catch (error) {
    if (error && typeof error === 'object' && 'code' in error && error.code === 'ENOENT') {
      return emptyStore();
    }
    throw error;
  }
}

async function writeStore(store: ProviderCredentialStore) {
  await ensureDataDir();
  await writeFile(STORE_PATH, `${JSON.stringify(store, null, 2)}\n`, 'utf8');
}

function runExclusive<T>(operation: () => Promise<T>) {
  const nextWrite = writeQueue.then(operation);
  writeQueue = nextWrite.catch(() => undefined);
  return nextWrite;
}

export async function upsertProviderCredential(input: {
  workspaceId: string;
  providerId: string;
  apiKey: string;
  keyVersion?: number;
}) {
  const workspaceId = normalizeId(input.workspaceId);
  const providerId = normalizeProviderKey(input.providerId);
  const apiKey = String(input.apiKey || '').trim();
  const keyVersion = Number(input.keyVersion || 1) || 1;

  if (!workspaceId || !providerId || !apiKey) {
    throw new Error('workspaceId, providerId, and apiKey are required');
  }

  return runExclusive(async () => {
    const store = await readStore();
    const key = credentialKey(workspaceId, providerId);
    const now = new Date().toISOString();
    const existing = store.credentials[key];
    const encrypted = encryptSecret(apiKey, workspaceId, providerId, keyVersion);

    const record: ProviderCredentialRecord = {
      workspaceId,
      providerId,
      ciphertext: encrypted.ciphertext,
      nonce: encrypted.nonce,
      authTag: encrypted.authTag,
      last4: encrypted.last4,
      keyVersion,
      createdAt: existing?.createdAt || now,
      updatedAt: now,
    };

    store.credentials[key] = record;
    await writeStore(store);

    return {
      providerId,
      last4: record.last4,
      connected: true,
      updatedAt: record.updatedAt,
      keyVersion: record.keyVersion,
    };
  });
}

export async function listProviderCredentialMetadata(workspaceId: string) {
  const normalizedWorkspaceId = normalizeId(workspaceId);
  if (!normalizedWorkspaceId) {
    throw new Error('workspaceId is required');
  }

  const store = await readStore();
  return Object.values(store.credentials)
    .filter((record) => record.workspaceId === normalizedWorkspaceId)
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
    .map((record) => ({
      providerId: record.providerId,
      last4: record.last4,
      connected: true,
      updatedAt: record.updatedAt,
      keyVersion: record.keyVersion,
    }));
}

export async function getDecryptedProviderKey(
  workspaceId: string,
  providerId: string,
  options?: { allowGlobalFallback?: boolean },
) {
  const normalizedWorkspaceId = normalizeId(workspaceId);
  const normalizedProviderId = normalizeProviderKey(providerId);
  if (!normalizedWorkspaceId || !normalizedProviderId) {
    throw new Error('workspaceId and providerId are required');
  }

  const store = await readStore();
  const allowGlobalFallback = options?.allowGlobalFallback ?? false;
  const record =
    store.credentials[credentialKey(normalizedWorkspaceId, normalizedProviderId)] ??
    (allowGlobalFallback && normalizedWorkspaceId !== GLOBAL_PROVIDER_CREDENTIAL_WORKSPACE_ID
      ? store.credentials[credentialKey(GLOBAL_PROVIDER_CREDENTIAL_WORKSPACE_ID, normalizedProviderId)]
      : undefined);
  if (!record) {
    return null;
  }

  return {
    apiKey: decryptSecret(
      record.ciphertext,
      record.nonce,
      record.authTag,
      record.workspaceId,
      record.providerId,
      record.keyVersion,
    ),
    last4: record.last4,
    updatedAt: record.updatedAt,
    keyVersion: record.keyVersion,
  };
}

export async function deleteProviderCredential(workspaceId: string, providerId: string) {
  const normalizedWorkspaceId = normalizeId(workspaceId);
  const normalizedProviderId = normalizeProviderKey(providerId);
  if (!normalizedWorkspaceId || !normalizedProviderId) {
    throw new Error('workspaceId and providerId are required');
  }

  return runExclusive(async () => {
    const store = await readStore();
    const key = credentialKey(normalizedWorkspaceId, normalizedProviderId);
    const existing = store.credentials[key];
    if (!existing) {
      return false;
    }

    delete store.credentials[key];
    await writeStore(store);
    return true;
  });
}