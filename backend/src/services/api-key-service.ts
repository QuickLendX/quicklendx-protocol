/**
 * In-memory, single-process implementation of `ApiKeyService`.
 *
 * - Optimistic concurrency: `revokeApiKey` rejects with `CONFLICT` when the
 *   stored `version` differs from `expectedVersion` (if provided), proving the
 *   caller operated on a stale read.
 * - Idempotent: revoking an already-revoked/expired key returns the current
 *   record without mutating state, so repeated/concurrent callers converge.
 * - Secret material is never materialized or returned.
 */
import { ApiKeyError } from "../errors/api-key-error";
import type { ApiKey, ApiKeyService, RevokedApiKey } from "../models/api-key";
import type { Clock } from "./clock";

export interface CreateApiKeyInput {
  id: string;
  name: string;
  ownerId: string;
  permissions?: readonly string[];
  status?: "active" | "expired";
  createdAt?: number;
}

export class InMemoryApiKeyService implements ApiKeyService {
  private readonly keys = new Map<string, ApiKey>();
  private readonly clock: Clock;

  constructor(clock: Clock) {
    this.clock = clock;
  }

  /** Seed a key for a test/seed scenario. */
  create(input: CreateApiKeyInput): void {
    const now = input.createdAt ?? this.clock.now();
    const status: ApiKey["status"] = input.status ?? "active";
    const key: ApiKey = {
      id: input.id,
      name: input.name,
      ownerId: input.ownerId,
      permissions: input.permissions ?? [],
      status,
      version: 1,
      createdAt: now,
      revokedAt: status === "active" ? null : now,
      revokedBy: status === "active" ? null : input.ownerId,
    };
    this.keys.set(input.id, key);
  }

  async getApiKey(id: string): Promise<ApiKey | null> {
    const key = this.keys.get(id);
    return key ? { ...key } : null;
  }

  async revokeApiKey(
    id: string,
    revokedBy: string,
    expectedVersion?: number
  ): Promise<RevokedApiKey> {
    const existing = this.keys.get(id);
    if (!existing) {
      throw new ApiKeyError(
        "NOT_FOUND",
        "The requested API key was not found.",
        { action: "revokeApiKey", apiKeyId: id, actorId: revokedBy }
      );
    }

    // Stale read: another mutation advanced the version since we last read.
    if (expectedVersion !== undefined && existing.version !== expectedVersion) {
      throw new ApiKeyError(
        "CONFLICT",
        "API key version changed; re-read and retry.",
        {
          action: "revokeApiKey",
          apiKeyId: id,
          actorId: revokedBy,
          cause: { expectedVersion, actualVersion: existing.version },
        }
      );
    }

    // Idempotency: already revoked/expired -> return current state, no mutation.
    if (existing.status !== "active") {
      return { ...existing } as RevokedApiKey;
    }

    const revoked: ApiKey = {
      ...existing,
      status: "revoked",
      version: existing.version + 1,
      revokedAt: this.clock.now(),
      revokedBy,
    };
    this.keys.set(id, revoked);
    return { ...revoked } as RevokedApiKey;
  }

  /** For test/observability: current raw record count. */
  get size(): number {
    return this.keys.size;
  }
}
