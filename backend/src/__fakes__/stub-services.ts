/**
 * Programmable test doubles for `ApiKeyService` and `AuthorizationService`.
 *
 * Each method is a mutable field so individual tests can install arbitrary
 * behaviour (success values, thrown errors, call counters). Shared helpers
 * build realistic `ApiKey`/`RevokedApiKey` fixtures.
 */
import { ApiKeyError } from "../errors/api-key-error";
import type { ApiKey, ApiKeyService, RevokedApiKey } from "../models/api-key";
import type { AuthorizationService } from "../services/authorization";

export const KEY_ID = "018f2b5e-3a1c-4d2e-9b7f-6a5c4d3e2f10";
export const OWNER_ID = "owner_abcdef";
export const ACTOR_ID = "actor_123456";
export const OTHER_ACTOR_ID = "actor_999999";
export const FIXED_TIME = 1_700_000_000_000;

export const buildApiKey = (overrides: Partial<ApiKey> = {}): ApiKey => ({
  id: KEY_ID,
  name: "Production Key",
  ownerId: OWNER_ID,
  permissions: ["read", "write"],
  status: "active",
  version: 1,
  createdAt: FIXED_TIME,
  revokedAt: null,
  revokedBy: null,
  ...overrides,
});

export const buildRevokedApiKey = (
  overrides: Partial<RevokedApiKey> = {}
): RevokedApiKey =>
  ({
    ...buildApiKey({
      status: "revoked",
      revokedAt: FIXED_TIME + 1000,
      revokedBy: OWNER_ID,
    }),
    ...overrides,
  }) as RevokedApiKey;

export interface CallRecord {
  args: unknown[];
}

/** A fully programmable fake of `ApiKeyService`. */
export class StubApiKeyService implements ApiKeyService {
  calls: Record<string, CallRecord[]> = {
    getApiKey: [],
    revokeApiKey: [],
  };

  getApiKeyImpl: (id: string) => Promise<ApiKey | null> = async () => null;
  revokeApiKeyImpl: (
    id: string,
    revokedBy: string,
    expectedVersion?: number
  ) => Promise<RevokedApiKey> = async () => {
    throw new ApiKeyError("NOT_FOUND", "stub not configured", {});
  };

  async getApiKey(id: string): Promise<ApiKey | null> {
    this.calls.getApiKey.push({ args: [id] });
    return this.getApiKeyImpl(id);
  }

  async revokeApiKey(
    id: string,
    revokedBy: string,
    expectedVersion?: number
  ): Promise<RevokedApiKey> {
    this.calls.revokeApiKey.push({ args: [id, revokedBy, expectedVersion] });
    return this.revokeApiKeyImpl(id, revokedBy, expectedVersion);
  }

  /** Convenience: configure the next `get` to return a fresh active key. */
  nextGetActive(key: Partial<ApiKey> = {}): void {
    this.getApiKeyImpl = async (id: string) =>
      id === KEY_ID ? buildApiKey({ id, ...key }) : null;
  }
}

/** A fully programmable fake of `AuthorizationService`. */
export class StubAuthorizationService implements AuthorizationService {
  calls: CallRecord[] = [];
  canActImpl: (
    actorId: string,
    action: string,
    keyId: string
  ) => Promise<boolean> = async () => false;

  async canAct(
    actorId: string,
    action: string,
    keyId: string
  ): Promise<boolean> {
    this.calls.push({ args: [actorId, action, keyId] });
    return this.canActImpl(actorId, action, keyId);
  }
}

export const transientError = (message = "upstream timeout"): ApiKeyError =>
  new ApiKeyError("TRANSIENT", message, { action: "upstream" });

export const systemError = (message = "unexpected failure"): ApiKeyError =>
  new ApiKeyError("SYSTEM_ERROR", message, { action: "upstream" });
