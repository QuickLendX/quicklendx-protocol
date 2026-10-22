/**
 * Tests for Read-Through Cache with Event-Driven Invalidation (#877)
 *
 * Run with: npx jest app/lib/cache/__tests__/read-through-cache.test.ts
 */

import {
  bestBidKey,
  bidKeyPrefix,
  CacheGetResult,
  InMemoryCacheStore,
  invalidateOnEvent,
  invoiceDetailKey,
  invoiceKeyPrefix,
  ReadThroughCache,
} from "../read-through-cache";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const INVOICE_ID = "inv_abc123";
const TTL_MS = 5_000; // 5 s for tests

function makeFetcher<T>(value: T, callCount = { n: 0 }) {
  return jest.fn(async () => {
    callCount.n++;
    return value;
  });
}

/**
 * A promise whose settlement is controlled by the test, so that interleavings
 * between a fetch and an invalidation are deterministic instead of timing
 * dependent.
 */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/**
 * Yield to the macrotask queue, so every pending microtask – a started
 * fetch, a background revalidation – has run to completion.
 */
async function flushPendingTasks(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

// ---------------------------------------------------------------------------
// Basic read-through behaviour
// ---------------------------------------------------------------------------

describe("ReadThroughCache – basic reads", () => {
  it("calls fetcher on a cache miss", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });
    const fetcher = makeFetcher({ amount: 1000 });

    const result = await cache.get(invoiceDetailKey(INVOICE_ID), fetcher);

    expect(result.hit).toBe(false);
    expect(result.value).toEqual({ amount: 1000 });
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("serves from cache on a subsequent read", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });
    const fetcher = makeFetcher({ amount: 2000 });

    await cache.get(invoiceDetailKey(INVOICE_ID), fetcher);
    const second = await cache.get(invoiceDetailKey(INVOICE_ID), fetcher);

    expect(second.hit).toBe(true);
    expect(fetcher).toHaveBeenCalledTimes(1); // no second fetch
  });

  it("reports is_stale = false on a fresh hit", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });
    const fetcher = makeFetcher({ price: 50 });

    await cache.get(bestBidKey(INVOICE_ID), fetcher);
    const hit = await cache.get(bestBidKey(INVOICE_ID), fetcher);

    expect(hit.is_stale).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Staleness
// ---------------------------------------------------------------------------

describe("ReadThroughCache – stale entries", () => {
  it("re-fetches when entry is expired and serve_stale=false (default)", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, {
      default_ttl_ms: 1, // expires immediately
      serve_stale: false,
    });
    const fetcher = makeFetcher({ x: 1 });

    await cache.get("key", fetcher);
    // Wait for TTL to elapse
    await new Promise((r) => setTimeout(r, 5));
    await cache.get("key", fetcher);

    expect(fetcher).toHaveBeenCalledTimes(2);
  });

  it("returns stale value with is_stale=true when serve_stale=true", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, {
      default_ttl_ms: 1,
      serve_stale: true,
    });
    const fetcher = makeFetcher({ x: 99 });

    await cache.get("key", fetcher);
    await new Promise((r) => setTimeout(r, 5));
    const result = await cache.get("key", fetcher);

    expect(result.hit).toBe(true);
    expect(result.is_stale).toBe(true);
    expect(result.value).toEqual({ x: 99 });
  });
});

// ---------------------------------------------------------------------------
// Manual invalidation
// ---------------------------------------------------------------------------

describe("ReadThroughCache – explicit invalidation", () => {
  it("invalidateInvoice evicts both invoice detail and best-bid keys", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });

    await cache.set(invoiceDetailKey(INVOICE_ID), { status: "Verified" });
    await cache.set(bestBidKey(INVOICE_ID), { bid_amount: 500 });

    await cache.invalidateInvoice(INVOICE_ID);

    const detailFetcher = makeFetcher({ status: "Funded" });
    const bidFetcher = makeFetcher({ bid_amount: 600 });

    const d = await cache.get(invoiceDetailKey(INVOICE_ID), detailFetcher);
    const b = await cache.get(bestBidKey(INVOICE_ID), bidFetcher);

    expect(d.hit).toBe(false);
    expect(b.hit).toBe(false);
    expect(detailFetcher).toHaveBeenCalledTimes(1);
    expect(bidFetcher).toHaveBeenCalledTimes(1);
  });

  it("invalidateBestBid only removes the best-bid key", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });

    await cache.set(invoiceDetailKey(INVOICE_ID), { status: "Funded" });
    await cache.set(bestBidKey(INVOICE_ID), { bid_amount: 700 });

    await cache.invalidateBestBid(INVOICE_ID);

    const detailFetcher = makeFetcher({ status: "Funded" });
    const bidFetcher = makeFetcher({ bid_amount: 800 });

    const d = await cache.get(invoiceDetailKey(INVOICE_ID), detailFetcher);
    const b = await cache.get(bestBidKey(INVOICE_ID), bidFetcher);

    expect(d.hit).toBe(true); // detail NOT evicted
    expect(b.hit).toBe(false); // best-bid evicted
  });
});

// ---------------------------------------------------------------------------
// Event-driven invalidation
// ---------------------------------------------------------------------------

describe("invalidateOnEvent", () => {
  async function seedCache(cache: ReadThroughCache) {
    await cache.set(invoiceDetailKey(INVOICE_ID), { status: "Verified" });
    await cache.set(bestBidKey(INVOICE_ID), { bid_amount: 100 });
  }

  it("busts invoice detail and bid cache on invoice.settled", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });
    await seedCache(cache);

    await invalidateOnEvent(cache, "invoice.settled", INVOICE_ID);

    const d = await cache.get(invoiceDetailKey(INVOICE_ID), makeFetcher({}));
    const b = await cache.get(bestBidKey(INVOICE_ID), makeFetcher({}));
    expect(d.hit).toBe(false);
    expect(b.hit).toBe(false);
  });

  it("busts only best-bid cache on bid.placed", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });
    await seedCache(cache);

    await invalidateOnEvent(cache, "bid.placed", INVOICE_ID);

    const d = await cache.get(invoiceDetailKey(INVOICE_ID), makeFetcher({}));
    const b = await cache.get(bestBidKey(INVOICE_ID), makeFetcher({}));
    expect(d.hit).toBe(true); // detail cache untouched
    expect(b.hit).toBe(false); // bid cache evicted
  });

  it("does nothing for unknown event types", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });
    await seedCache(cache);

    await invalidateOnEvent(cache, "unknown.event", INVOICE_ID);

    const d = await cache.get(invoiceDetailKey(INVOICE_ID), makeFetcher({}));
    expect(d.hit).toBe(true); // still cached
  });
});

// ---------------------------------------------------------------------------
// Flush
// ---------------------------------------------------------------------------

describe("ReadThroughCache.flush", () => {
  it("clears all entries", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });

    await cache.set("key1", 1);
    await cache.set("key2", 2);
    await cache.flush();

    expect(store.size).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Invalidation races (#2881)
//
// An on-chain event can land while a fetch is already in flight. The cache
// must not write the value that fetch is holding, because that value
// predates the event.
// ---------------------------------------------------------------------------

describe("ReadThroughCache – invalidation races", () => {
  it("does not cache a pre-event value when an invalidation lands during a miss fetch", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store);
    const key = bestBidKey("0x1");

    const pending = deferred<string>();
    const started = deferred<void>();
    const fetcher = jest.fn(() => {
      started.resolve();
      return pending.promise;
    });

    const first = cache.get(key, fetcher);

    // The fetch is in flight when the event is observed.
    await started.promise;
    await cache.invalidateBestBid("0x1");
    pending.resolve("bid-before-event");

    const firstResult = await first;
    expect(firstResult.value).toBe("bid-before-event");
    expect(firstResult.is_stale).toBe(true);
    expect(firstResult.hit).toBe(false);
    expect(firstResult.cached_at).toBeUndefined();

    // The pre-event value must not have reached the store.
    expect(await store.get(key)).toBeNull();

    // The next read refetches and serves the post-event value.
    const afterFetcher = jest.fn(async () => "bid-after-event");
    const again = await cache.get(key, afterFetcher);

    expect(afterFetcher).toHaveBeenCalledTimes(1);
    expect(again.value).toBe("bid-after-event");
    expect(again.is_stale).toBe(false);
    expect(again.hit).toBe(false);
  });

  it("does not write a revalidated value when an invalidation lands during the revalidation", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, {
      default_ttl_ms: 1,
      serve_stale: true,
    });
    const key = invoiceDetailKey(INVOICE_ID);

    await cache.set(key, { status: "Verified" });
    await new Promise((resolve) => setTimeout(resolve, 5));

    const pending = deferred<{ status: string }>();
    const started = deferred<void>();
    const fetcher = jest.fn(() => {
      started.resolve();
      return pending.promise;
    });

    const stale = await cache.get(key, fetcher);
    expect(stale.hit).toBe(true);
    expect(stale.is_stale).toBe(true);
    expect(stale.value).toEqual({ status: "Verified" });

    await started.promise;
    await cache.invalidateInvoice(INVOICE_ID);
    pending.resolve({ status: "Funded" });
    await flushPendingTasks();

    // The store holds no entry for the key: the revalidation must not have
    // written the value it fetched before the invalidation.
    expect(await store.get(key)).toBeNull();
  });

  it("does not write a fetched value when the key is invalidated mid-fetch", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store);
    const key = invoiceDetailKey(INVOICE_ID);

    const pending = deferred<{ status: string }>();
    const started = deferred<void>();
    const fetcher = jest.fn(() => {
      started.resolve();
      return pending.promise;
    });

    const read = cache.get(key, fetcher);
    await started.promise;
    await cache.invalidate(key);
    pending.resolve({ status: "Funded" });

    const result = await read;
    expect(result.is_stale).toBe(true);
    expect(await store.get(key)).toBeNull();
  });

  it("does not let an in-flight fetch repopulate the cache after a flush", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store);
    const key = invoiceDetailKey(INVOICE_ID);

    const pending = deferred<{ status: string }>();
    const started = deferred<void>();
    const fetcher = jest.fn(() => {
      started.resolve();
      return pending.promise;
    });

    const read = cache.get(key, fetcher);
    await started.promise;
    await cache.flush();
    pending.resolve({ status: "Funded" });

    const result = await read;
    expect(result.is_stale).toBe(true);
    expect(await store.get(key)).toBeNull();
  });

  it("does not cache a sub-key whose prefix is invalidated mid-fetch", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store);
    const key = `${invoiceKeyPrefix(INVOICE_ID)}history`;

    const pending = deferred<string>();
    const started = deferred<void>();
    const fetcher = jest.fn(() => {
      started.resolve();
      return pending.promise;
    });

    const read = cache.get(key, fetcher);
    await started.promise;
    await cache.invalidateInvoice(INVOICE_ID);
    pending.resolve("history-before-event");

    const result = await read;
    expect(result.is_stale).toBe(true);
    expect(await store.get(key)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Single-flight (#2881)
// ---------------------------------------------------------------------------

describe("ReadThroughCache – single-flight fetches", () => {
  it("calls the fetcher once for concurrent misses and shares the result", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store);

    const pending = deferred<{ amount: number }>();
    const fetcher = jest.fn(() => pending.promise);

    const reads = Array.from({ length: 10 }, () => cache.get("k", fetcher));
    await flushPendingTasks();

    expect(fetcher).toHaveBeenCalledTimes(1);

    pending.resolve({ amount: 42 });
    const results = await Promise.all(reads);

    expect(fetcher).toHaveBeenCalledTimes(1);
    for (const result of results) {
      expect(result.value).toEqual({ amount: 42 });
      expect(result.hit).toBe(false);
      expect(result.is_stale).toBe(false);
    }
  });

  it("starts exactly one background revalidation for concurrent stale reads", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, {
      default_ttl_ms: 1,
      serve_stale: true,
    });

    await cache.set("k", { x: 1 });
    await new Promise((resolve) => setTimeout(resolve, 5));

    const pending = deferred<{ x: number }>();
    const fetcher = jest.fn(() => pending.promise);

    const reads = Array.from({ length: 10 }, () => cache.get("k", fetcher));
    await flushPendingTasks();

    // Every stale read is served immediately, and only one revalidation is
    // started for the key.
    expect(fetcher).toHaveBeenCalledTimes(1);
    for (const result of await Promise.all(reads)) {
      expect(result.hit).toBe(true);
      expect(result.is_stale).toBe(true);
      expect(result.value).toEqual({ x: 1 });
    }

    pending.resolve({ x: 2 });
    await flushPendingTasks();

    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(await store.get("k")).not.toBeNull();
  });

  it("propagates a fetch failure to every concurrent caller and retries afterwards", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store);

    const failure = new Error("backend unavailable");
    const pending = deferred<string>();
    const fetcher = jest.fn(() => pending.promise);

    const reads = Array.from({ length: 10 }, () => cache.get("k", fetcher));
    await flushPendingTasks();

    expect(fetcher).toHaveBeenCalledTimes(1);

    pending.reject(failure);
    const settled = await Promise.allSettled(reads);

    for (const outcome of settled) {
      expect(outcome.status).toBe("rejected");
      if (outcome.status === "rejected") {
        expect(outcome.reason).toBe(failure);
      }
    }

    // The rejected fetch must not poison the single-flight map: the next
    // read calls the fetcher again.
    const retry = jest.fn(async () => "recovered");
    const result = await cache.get("k", retry);

    expect(retry).toHaveBeenCalledTimes(1);
    expect(result.value).toBe("recovered");
    expect(result.is_stale).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Key prefixes (#2881)
// ---------------------------------------------------------------------------

describe("ReadThroughCache – key prefixes", () => {
  it("terminates the bulk-invalidation prefixes with a delimiter", () => {
    expect(invoiceKeyPrefix("0xab")).toBe("invoice:0xab:");
    expect(bidKeyPrefix("0xab")).toBe("best_bid:0xab:");
  });

  it("leaves an invoice whose id shares a prefix with the invalidated id intact", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });

    await cache.set(invoiceDetailKey("0xab"), { id: "0xab" });
    await cache.set(bestBidKey("0xab"), { id: "0xab" });
    await cache.set(invoiceDetailKey("0xabc"), { id: "0xabc" });
    await cache.set(bestBidKey("0xabc"), { id: "0xabc" });

    await cache.invalidateInvoice("0xab");

    expect(await store.get(invoiceDetailKey("0xab"))).toBeNull();
    expect(await store.get(bestBidKey("0xab"))).toBeNull();
    expect(await store.get(invoiceDetailKey("0xabc"))).not.toBeNull();
    expect(await store.get(bestBidKey("0xabc"))).not.toBeNull();
  });

  it("leaves an invoice whose id shares a prefix with the invalidated id intact on a bid event", async () => {
    const store = new InMemoryCacheStore();
    const cache = new ReadThroughCache(store, { default_ttl_ms: TTL_MS });

    await cache.set(invoiceDetailKey("0xab"), { id: "0xab" });
    await cache.set(bestBidKey("0xab"), { id: "0xab" });
    await cache.set(invoiceDetailKey("0xabc"), { id: "0xabc" });
    await cache.set(bestBidKey("0xabc"), { id: "0xabc" });

    await cache.invalidateBestBid("0xab");

    expect(await store.get(bestBidKey("0xab"))).toBeNull();
    expect(await store.get(bestBidKey("0xabc"))).not.toBeNull();
    expect(await store.get(invoiceDetailKey("0xabc"))).not.toBeNull();
  });
});
