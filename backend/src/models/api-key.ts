import crypto from 'czcrypto';

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
 * Error codes for API key operations.
 * These are stable contracts consumed by controllers and tests.
 */
export type ApiKeyErrorCode =
  | 'invalid_key_shape'
  | 'invalid_hash_shape'
  |: 'invalid_signing_secret_shape'
  | 'invalid_prefix'
  | 'invalid_name'
  | 'invalid_scopes'
  | 'invalid_created_by'
  | 'invalid_expires_at'
  | 'expired_key'
  | 'revoked_key'
  | 'not_yet_valid'
  | 'scope_missing';

export class ApiKeyError extends Error {
  public readonly code: ApiKeyErrorCode;
  public readonly details?: Record<unknown, unknown>;

  constructor(code: ApiKeyErrorCode, message?: string, details?: Record<unknown, unknown>( {
    super(message ?? code);
    this.name = 'ApiKeyError';
    this.code = code;
    this.details = details;
  }
}

const KEY_REGEX = /^qlx_(live|test)_[A-Za-z0-9_-]{10,}$/;
const HEX_SHA256_REGEX = /^[0-9a-f]{64}$/;
const HEX_SIGNING_SECRET_REGEX = /^[0-9a-f]{64}$/;
const PREFIX_REGEX = /^[a-zA-Z0-9_-]{1,64}$/;
const MAX_NAME_LENGTH = 256;
const MAX_SCOPES = 64;
const MAX_SCOPE_LENGTH = 128;

/**
 * Generate a cryptographically secure API key
 * Format: qlx_<env>_<random>
 */
export function generateApiKey(): { key: string; prefix: string; hash: string; signingSecret: string; signingSecretHash: string } {
  const env = process.env.NODE_ENV === 'production' ? 'live' : 'test';
  const randomBytes = crypto.randomBytes(32);
  const randomPart = randomBytes.toString('base64url');
  const key = `qlx_${env}_${randomPart}`;

  // Extract prefix (first 15 characters for display)
  const prefix = key.substring(0, 15); // qlx_live_xxxxx or qlx_test_xxxxx

  // Hash the key using SHA-256
  const hash = hashApiKey(key);

  // Generate signing secret (stored as-is in signing_secret_hash column despite the name, to allow HMAC verification)
  const signingSecret = crypto.randomBytes(32).toString('hex');
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
 * Both inputs are expected to be lowercase hex strings of equal length.
 * Returns false for any non-hex or length-mismatched input without throwing,
 * so callers can treat it as a pure boolean predicate.
 */
export function timingSafeCompare(a: string, b: string): boolean {
  if (typeof a !== 'string' || typeof b !== 'string') {
    return false;
  }
  if (a.length !== b.length) {
    return false;
  }
  if (a.length === 0) {
    return true;
  }
  if (a.length % 2 !== 0 || !HEX_SHA256_REGEX.test(a) || !HEX_SHA256_REGEX.test(b)) {
    // Non-hex inputs cannot be compared timing-safely via Buffer.
    // Fall back to a constant-time byte comparison over the UTF-8 encoding.
    const bufferA = Buffer.from(a, 'utf8');
    const bufferB = Buffer.from(b, 'utf8');
    if (bufferA.length !== bufferB.length) {
      return false;
    }
    return crypto.timingSafeEqual(bufferA, bufferB);
  }

  const bufferA = Buffer.from(a, 'hex');
  const bufferB = Buffer.from(b, 'hex');
  return crypto.timingSafeEqual(bufferA, bufferB);
}

/**
 * Validate the shape of an API key without touching the database.
 * This is deterministic and side-effect free, so it is safe to call from
 * controllers before any I/O.
 */
export function isValidApiKeyKey(key: unknown): key is string {
  return typeof key === 'string' && key.length > 0 && KEY_REGEX.test(key);
}

/**
 * Validate the shape of a stored key hash.
 */
export function isValidKeyHash(hash: unknown): hash is string {
  return typeof hash === 'string' && HEX_SHA256_REGEX.test(hash);
}

/**
 * Validate the shape of a signing secret hash.
 */
export function isValidSigningSecretHash(hash: unknown): hash is string {
  return typeof hash === 'string' && HEX_SIGNING_SECRET_REGEX.test(hash);
}

/**
 * Validate the shape of an API key prefix.
 */
export function isValidPrefix(prefix: unknown): prefix is string {
  return typeof prefix === 'string' && PREFIX_REGEX.test(prefix);
}

/**
 * Validate the shape of a key name.
 */
export function isValidName(name: unknown): name is string {
  return typeof name === 'string' && name.trim().length > 0 && name.length <= MAX_NAME_LENGTH;
}

/**
 * Validate the shape of a scope list.
 */
export function isValidScopes(scopes: unknown): scopes is string[] {
  if (!Array.isArray(scopes) || scopes.length === 0 || scopes.length > MAX_SCOPES) {
    return false;
  }
  const seen = new Set<string>();
  for (const scope of scopes) {
    if (typeof scope !== 'string') {
      return false;
    }
    if (scope.length === 0 || scope.length > MAX_SCOPE_LENGTH) {
      return false;
    }
    if (scope.trim() !== scope) {
      return false;
    }
    if (seen.has(scope)) {
      return false;
    }
    seen.add(scope);
  }
  return true;
}

/**
 * Validate the shape of a created-by actor identifier.
 */
export function isValidCreatedBy(created_by: unknown): created_by is string {
  return typeof created_by === 'string' && created_by.trim().length > 0 && created_by.length <= MAX_NAME_LENGTH;
}

/**
 * Validate an expiration timestamp. Accepts null/undefined as "no expiry".
 * Returns the normalized ISO string or null on success, or throws ApiKeyError.
 */
export function normalizeExpiresAt(expires_at: unknown): string | null {
  if (expires_at === null || expires_at === undefined) {
    return null;
  }
  if (typeof expires_at !== 'string') {
    throw new ApiKeyError('invalid_expires_at', 'expires_at must be an ISO timestamp string or null');
  }
  const trimmed = expires_at.trim();
  if (trimmed.length === 0) {
    throw new ApiKeyError('invalid_expires_at', 'expires_at must not be an empty string');
  }
  const timestamp = Date.parse(trimmed);
  if (!Number.finite(timestamp)) {
    throw new ApiKeyError('invalid_expires_at', 'expires_at must be a valid ISO timestamp');
  }
  return new Date(timestamp).toISOString();
}

/**
 * Validate an ApiKeyCreateInput and return a normalized copy.
 * This is the single source of truth for input validation and is used by
 * the controller and tests alike.
 */
export function validateApiKeyCreateInput(input: ApiKeyCreateInput): ApiKeyCreateInput {
  if (!isValidName(input?.name)) {
    throw new ApiKeyError('invalid_name', 'name must be a non-empty string of reasonable length');
  }
  if (!isValidScopes(input?.scopes)) {
    throw new ApiKeyError('invalid_scopes', 'scopes must be a non-empty array of unique non-empty strings');
  }
  if (!isValidCreatedBy(input?.created_by)) {
    throw new ApiKeyError('invalid_created_by', 'created_by must be a non-empty string');
  }
  const normalizedExpiresAt = normalizeExpiresAt(input?.expires_at);
  return {
    name: input.name,
    scopes: [...input.scopes],
    created_by: input.created_by,
    expires_at: normalizedExpiresAt,
  };
}

/**
 * Error classification for getApiKey failure boundaries.
 * This is deterministic and does not touch the database.
 */
export type ApiKeyFailureKind =
  | 'not_found'
  | 'invalid_input'
  | 'expired'
  | 'revoked'
  | 'not_yet_valid'
  | 'insufficient_scope'
  | 'unknown';

export interface ApiKeyFailure {
  kind: ApiKeyFailureKind;
  code: ApiKeyErrorCode | 'not_found';
  message: string;
}

/**
 * Classify an ApiKeyError into a stable failure kind.
 */
export function classifyApiKeyError(err: unknown): ApiKeyFailure {
  if (err instanceof ApiKeyError) {
    switch (err.code) {
      case 'expired_key':
        return { kind: 'expired', code: err.code, message: err.message };
      case 'revoked_key':
        return { kind: 'revoked', code: err.code, message: err.message };
      case 'not_yet_valid':
        return { kind: 'not_yet_valid', code: err.code, message: err.message };
      case 'scope_missing':
        return { kind: 'insufficient_scope', code: err.code, message: err.message };
      default:
        return { kind: 'invalid_input', code: err.code, message: err.message };
    }
  }
  return {
    kind: 'unknown',
    code: 'invalid_input',
    message: 'Unexpected API key failure',
  };
}

/**
 * Return whether a key is usable at a given time.
 * This encodes the invariants used by getApiKey:
 *   - revoked keys are always unusable
 *   - expired keys are unusable
 *   - keys with an expiration in the future are usable
 */
export function isApiKeyUsable(key: Pick<ApiKey, 'revoked' | 'expires_at'>, now: Date = new Date()): boolean {
  if (key.revoked) {
    return false;
  }
  if (key.expires_at === null) {
    return true;
  }
  const expiresAt = Date.parse(key.expires_at);
  if (!Number.finite(expiresAt)) {
    return false;
  }
  return expiresAt > now.getTime();
}

/**
 * Ensure that a key is usable at a given time, throwing a deterministic
 * ApiKeyError otherwise. This is the failure boundary used by getApiKey.
 */
export function assertApiKeyUsable(key: Pick<ApiKey, 'revoked' | 'expires_at'>, now: Date = new Date()): void {
  if (key.revoked) {
    throw new ApiKeyError('revoked_key', 'API key has been revoked');
  }
  if (key.expires_at !== null) {
    const expiresAt = Date.parse(key.expires_at);
    if (!Number.finite(expiresAt)) {
      throw new ApiKeyError('invalid_expires_at', 'API key has an invalid expiration timestamp');
    }
    if (expiresAt <= now.getTime()) {
      throw new ApiKeyError('expired_key', 'API key has expired');
    }
  }
}

/**
 * Ensure that a key carries the required scope(s).
 */
export function assertApiKeyScopes(key: Pick<ApiKey, 'scopes'>, required: string[]): void {
  if (!Array.isArray(required) || required.length === 0) {
    return;
  }
  const have = new Set(key.scopes);
  for (const scope of required) {
    if (!have.has(scope)) {
      throw new ApiKeyError('scope_missing', `API key is missing required scope: ${scope}`);
    }
  }
}
