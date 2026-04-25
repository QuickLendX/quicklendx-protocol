/**
 * Read-Through Cache with Event-Driven Invalidation (#877)
 *
 * Provides:
 * - A typed in-memory cache (swap CacheStore implementation for Redis in prod)
 * - Configurable TTL per cache key with absolute freshness windows
 * - Event-driven invalidation: indexed events evict the affected cache entries
 * - Stale-while-revalidate flag with an explicit `is_stale` indicator on
 *   responses – financial data is NEVER served silently stale
 * - Cache-aside (read-through) helper that populates on miss
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface CacheEntry<T> {
  value: T;
  /** Unix ms when this entry was cached */
  cached_at: number;
  /** Unix ms after which this entry is considered stale */
  expires_at: number;
}

export interface CacheGetResult<T> {
  hit: boolean;
  value: T | null;
  /** Present only on a hit. True if the entry is past its freshness window. */
  is_stale?: boolean;
  /** Unix ms when the value was cached */
  cached_at?: number;
}

export interface CacheConfig {
  /**
   * Default TTL in milliseconds.
   * Default: 30 seconds (conservative for financial data).
   */
  default_ttl_ms: number;
  /**
   * If true, stale entries are returned with is_stale=true while a background
   * revalidation is triggered. If false, stale = miss.
   *
   * IMPORTANT: Callers must check `is_stale` and display a freshness warning
   * when serving financial data.
   */
  serve_stale: boolean;
}

export const DEFAULT_CACHE_CONFIG: CacheConfig = {
  default_ttl_ms: 30_000, // 30 s
  serve_stale: false,     // safe default for financial data
};

// ---------------------------------------------------------------------------
// CacheStore interface (in-memory impl; swap for Redis in prod)
// ---------------------------------------------------------------------------

export interface CacheStore {
  get<T>(key: string): Promise<CacheEntry<T> | null>;
  set<T>(key: string, entry: CacheEntry<T>): Promise<void>;
  delete(key: string): Promise<void>;
  deleteByPattern(prefix: string): Promise<number>;
  clear(): Promise<void>;
}

export class InMemoryCacheStore implements CacheStore {
  private store = new Map<string, CacheEntry<unknown>>();

  async get<T>(key: string): Promise<CacheEntry<T> | null> {
    return (this.store.get(key) as CacheEntry<T>) ?? null;
  }

  async set<T>(key: string, entry: CacheEntry<T>): Promise<void> {
    this.store.set(key, entry as CacheEntry<unknown>);
  }

  async delete(key: string): Promise<void> {
    this.store.delete(key);
  }

  async deleteByPattern(prefix: string): Promise<number> {
    let count = 0;
    for (const key of this.store.keys()) {
      if (key.startsWith(prefix)) {
        this.store.delete(key);
        count++;
      }
    }
    return count;
  }

  async clear(): Promise<void> {
    this.store.clear();
  }

  /** Expose store size for testing */
  get size(): number {
    return this.store.size;
  }
}

// ---------------------------------------------------------------------------
// Cache key conventions
// ---------------------------------------------------------------------------

/** Canonical cache key for the best bid on an invoice */
export function bestBidKey(invoiceId: string): string {
  return `best_bid:${invoiceId}`;
}

/** Canonical cache key for invoice detail */
export function invoiceDetailKey(invoiceId: string): string {
  return `invoice:${invoiceId}`;
}

/** Prefix for all invoice-related cache keys (used for bulk invalidation) */
export function invoiceKeyPrefix(invoiceId: string): string {
  return `invoice:${invoiceId}`;
}

/** Prefix for all bid-related cache keys */
export function bidKeyPrefix(invoiceId: string): string {
  return `best_bid:${invoiceId}`;
}

// ---------------------------------------------------------------------------
// ReadThroughCache
// ---------------------------------------------------------------------------

/**
 * Generic read-through cache with event-driven invalidation.
 *
 * @example
 * ```ts
 * const cache = new ReadThroughCache(new InMemoryCacheStore());
 *
 * // Read-through: fetches and caches on miss
 * const result = await cache.get(invoiceDetailKey(id), () => fetchInvoice(id));
 * if (result.is_stale) showFreshnessWarning();
 *
 * // Invalidate on event
 * await cache.invalidateInvoice(id); // called by the indexer on new events
 * ```
 */
export class ReadThroughCache {
  private readonly config: CacheConfig;

  constructor(
    private readonly store: CacheStore,
    config: Partial<CacheConfig> = {}
  ) {
    this.config = { ...DEFAULT_CACHE_CONFIG, ...config };
  }

  /**
   * Read-through get.
   *
   * 1. Check cache.
   * 2. If missing (or stale and serve_stale=false) → call `fetcher` and cache.
   * 3. If stale and serve_stale=true → return stale with is_stale=true and
   *    revalidate in the background.
   */
  async get<T>(
    key: string,
    fetcher: () => Promise<T>,
    ttlMs: number = this.config.default_ttl_ms
  ): Promise<CacheGetResult<T>> {
    const entry = await this.store.get<T>(key);
    const now = Date.now();

    if (entry !== null) {
      const isStale = now > entry.expires_at;

      if (!isStale) {
        return {
          hit: true,
          value: entry.value,
          is_stale: false,
          cached_at: entry.cached_at,
        };
      }

      // Entry is stale
      if (this.config.serve_stale) {
        // Return stale data immediately; revalidate in background
        this._revalidate(key, fetcher, ttlMs).catch((err) =>
          console.error(`[cache] Background revalidation failed for "${key}":`, err)
        );
        return {
          hit: true,
          value: entry.value,
          is_stale: true,
          cached_at: entry.cached_at,
        };
      }
    }

    // Cache miss (or stale + no serve_stale) → fetch and populate
    const value = await fetcher();
    await this._set(key, value, ttlMs, now);
    return { hit: false, value, is_stale: false, cached_at: now };
  }

  /** Explicitly set a value in the cache */
  async set<T>(key: string, value: T, ttlMs?: number): Promise<void> {
    await this._set(key, value, ttlMs ?? this.config.default_ttl_ms, Date.now());
  }

  /** Explicitly invalidate a single key */
  async invalidate(key: string): Promise<void> {
    await this.store.delete(key);
  }

  /**
   * Invalidate all cache entries related to an invoice.
   * Called by the indexer whenever a relevant contract event is observed.
   */
  async invalidateInvoice(invoiceId: string): Promise<void> {
    await Promise.all([
      this.store.deleteByPattern(invoiceKeyPrefix(invoiceId)),
      this.store.deleteByPattern(bidKeyPrefix(invoiceId)),
    ]);
  }

  /** Invalidate all best-bid entries for an invoice */
  async invalidateBestBid(invoiceId: string): Promise<void> {
    await this.store.delete(bestBidKey(invoiceId));
  }

  /** Clear the entire cache (e.g. after a full re-index) */
  async flush(): Promise<void> {
    await this.store.clear();
  }

  // Internal helpers

  private async _set<T>(
    key: string,
    value: T,
    ttlMs: number,
    now: number
  ): Promise<void> {
    await this.store.set(key, {
      value,
      cached_at: now,
      expires_at: now + ttlMs,
    });
  }

  private async _revalidate<T>(
    key: string,
    fetcher: () => Promise<T>,
    ttlMs: number
  ): Promise<void> {
    const value = await fetcher();
    await this._set(key, value, ttlMs, Date.now());
  }
}

// ---------------------------------------------------------------------------
// Event-driven invalidation adapter
// ---------------------------------------------------------------------------

/**
 * Connect cache invalidation to indexed on-chain events.
 *
 * Call this after the MonotonicIngester successfully commits each event.
 * The function inspects the event type and evicts the affected cache entries.
 */
export async function invalidateOnEvent(
  cache: ReadThroughCache,
  eventType: string,
  invoiceId: string
): Promise<void> {
  // All events that touch an invoice must bust its detail cache
  const invoiceEvents = new Set([
    "invoice.uploaded",
    "invoice.verified",
    "invoice.cancelled",
    "invoice.settled",
    "invoice.defaulted",
    "invoice.expired",
    "invoice.funded",
    "payment.recorded",
    "payment.partial",
    "dispute.created",
    "dispute.resolved",
  ]);

  // Bid events only need to bust the best-bid cache
  const bidEvents = new Set([
    "bid.placed",
    "bid.accepted",
    "bid.withdrawn",
    "bid.expired",
  ]);

  if (invoiceEvents.has(eventType)) {
    await cache.invalidateInvoice(invoiceId);
    return;
  }

  if (bidEvents.has(eventType)) {
    await cache.invalidateBestBid(invoiceId);
  }
}
