const RECOVERY_EXPORT_SCHEMA = 'ctox.browser-recovery.v2';
const RECOVERY_CRYPTO_SCHEMA = 'ctox.browser-recovery.crypto.v1';
const PBKDF2_ITERATIONS = 600_000;

export async function encryptRecoveryArtifact(value, passphrase) {
  requirePassphrase(passphrase);
  const subtle = requireSubtle();
  const salt = randomBytes(16);
  const iv = randomBytes(12);
  const key = await deriveRecoveryKey(subtle, passphrase, salt, ['encrypt']);
  const plaintext = new TextEncoder().encode(JSON.stringify(value));
  const ciphertext = await subtle.encrypt({ name: 'AES-GCM', iv }, key, plaintext);
  return {
    schema: RECOVERY_CRYPTO_SCHEMA,
    contentSchema: RECOVERY_EXPORT_SCHEMA,
    kdf: { name: 'PBKDF2', hash: 'SHA-256', iterations: PBKDF2_ITERATIONS, saltBase64: bytesToBase64(salt) },
    cipher: { name: 'AES-GCM', ivBase64: bytesToBase64(iv) },
    ciphertextBase64: bytesToBase64(new Uint8Array(ciphertext)),
  };
}

export async function decryptRecoveryArtifact(envelope, passphrase) {
  requirePassphrase(passphrase);
  if (envelope?.schema !== RECOVERY_CRYPTO_SCHEMA) {
    throw recoveryCryptoError('recovery_integrity_failed', 'Unsupported recovery encryption envelope.');
  }
  try {
    const subtle = requireSubtle();
    const salt = base64ToBytes(envelope.kdf?.saltBase64 || '');
    const iv = base64ToBytes(envelope.cipher?.ivBase64 || '');
    const ciphertext = base64ToBytes(envelope.ciphertextBase64 || '');
    const iterations = Number(envelope.kdf?.iterations || 0);
    if (
      envelope.kdf?.name !== 'PBKDF2'
      || envelope.kdf?.hash !== 'SHA-256'
      || envelope.cipher?.name !== 'AES-GCM'
      || iterations < 100_000
      || salt.byteLength !== 16
      || iv.byteLength !== 12
      || ciphertext.byteLength === 0
    ) {
      throw new Error('invalid recovery encryption parameters');
    }
    const key = await deriveRecoveryKey(subtle, passphrase, salt, ['decrypt'], iterations);
    const plaintext = await subtle.decrypt({ name: 'AES-GCM', iv }, key, ciphertext);
    const parsed = JSON.parse(new TextDecoder().decode(plaintext));
    if (parsed?.schema !== RECOVERY_EXPORT_SCHEMA) throw new Error('unexpected recovery content schema');
    return parsed;
  } catch (cause) {
    throw recoveryCryptoError('recovery_integrity_failed', 'Recovery export could not be decrypted or failed integrity validation.', cause);
  }
}

export async function sha256Json(value) {
  const bytes = new TextEncoder().encode(canonicalJson(value));
  const digest = await requireSubtle().digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

async function deriveRecoveryKey(subtle, passphrase, salt, usages, iterations = PBKDF2_ITERATIONS) {
  const base = await subtle.importKey(
    'raw',
    new TextEncoder().encode(String(passphrase)),
    'PBKDF2',
    false,
    ['deriveKey'],
  );
  return subtle.deriveKey(
    { name: 'PBKDF2', hash: 'SHA-256', salt, iterations },
    base,
    { name: 'AES-GCM', length: 256 },
    false,
    usages,
  );
}

function requireSubtle() {
  if (!globalThis.crypto?.subtle) throw recoveryCryptoError('recovery_integrity_failed', 'WebCrypto is required for recovery export encryption.');
  return globalThis.crypto.subtle;
}

function requirePassphrase(passphrase) {
  if (String(passphrase || '').length < 8) {
    throw recoveryCryptoError('recovery_integrity_failed', 'Recovery export passphrase must contain at least eight characters.');
  }
}

function randomBytes(length) {
  const bytes = new Uint8Array(length);
  globalThis.crypto.getRandomValues(bytes);
  return bytes;
}

function bytesToBase64(bytes) {
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return globalThis.btoa(binary);
}

function base64ToBytes(value) {
  const binary = globalThis.atob(String(value || ''));
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function recoveryCryptoError(code, message, cause = null) {
  const error = new Error(message, cause ? { cause } : undefined);
  error.code = code;
  error.retryable = false;
  return error;
}

export const recoveryCryptoTestInternals = Object.freeze({
  RECOVERY_EXPORT_SCHEMA,
  RECOVERY_CRYPTO_SCHEMA,
  PBKDF2_ITERATIONS,
  canonicalJson,
});
