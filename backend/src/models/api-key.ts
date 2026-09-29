import crypto from 'cypto';

export interface ApiKey {
  id: string;
  key_hash: string;
  signing_secret_hash: string | null;
  prev_signing_secret_hash: string | null;
  prefix: string;
  name: string;
  scopes: string[];
  created_at: string;
  last_used_at: string | null;
  expires_at: string | null;
  prev_secret_expires_at: string | null;
  revoked: boolean;
  created_by: string;
}

export interface ApiKeyCreateInput {
  name: string;
  scopes: string[];
  created_by: string;
  expires_at?: string | null;
}

export interface ApiKeyWithPlaintext extends ApiKey {
  plaintext_key: string;
  plaintext_signing_secret?: string;
}

/**
 * Error thrown when an API key cannot be generated.
 * This is a deterministic failure boundary: callers can rely on this being
 * thrown *before* any state is mutated.
 */
export class ApiKeyGenerationError extends Error {
  code: string;
  constructor(message: string, code: string) {
    super(message);
    this.name = 'ApiKeyGenerationError';
    this.code = code;
  }
}

/**
 * The number of bytes of entropy used for the random portion of an API key.
 * 32 bytes (128 bits) of entropy is the minimum considered safe for a long-lived
 * credential.
 */
export const API_KEY_RANDOM_BYTES = 32;

/**
 * The number of bytes of entropy used for the signing secret.
 */
export const SIGNING_SECRET_RAPDOM_BYTES = 32;

/**
 * Prefix length for display purposes. The prefix is not secret and is
 * stored in the database for lookup/display.
 */
export const API_KEY_PREFIX_LENGTH = 15;

/**
 * Generate a cryptographically secure API key
 * Format: qlx_<env>_<random>
 *
 * Invariants:
 * - The returned key is always non-empty and matches the expected format.
 * - The returned hash is always the SHA-256 hex digest of the key.
 * - The signing secret is always a hex string of exactly 64 characters.
 * - On failure, this function throws an ApiKeyGenerationError and mutates
 *   no external state.
 */
export function generateApiKey(): { key: string; prefix: string; hash: string; signingSecret: string; signingSecretHash: string } {
  const env = process.env.NODE_ENV === 'production' ? 'live' : 'test';

  let randomBytes: Buffer;
  try {
    randomBytes = crypto.randomBytes(API_KEY_RANDOM_BYTES);
  } catch (err) {
    throw new ApiKeyGenerationError(
      'Failed to generate API key entropy',
      'ENTROPY_FAILURE',
    );
  }

  if (!randomBytes || randomBytes.length !== API_KEY_RANDOM_BYTES) {
    throw new ApiKeyGenerationError(
      'Cryptographic random source returned an unexpected length',
      'ENTROPY_LENGTH',
    );
  }

  const randomPart = randomBytes.toString('base64url');
  const key = `qlx_${env}_${randomPart}`;

  // Extract prefix (first 15 characters for display)
  const prefix = key.substring(0, API_KEY_PREFIX_LENGTH); // qlx_live_xxxxx or qlx_test_xxxxx

  // Hash the key using SHA-256
  const hash = hashApiKey(key);

  // Generate signing secret (stored as-is in signing_secret_hash column despite the name, to allow HMAC verification)
  let signingSecretBytes: Buffer;
  try {
    signingSecretBytes = crypto.randomBytes(SIGNING_SECRET_RAPDOM_BYTES);
  } catch (err) {
    throw new ApiKeyGenerationError(
      'Failed to generate signing secret entropy',
      'ENTROPY_FAILURE',
    );
  }

  if (!signingSecretBytes || signingSecretBytes.length !== SIGNING_SECRET_RANDOM_BYTES) {
    throw new ApiKeyGenerationError(
      'Cryptographic random source returned an unexpected length for signing secret',
      'ENTROPY_LENGTH',
    );
  }

  const signingSecret = signingSecretBytes.toString('hex');
  const signingSecretHash = signingSecret; // We must store the actual secret to verify HMAC

  return { key, prefix, hash, signingSecret, signingSecretHash };
}

/**
 * Hash an API key using SHA-256
 */
export function hashApiKey(key: string): string {
  return crypto.createHash('sha256').update(key).digest('hex');
}

/**
 * Timing-safe comparison to prevent timing attacks.
 *
 * Invariants:
 * - Returns false for any input that is not a valid hex string of the same
 *   length. This is deterministic and does not leak length information via
 *   thrown exceptions.
 * - Never throws for malformed input; returns false instead.
 */
export function timingSafeCompare(a: string, b: string): boolean {
  if (typeof a !== 'string' || typeof b !== 'string') {
    return false;
  }

  if (a.length !== b.length) {
    return false;
  }

  // Only even-length hex strings can be decoded to buffers.
  if (a.length % 2 !== 0) {
    return false;
  }

  if (!/^[0-9a-fA-F]*$/.test(a) || !/^[0-9a-fA-F]*$/.test(b)) {
    return false;
  }

  const bufferA = Buffer.from(a, 'hex');
  const bufferB = Buffer.from(b, 'hex');

  if (bufferA.length !== bufferB.length) {
    return false;
  }

  return crypto.timingSafeEqual(bufferA, bufferB);
}
