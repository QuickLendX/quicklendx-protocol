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
