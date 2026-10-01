/**
 * API key domain models for the QuickLendX backend.
 *
 * Invariants:
 * - `id` is a non-secret, opaque identifier (UUIDv4). It is safe to log.
 * - `secret` (the credential material) is NEVER persisted or returned by the
 *   service layer. The controller never logs or includes it in errors.
 * - `version` is an optimistic-concurrency token (monotonically increasing).
 *   Every successful mutation bumps it so stale clients can be detected.
 */
export type ApiKeyStatus = "active" | "revoked" | "expired";

export interface ApiKey {
  readonly id: string;
  readonly name: string;
  readonly ownerId: string;
  readonly permissions: readonly string[];
  readonly status: ApiKeyStatus;
  /** Optimistic-concurrency version. Incremented on each mutation. */
  readonly version: number;
  readonly createdAt: number;
  readonly revokedAt: number | null;
  readonly revokedBy: string | null;
}

/** A key that has been revoked. `revokedAt`/`revokedBy` are always present. */
export interface RevokedApiKey extends ApiKey {
  readonly status: "revoked";
  readonly revokedAt: number;
  readonly revokedBy: string;
}

/**
 * Service contract for API key persistence.
 *
 * `revokeApiKey` is atomic with respect to the provided `expectedVersion`:
 * if the stored key's version has advanced since the caller read it, the call
 * MUST be rejected with an `ApiKeyError` (code `"CONFLICT"`) so the caller can
 * re-read and recover (see `ApiKeysController.revokeApiKey`).
 */
export interface ApiKeyService {
  /** Returns the key by id, or `null` when absent. Never returns secret material. */
  getApiKey(id: string): Promise<ApiKey | null>;
  /**
   * Atomically revokes the key owned by `revokedBy`.
   * @throws {ApiKeyError} code `"CONFLICT"` when `expectedVersion` is supplied
   *   and no longer matches the stored version (stale read).
   * @throws {ApiKeyError} code `"NOT_FOUND"` when the key does not exist.
   * Idempotent: revoking an already-revoked/expired key returns the current
   * record without error.
   */
  revokeApiKey(
    id: string,
    revokedBy: string,
    expectedVersion?: number
  ): Promise<RevokedApiKey>;
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
