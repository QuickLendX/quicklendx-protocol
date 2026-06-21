/**
 * Tests for Read-Through Cache with Event-Driven Invalidation (#877)
 *
 * Run with: npx jest app/lib/cache/__tests__/read-through-cache.test.ts
 */

import {
  bestBidKey,
  CacheGetResult,
  InMemoryCacheStore,
  invalidateOnEvent,
  invoiceDetailKey,
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
