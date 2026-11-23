/**
 * API key domain models for the QuickLendX backend.
 *
 * Invariants:
 * - `id` is a non-secret, opaque identifier (UUIDv4). It is safe to log.
 * - Plaintext is returned only by creation/rotation, never persisted or logged.
 *   Signing hashes are internal HMAC material and must not reach HTTP reads.
 * - `version` is an optimistic-concurrency token (monotonically increasing).
 *   Credential mutations bump it; usage telemetry does not invalidate readers.
 */
import crypto from "node:crypto";

export type ApiKeyStatus = "active" | "revoked" | "expired";

export interface ApiKey {
  readonly prefix: string;
  readonly scopes: string[];
  readonly created_by: string;
  readonly created_at: string;
  readonly last_used_at: string | null;
  readonly expires_at: string | null;
  readonly revoked: boolean;
  readonly signing_secret_hash: string | null;
  readonly prev_signing_secret_hash: string | null;
  readonly prev_secret_expires_at: string | null;
  readonly id: string;
  readonly name: string;
  readonly ownerId: string;
  readonly permissions: readonly string[];
  readonly status: ApiKeyStatus;
  /** Optimistic-concurrency version. Incremented on credential mutations. */
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
  createApiKey(input: ApiKeyCreateInput, ip?: string): Promise<ApiKeyWithPlaintext>;
  verifyApiKey(plaintext: string): Promise<ApiKey | null>;
  getApiKeyById(id: string): Promise<ApiKey | null>;
  listApiKeys(filters?: { created_by?: string; revoked?: boolean }): Promise<ApiKey[]>;
  updateLastUsed(id: string, endpoint: string, ip?: string): Promise<void>;
  rotateApiKey(id: string, actor: string, ip?: string): Promise<ApiKeyWithPlaintext>;
  rotateSigningSecret(id: string, actor: string, ip?: string, graceHours?: number): Promise<ApiKey & { plaintext_signing_secret: string }>;
  /** Returns the key by id, or `null` when absent. Never returns secret material. */
  getApiKey(id: string): Promise<ApiKey | null>;
  /**
   * Atomically revokes the key owned by `revokedBy`.
   * @throws {ApiKeyError} code `"CONFLICT"` when `expectedVersion` is supplied
   *   and no longer matches the stored version (stale read).
   * @throws {ApiKeyError} code `"NOT_FOUND"` when the key does not exist.
   * Idempotent: revoking an already-revoked key returns the current
   * record without error.
   */
  revokeApiKey(
    id: string,
    revokedBy: string,
    expectedVersion?: number
  ): Promise<RevokedApiKey>;
}

export interface ApiKeyCreateInput {
  name: string;
  scopes: string[];
  created_by: string;
  expires_at?: string | null;
}

export interface ApiKeyWithPlaintext extends ApiKey {
  plaintext_key: string;
  plaintext_signing_secret: string;
}

export function generateApiKey(): { key: string; prefix: string; hash: string } {
  const key = `qlx_${crypto.randomBytes(32).toString("base64url")}`;
  return { key, prefix: key.slice(0, 15), hash: hashApiKey(key) };
}

export function hashApiKey(key: string): string {
  return crypto.createHash("sha256").update(key).digest("hex");
}

export function timingSafeCompare(left: string, right: string): boolean {
  if (!/^[a-f0-9]{64}$/.test(left) || !/^[a-f0-9]{64}$/.test(right)) return false;
  return crypto.timingSafeEqual(Buffer.from(left, "hex"), Buffer.from(right, "hex"));
}
