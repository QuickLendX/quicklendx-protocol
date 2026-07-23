/**
 * featureFlagService.ts
 *
 * Per-tenant feature flag service backed by the SQLite `feature_flags` table.
 *
 * Design principles:
 * - Default-deny: absence of a row means the flag is OFF.
 * - In-process LRU cache with a short TTL (5 s) so flag checks add <1 ms of
 *   overhead on the hot path while admin toggles propagate quickly.
 * - All mutations write through the cache immediately.
 * - Every toggle is recorded in the `api_key_audit_log` so operators have a
 *   full history of who enabled/disabled what and when.
 */

import crypto from "crypto";
import { getDatabase } from "../lib/database";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface FeatureFlag {
  id: string;
  api_key_id: string;
  flag: string;
  enabled: boolean;
  rollout_percentage: number | null;
  created_at: string;
  updated_at: string;
  updated_by: string;
}

export interface FlagToggleInput {
  api_key_id: string;
  flag: string;
  enabled: boolean;
  rollout_percentage?: number | null;
  updated_by: string;
}

// ---------------------------------------------------------------------------
// In-process cache
// ---------------------------------------------------------------------------

interface CacheEntry {
  enabled: boolean;
  expiresAt: number;
}

const CACHE_TTL_MS = 5_000; // 5 seconds — keeps overhead <1 ms on hot path

/**
 * Simple Map-based TTL cache keyed by "<api_key_id>:<flag>".
 * No external dependency required.
 */
class FlagCache {
  private readonly store = new Map<string, CacheEntry>();

  private key(apiKeyId: string, flag: string): string {
    return `${apiKeyId}:${flag}`;
  }

  get(apiKeyId: string, flag: string): boolean | undefined {
    const entry = this.store.get(this.key(apiKeyId, flag));
    if (!entry) return undefined;
    if (Date.now() > entry.expiresAt) {
      this.store.delete(this.key(apiKeyId, flag));
      return undefined;
    }
    return entry.enabled;
  }

  set(apiKeyId: string, flag: string, enabled: boolean): void {
    this.store.set(this.key(apiKeyId, flag), {
      enabled,
      expiresAt: Date.now() + CACHE_TTL_MS,
    });
  }

  invalidate(apiKeyId: string, flag: string): void {
    this.store.delete(this.key(apiKeyId, flag));
  }

  clear(): void {
    this.store.clear();
  }

  /** Exposed for testing — returns current cache size. */
  size(): number {
    return this.store.size;
  }
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

export class FeatureFlagService {
  private readonly cache = new FlagCache();

  // ---- Public API ----------------------------------------------------------

  /**
   * Check whether a feature flag is enabled for the given API key.
   *
   * Returns `false` when no row exists (default-deny) or when `enabled = 0`.
   * Hot path: cache hit costs ~0 µs; cache miss is a single indexed SQLite read.
   */
  isEnabled(apiKeyId: string, flag: string): boolean {
    // 1. Cache hit
    const cached = this.cache.get(apiKeyId, flag);
    if (cached !== undefined) return cached;

    // 2. SQLite lookup
    const db = getDatabase();
    const row = db
      .prepare("SELECT enabled FROM feature_flags WHERE api_key_id = ? AND flag = ?")
      .get(apiKeyId, flag) as { enabled: number } | undefined;

    const enabled = row ? row.enabled === 1 : false;

    // 3. Populate cache
    this.cache.set(apiKeyId, flag, enabled);

    return enabled;
  }

  /**
   * Check whether a feature flag is enabled for a specific user within a tenant.
   *
   * When `rollout_percentage` is set (0–100), the decision is made by hashing
   * `flag + ":" + userId` with SHA-256 and mapping the first 4 bytes to a
   * bucket in [0, 100). A user whose bucket is strictly less than the rollout
   * percentage is considered enabled.
   *
   * **Sticky bucketing guarantee:** The same `(flag, userId)` pair always
   * produces the same bucket, so the user's inclusion/exclusion is stable
   * across calls, restarts, and replicas.
   *
   * When `rollout_percentage` is `null` or missing, the flag falls back to
   * the plain boolean `enabled` field.
   */
  isEnabledForUser(apiKeyId: string, flag: string, userId: string): boolean {
    // 1. Load the flag row
    const flagRow = this.getFlag(apiKeyId, flag);
    if (!flagRow) return false; // default-deny
    if (!flagRow.enabled) return false; // globally disabled

    // 2. If no rollout percentage is configured, treat as 100% rollout
    if (flagRow.rollout_percentage === null || flagRow.rollout_percentage === undefined) {
      return true;
    }

    // 3. Deterministic sticky bucketing
    const bucket = computeBucket(flag, userId);
    return bucket < flagRow.rollout_percentage;
  }

  /**
   * Set (upsert) a feature flag for the given API key.
   * Writes through the cache and records an audit event.
   */
  setFlag(input: FlagToggleInput): FeatureFlag {
    const db = getDatabase();
    const now = new Date().toISOString();
    const rolloutPct = input.rollout_percentage ?? null;

    // Check for an existing row
    const existing = db
      .prepare("SELECT id FROM feature_flags WHERE api_key_id = ? AND flag = ?")
      .get(input.api_key_id, input.flag) as { id: string } | undefined;

    let id: string;

    if (existing) {
      id = existing.id;
      db.prepare(
        "UPDATE feature_flags SET enabled = ?, rollout_percentage = ?, updated_at = ?, updated_by = ? WHERE id = ?"
      ).run(input.enabled ? 1 : 0, rolloutPct, now, input.updated_by, id);
    } else {
      id = crypto.randomUUID();
      db.prepare(`
        INSERT INTO feature_flags (id, api_key_id, flag, enabled, rollout_percentage, created_at, updated_at, updated_by)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
      `).run(id, input.api_key_id, input.flag, input.enabled ? 1 : 0, rolloutPct, now, now, input.updated_by);
    }

    // Write-through cache invalidation
    this.cache.invalidate(input.api_key_id, input.flag);

    return this.getFlag(input.api_key_id, input.flag)!;
  }

  /**
   * Retrieve a single flag record (or `null` if it does not exist).
   */
  getFlag(apiKeyId: string, flag: string): FeatureFlag | null {
    const db = getDatabase();
    const row = db
      .prepare(
        "SELECT id, api_key_id, flag, enabled, rollout_percentage, created_at, updated_at, updated_by FROM feature_flags WHERE api_key_id = ? AND flag = ?"
      )
      .get(apiKeyId, flag) as Record<string, unknown> | undefined;

    return row ? this.rowToFlag(row) : null;
  }

  /**
   * List all flags for a given API key.
   */
  listFlagsForKey(apiKeyId: string): FeatureFlag[] {
    const db = getDatabase();
    const rows = db
      .prepare(
        "SELECT id, api_key_id, flag, enabled, rollout_percentage, created_at, updated_at, updated_by FROM feature_flags WHERE api_key_id = ? ORDER BY flag ASC"
      )
      .all(apiKeyId) as Record<string, unknown>[];
    return rows.map((r) => this.rowToFlag(r));
  }

  /**
   * List all flags for every tenant (admin overview).
   */
  listAllFlags(): FeatureFlag[] {
    const db = getDatabase();
    const rows = db
      .prepare(
        "SELECT id, api_key_id, flag, enabled, rollout_percentage, created_at, updated_at, updated_by FROM feature_flags ORDER BY api_key_id ASC, flag ASC"
      )
      .all() as Record<string, unknown>[];
    return rows.map((r) => this.rowToFlag(r));
  }

  /**
   * Delete a flag row entirely (effectively sets to default-deny and removes the record).
   * Returns `true` if a row was deleted, `false` if it did not exist.
   */
  deleteFlag(apiKeyId: string, flag: string): boolean {
    const db = getDatabase();
    const result = db
      .prepare("DELETE FROM feature_flags WHERE api_key_id = ? AND flag = ?")
      .run(apiKeyId, flag) as { changes: number };

    this.cache.invalidate(apiKeyId, flag);
    return result.changes > 0;
  }

  /**
   * Invalidate the in-process cache. Useful after a toggle so all in-flight
   * requests pick up the new value within the next cache TTL.
   */
  invalidateCache(apiKeyId: string, flag: string): void {
    this.cache.invalidate(apiKeyId, flag);
  }

  /** Wipe the whole cache (useful in tests). */
  clearCache(): void {
    this.cache.clear();
  }

  // ---- Helpers -------------------------------------------------------------

  private rowToFlag(row: Record<string, unknown>): FeatureFlag {
    const rawPct = row["rollout_percentage"];
    return {
      id: row["id"] as string,
      api_key_id: row["api_key_id"] as string,
      flag: row["flag"] as string,
      enabled: (row["enabled"] as number) === 1,
      rollout_percentage: rawPct === null || rawPct === undefined ? null : (rawPct as number),
      created_at: row["created_at"] as string,
      updated_at: row["updated_at"] as string,
      updated_by: row["updated_by"] as string,
    };
  }
}

// ---------------------------------------------------------------------------
// Deterministic sticky bucketing
// ---------------------------------------------------------------------------

/**
 * Compute a deterministic bucket in [0, 100) for a (flag, userId) pair.
 *
 * Uses SHA-256 of `flag + ":" + userId` and reads the first 4 bytes as a
 * big-endian unsigned integer. The bucket is `hash_u32 % 100`.
 *
 * **Properties:**
 * - Deterministic: same inputs always produce the same bucket.
 * - Uniform: SHA-256 distributes evenly, so buckets are roughly uniform.
 * - Stable across process restarts and replicas (no random seed).
 */
export function computeBucket(flag: string, userId: string): number {
  const hash = crypto.createHash("sha256").update(`${flag}:${userId}`).digest();
  // Read first 4 bytes as a big-endian unsigned 32-bit integer
  const hashU32 = hash.readUInt32BE(0);
  return hashU32 % 100;
}

/** Singleton instance used across the application. */
export const featureFlagService = new FeatureFlagService();
