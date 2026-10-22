/**
 * Read-Through Cache with Event-Driven Invalidation (#877, #2881)
 *
 * Provides:
 * - A typed in-memory cache (swap CacheStore implementation for Redis in prod)
 * - Configurable TTL per cache key with absolute freshness windows
 * - Event-driven invalidation: indexed events evict the affected cache entries
 * - Stale-while-revalidate flag with an explicit `is_stale` indicator on
 *   responses – financial data is NEVER served silently stale
 * - Cache-aside (read-through) helper that populates on miss
 *
 * Consistency guarantee
 * ---------------------
 * Every cache key carries a monotonically increasing *generation*. An
 * invalidation bumps the generation of every key it affects – including keys
 * that are not cached yet but have a fetch in flight – before it touches the
 * store. A fetch captures the generation of its key before it starts and only
 * writes to the store while that generation is still current.
 *
 * A fetch that lost the race against an invalidation is never cached: its
 * value is handed back to the caller with `is_stale: true` so the caller can
 * surface the freshness warning, and the next read refetches. Financial data
 * is therefore never served silently stale.
 *
 * Fetches are single-flight per key: concurrent misses share one fetch, and
 * at most one background revalidation runs per key at a time. A rejected
 * fetch releases the slot, so the next read retries instead of replaying the
 * failure.
 *
 * Key prefixes are delimiter-terminated (`invoice:<id>:`, `best_bid:<id>:`),
 * so invalidating one invoice can never evict a different invoice whose id
 * merely starts with the same characters.
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
  /** True when the value was served from the cache. */
  hit: boolean;
  value: T | null;
  /**
   * True when the value must not be treated as fresh.
   *
   * Set when a cached entry is past its freshness window, and also when a
   * fetch lost the race against an invalidation: the value is handed back to
   * the caller, but it was deliberately not cached because it may predate
   * the event that triggered the invalidation.
   */
  is_stale?: boolean;
  /**
   * Unix ms when the value was cached. Absent when the value was not cached
   * (a fetch that lost the race against an invalidation).
   */
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
  serve_stale: false, // safe default for financial data
};

/**
 * Snapshot of the cache state a fetch started from.
 *
 * A fetch may only write to the store while this snapshot is still current.
 */
interface CacheVersion {
  /** Generation of the key the fetch writes to. */
  generation: number;
  /** Global epoch, bumped by `flush()`. */
  epoch: number;
}

/** Internal result of a single-flight fetch. */
interface FetchOutcome<T> {
  value: T;
  /** True when the value was written to the store. */
  cached: boolean;
  /** Timestamp recorded on the written entry; absent when it was not written. */
  cached_at?: number;
}

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

/**
 * Prefix for all invoice-related cache keys (used for bulk invalidation).
 *
 * Delimiter-terminated: `invoice:0xab:` never matches `invoice:0xabc`.
 */
export function invoiceKeyPrefix(invoiceId: string): string {
  return `invoice:${invoiceId}:`;
}

/**
 * Prefix for all bid-related cache keys.
 *
 * Delimiter-terminated: `best_bid:0xab:` never matches `best_bid:0xabc`.
 */
export function bidKeyPrefix(invoiceId: string): string {
  return `best_bid:${invoiceId}:`;
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

  /**
   * Per-key invalidation generation. Monotonically increasing: a fetch may
   * only write to the store while the generation it captured is still
   * current.
   */
  private readonly generations = new Map<string, number>();

  /**
   * In-flight fetches, keyed by cache key. Concurrent misses share one
   * fetch, and at most one background revalidation runs per key.
   */
  private readonly inFlight = new Map<string, Promise<unknown>>();

  /**
   * Global epoch, bumped by `flush()`. A fetch that started before the flush
   * can no longer write to the store.
   */
  private epoch = 0;

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
   *
   * The fetch is single-flight: concurrent misses share one `fetcher` call.
   * If an invalidation lands while the fetch is in flight, the fetched value
   * is not cached and is returned with `is_stale: true`.
   */
  async get<T>(
    key: string,
    fetcher: () => Promise<T>,
    ttlMs: number = this.config.default_ttl_ms
  ): Promise<CacheGetResult<T>> {
    // The version is captured before the first await: an invalidation that
    // lands while the store lookup is in flight has to be visible to the
    // fetch that follows, otherwise pre-event data would be cached.
    const version = this._captureVersion(key);

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
        this._revalidate(key, fetcher, ttlMs, version).catch((err) =>
          console.error(
            `[cache] Background revalidation failed for "${key}":`,
            err
          )
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
    const outcome = await this._fetch(key, fetcher, ttlMs, version, now);
    if (!outcome.cached) {
      // An invalidation won the race: the value is handed back to the caller
      // marked stale, but it is deliberately not cached.
      return { hit: false, value: outcome.value, is_stale: true };
    }

    return {
      hit: false,
      value: outcome.value,
      is_stale: false,
      cached_at: outcome.cached_at,
    };
  }

  /** Explicitly set a value in the cache */
  async set<T>(key: string, value: T, ttlMs?: number): Promise<void> {
    await this._set(
      key,
      value,
      ttlMs ?? this.config.default_ttl_ms,
      Date.now()
    );
  }

  /** Explicitly invalidate a single key */
  async invalidate(key: string): Promise<void> {
    // The generation is bumped before the store is touched, so a fetch that
    // is already in flight cannot write the value it is holding.
    this._bumpGeneration(key);
    await this.store.delete(key);
  }

  /**
   * Invalidate all cache entries related to an invoice.
   * Called by the indexer whenever a relevant contract event is observed.
   */
  async invalidateInvoice(invoiceId: string): Promise<void> {
    const invoiceKey = invoiceDetailKey(invoiceId);
    const bidKey = bestBidKey(invoiceId);

    // Generations are bumped synchronously, before the first await, so that a
    // fetch already in flight for one of these keys observes the
    // invalidation even if it resolves while the deletes are still running.
    this._bumpGeneration(invoiceKey);
    this._bumpGeneration(bidKey);
    this._bumpPrefixGeneration(invoiceKeyPrefix(invoiceId));
    this._bumpPrefixGeneration(bidKeyPrefix(invoiceId));

    // The exact keys are deleted alongside the delimiter-terminated
    // prefixes: `invoice:0xab` is evicted, `invoice:0xabc` is not.
    await Promise.all([
      this.store.delete(invoiceKey),
      this.store.delete(bidKey),
      this.store.deleteByPattern(invoiceKeyPrefix(invoiceId)),
      this.store.deleteByPattern(bidKeyPrefix(invoiceId)),
    ]);
  }

  /** Invalidate all best-bid entries for an invoice */
  async invalidateBestBid(invoiceId: string): Promise<void> {
    const key = bestBidKey(invoiceId);
    this._bumpGeneration(key);
    await this.store.delete(key);
  }

  /** Clear the entire cache (e.g. after a full re-index) */
  async flush(): Promise<void> {
    // A fetch that started before the flush must not repopulate the cache
    // with data that predates the re-index.
    this.epoch += 1;
    await this.store.clear();
  }

  // Internal helpers

  /**
   * Capture the version of the cache state a fetch starts from.
   *
   * The key is registered on the way, so that a prefix invalidation can find
   * it even when it is not cached yet but has a fetch in flight.
   */
  private _captureVersion(key: string): CacheVersion {
    const generation = this._generation(key);
    if (!this.generations.has(key)) {
      this.generations.set(key, generation);
    }
    return { generation, epoch: this.epoch };
  }

  /** Current generation of a key. */
  private _generation(key: string): number {
    return this.generations.get(key) ?? 0;
  }

  /** Bump the generation of a single key. */
  private _bumpGeneration(key: string): void {
    this.generations.set(key, this._generation(key) + 1);
  }

  /**
   * Bump the generation of every known key that starts with `prefix`.
   *
   * Only keys the cache already knows about are affected: a key that was
   * never fetched cannot have a fetch in flight.
   */
  private _bumpPrefixGeneration(prefix: string): void {
    for (const key of Array.from(this.generations.keys())) {
      if (key.startsWith(prefix)) {
        this.generations.set(key, this._generation(key) + 1);
      }
    }
  }

  /** True while `version` still describes the current state of `key`. */
  private _isCurrent(key: string, version: CacheVersion): boolean {
    return (
      version.generation === this._generation(key) &&
      version.epoch === this.epoch
    );
  }

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

  /**
   * Fetch `key` and cache the result, unless an invalidation wins the race.
   *
   * The fetch is single-flight: the first caller starts it and every other
   * caller awaits the same promise.
   */
  private _fetch<T>(
    key: string,
    fetcher: () => Promise<T>,
    ttlMs: number,
    version: CacheVersion,
    now: number
  ): Promise<FetchOutcome<T>> {
    const inFlight = this.inFlight.get(key) as
      | Promise<FetchOutcome<T>>
      | undefined;
    if (inFlight !== undefined) {
      return inFlight;
    }

    const promise = this._fetchAndCache(key, fetcher, ttlMs, version, now);
    this.inFlight.set(key, promise);

    // The slot is released once the fetch settles, successfully or not, so a
    // rejected fetch does not poison the map and the next read retries.
    const release = () => {
      if (this.inFlight.get(key) === promise) {
        this.inFlight.delete(key);
      }
    };
    promise.then(release, release);

    return promise;
  }

  private async _fetchAndCache<T>(
    key: string,
    fetcher: () => Promise<T>,
    ttlMs: number,
    version: CacheVersion,
    now: number
  ): Promise<FetchOutcome<T>> {
    const value = await fetcher();
    const cached = await this._setIfCurrent(key, value, ttlMs, now, version);
    return { value, cached, cached_at: cached ? now : undefined };
  }

  /**
   * Write a fetched value to the store, but only while the version the fetch
   * started from is still current.
   *
   * Returns false when an invalidation won the race, in which case the value
   * is not cached and the caller is told it may be stale.
   */
  private async _setIfCurrent<T>(
    key: string,
    value: T,
    ttlMs: number,
    now: number,
    version: CacheVersion
  ): Promise<boolean> {
    if (!this._isCurrent(key, version)) {
      return false;
    }

    await this.store.set(key, {
      value,
      cached_at: now,
      expires_at: now + ttlMs,
    });

    if (this._isCurrent(key, version)) {
      return true;
    }

    // An invalidation landed while the write was in flight. The entry just
    // written is pre-event data, so it is dropped again – but only if the
    // store still holds the entry that was written.
    const entry = await this.store.get(key);
    if (entry !== null && entry.cached_at === now) {
      await this.store.delete(key);
    }
    return false;
  }

  private _revalidate<T>(
    key: string,
    fetcher: () => Promise<T>,
    ttlMs: number,
    version: CacheVersion
  ): Promise<FetchOutcome<T>> {
    return this._fetch(key, fetcher, ttlMs, version, Date.now());
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
