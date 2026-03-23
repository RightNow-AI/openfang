import { createCipheriv, createDecipheriv, randomBytes } from 'node:crypto';

const ALGORITHM = 'aes-256-gcm';
const DEV_FALLBACK_MASTER_KEY = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';

function readMasterKey() {
  const configured = String(process.env.OPENFANG_SECRETS_MASTER_KEY || '').trim();
  const value = configured || (process.env.NODE_ENV === 'production' ? '' : DEV_FALLBACK_MASTER_KEY);

  if (!value) {
    throw new Error('OPENFANG_SECRETS_MASTER_KEY is required in production');
  }

  if (!/^[0-9a-fA-F]{64}$/.test(value)) {
    throw new Error('OPENFANG_SECRETS_MASTER_KEY must be a 64-character hex string');
  }

  return Buffer.from(value, 'hex');
}

function buildAad(workspaceId: string, providerId: string, version: number) {
  return Buffer.from(`${workspaceId}:${providerId}:${version}`, 'utf8');
}

export function encryptSecret(plainText: string, workspaceId: string, providerId: string, version = 1) {
  const masterKey = readMasterKey();
  const iv = randomBytes(12);
  const cipher = createCipheriv(ALGORITHM, masterKey, iv);

  cipher.setAAD(buildAad(workspaceId, providerId, version));

  let ciphertext = cipher.update(plainText, 'utf8', 'hex');
  ciphertext += cipher.final('hex');

  return {
    ciphertext,
    nonce: iv.toString('hex'),
    authTag: cipher.getAuthTag().toString('hex'),
    last4: plainText.slice(-4),
  };
}

export function decryptSecret(
  ciphertext: string,
  nonce: string,
  authTag: string,
  workspaceId: string,
  providerId: string,
  version = 1,
) {
  const masterKey = readMasterKey();
  const decipher = createDecipheriv(ALGORITHM, masterKey, Buffer.from(nonce, 'hex'));

  decipher.setAAD(buildAad(workspaceId, providerId, version));
  decipher.setAuthTag(Buffer.from(authTag, 'hex'));

  let decrypted = decipher.update(ciphertext, 'hex', 'utf8');
  decrypted += decipher.final('utf8');

  return decrypted;
}