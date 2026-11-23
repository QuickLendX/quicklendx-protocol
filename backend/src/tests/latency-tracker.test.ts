/**
 * Latency Tracker Tests
 *
 * Coverage targets:
 * ─ Percentile correctness on known distributions
 * ─ Window rotation (stale samples excluded)
 * ─ Route key normalisation (param routes collapse)
 * ─ Memory cap (MAX_ROUTES hard ceiling, overflow sentinel)
 * ─ Concurrency safety (rapid interleaved record/getStats)
 * ─ Edge cases: empty tracker, single sample, full ring-buffer wrap
 * ─ Middleware integration: latencyTracker.record() called from request-logger
 */

import {
  LatencyTracker,
  latencyTracker,
  normaliseRoute,
  MAX_ROUTES,
  BUCKET_SIZE,
  DEFAULT_WINDOW_MS,
  OVERFLOW_KEY,
} from "../services/latencyTracker";

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Feed N evenly-spaced duration values into a tracker for a given route. */
function feed(
  tracker: LatencyTracker,
  route: string,
  durations: number[],
  nowMs = Date.now()
): void {
  for (const d of durations) {
    tracker.recordNormalised(route, d, nowMs);
  }
}

/** Returns a sorted array of integers from start to end (inclusive). */
function range(start: number, end: number): number[] {
  return Array.from({ length: end - start + 1 }, (_, i) => start + i);
}

// ── normaliseRoute ────────────────────────────────────────────────────────────

describe("normaliseRoute()", () => {
  it("leaves plain API paths unchanged", () => {
    expect(normaliseRoute("/api/v1/admin/monitoring")).toBe(
      "/api/v1/admin/monitoring"
    );
    expect(normaliseRoute("/api/v1/invoices")).toBe("/api/v1/invoices");
    expect(normaliseRoute("/health")).toBe("/health");
  });

  it("collapses ULID path segments to :id", () => {
    // 26-char uppercase base32 ULID
    expect(normaliseRoute("/api/v1/invoices/01HX1Y2Z3A4B5C6D7E8F9G0H1J")).toBe(
      "/api/v1/invoices/:id"
    );
  });

  it("collapses UUID path segments to :id", () => {
    expect(
      normaliseRoute("/api/v1/bids/550e8400-e29b-41d4-a716-446655440000")
    ).toBe("/api/v1/bids/:id");
  });

  it("collapses long hex strings to :id", () => {
    expect(normaliseRoute("/api/v1/invoices/deadbeef1234abcd")).toBe(
      "/api/v1/invoices/:id"
    );
  });

  it("collapses bare integer segments to :id", () => {
    expect(normaliseRoute("/api/v1/settlements/42")).toBe(
      "/api/v1/settlements/:id"
    );
    expect(normaliseRoute("/api/v1/items/100/details")).toBe(
      "/api/v1/items/:id/details"
    );
  });

  it("deduplicates consecutive :id segments", () => {
    // Would happen if both a UUID and hex match fire
    expect(
      normaliseRoute("/api/v1/550e8400-e29b-41d4-a716-446655440000/deadbeef12345678")
    ).toBe("/api/v1/:id");
  });

  it("lowercases the result", () => {
    expect(normaliseRoute("/API/V1/Invoices")).toBe("/api/v1/invoices");
  });

  it("trims trailing slash except for bare /", () => {
    expect(normaliseRoute("/api/v1/invoices/")).toBe("/api/v1/invoices");
    expect(normaliseRoute("/")).toBe("/");
  });

  it("handles query-string-free path (req.path never has query string)", () => {
    // Express req.path strips the query string already; confirm no issues
    expect(normaliseRoute("/api/v1/invoices")).toBe("/api/v1/invoices");
  });

  it("is idempotent — normalising an already-normalised key is stable", () => {
    const once = normaliseRoute("/api/v1/invoices/01HX1Y2Z3A4B5C6D7E8F9G0H1J");
    expect(normaliseRoute(once)).toBe(once);
  });
});

// ── Percentile correctness ────────────────────────────────────────────────────

describe("LatencyTracker — percentile correctness", () => {
  let tracker: LatencyTracker;
  const NOW = 1_700_000_000_000; // fixed epoch

  beforeEach(() => {
    tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
  });

  it("returns null percentiles for a route with no samples", () => {
    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    expect(stats.routes).toHaveLength(0);
  });

  it("returns correct percentiles for a perfectly uniform distribution (1–100)", () => {
    // Feed 1ms … 100ms in order
    feed(tracker, "/api/v1/invoices", range(1, 100), NOW);
    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    const route = stats.routes.find((r) => r.route === "/api/v1/invoices");
    expect(route).toBeDefined();
    expect(route!.count).toBe(100);
    // p50 = 50th value of [1..100] = 50
    expect(route!.p50).toBe(50);
    // p95 = 95th value = 95
    expect(route!.p95).toBe(95);
    // p99 = 99th value = 99
    expect(route!.p99).toBe(99);
    expect(route!.min).toBe(1);
    expect(route!.max).toBe(100);
  });

  it("returns correct percentiles for a skewed distribution", () => {
    // 99 fast requests (1ms) + 1 slow spike (500ms)
    const durations = [...Array(99).fill(1), 500];
    feed(tracker, "/api/v1/bids", durations, NOW);
    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    const route = stats.routes.find((r) => r.route === "/api/v1/bids");
    expect(route!.p50).toBe(1);
    expect(route!.p95).toBe(1);
    expect(route!.p99).toBe(500);
    expect(route!.max).toBe(500);
  });

  it("returns correct percentiles for a single-sample route", () => {
    tracker.recordNormalised("/api/v1/health", 42, NOW);
    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    const route = stats.routes.find((r) => r.route === "/api/v1/health");
    expect(route!.count).toBe(1);
    expect(route!.p50).toBe(42);
    expect(route!.p95).toBe(42);
    expect(route!.p99).toBe(42);
    expect(route!.min).toBe(42);
    expect(route!.max).toBe(42);
  });

  it("handles 1000 samples correctly", () => {
    // 1000 samples: values 1..1000
    feed(tracker, "/api/v1/settlements", range(1, 1000), NOW);
    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    const route = stats.routes.find((r) => r.route === "/api/v1/settlements");
    // With 1000 samples p50 = ceil(500/100*1000)-1 = 499th index = 500
    expect(route!.p50).toBe(500);
    expect(route!.p95).toBe(950);
    expect(route!.p99).toBe(990);
  });

  it("tracks multiple routes independently", () => {
    feed(tracker, "/api/v1/invoices", [10, 20, 30], NOW);
    feed(tracker, "/api/v1/bids", [100, 200, 300], NOW);

    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    const inv = stats.routes.find((r) => r.route === "/api/v1/invoices");
    const bids = stats.routes.find((r) => r.route === "/api/v1/bids");

    expect(inv!.p50).toBe(20);
    expect(bids!.p50).toBe(200);
  });
});

// ── Window rotation ───────────────────────────────────────────────────────────

describe("LatencyTracker — window rotation", () => {
  const WINDOW = 60_000; // 1-minute window for tests
  let tracker: LatencyTracker;

  beforeEach(() => {
    tracker = new LatencyTracker(WINDOW);
  });

  it("includes samples within the window", () => {
    const now = 1_700_000_100_000;
    feed(tracker, "/api/v1/invoices", [50, 60, 70], now);
    const stats = tracker.getStats(WINDOW, now);
    expect(stats.routes[0].count).toBe(3);
  });

  it("excludes samples older than windowMs", () => {
    const old = 1_700_000_000_000;
    const now = old + WINDOW + 1_000; // 1s past the window boundary

    // Record old samples
    feed(tracker, "/api/v1/invoices", [10, 20, 30], old);
    // Record fresh sample
    tracker.recordNormalised("/api/v1/invoices", 99, now);

    const stats = tracker.getStats(WINDOW, now);
    const route = stats.routes[0];
    // Only the fresh sample should appear
    expect(route.count).toBe(1);
    expect(route.p50).toBe(99);
  });

  it("returns count=0 when all samples have aged out", () => {
    const old = 1_700_000_000_000;
    const now = old + WINDOW + 1_000;
    feed(tracker, "/api/v1/invoices", [50], old);

    const stats = tracker.getStats(WINDOW, now);
    expect(stats.routes[0].count).toBe(0);
    expect(stats.routes[0].p50).toBeNull();
    expect(stats.routes[0].p95).toBeNull();
    expect(stats.routes[0].p99).toBeNull();
  });

  it("boundary sample at exactly cutoff is included", () => {
    const now = 1_700_000_100_000;
    const cutoff = now - WINDOW;
    // Record exactly at cutoff
    tracker.recordNormalised("/api/v1/invoices", 55, cutoff);

    const stats = tracker.getStats(WINDOW, now);
    expect(stats.routes[0].count).toBe(1);
    expect(stats.routes[0].p50).toBe(55);
  });

  it("supports custom windowMs override at query time", () => {
    const now = 1_700_000_100_000;
    // Record two batches at different ages
    tracker.recordNormalised("/api/v1/invoices", 10, now - 30_000); // 30s ago
    tracker.recordNormalised("/api/v1/invoices", 20, now - 90_000); // 90s ago

    // 1-minute window should see only the 30s sample
    const stats1m = tracker.getStats(60_000, now);
    expect(stats1m.routes[0].count).toBe(1);

    // 2-minute window should see both
    const stats2m = tracker.getStats(120_000, now);
    expect(stats2m.routes[0].count).toBe(2);
  });
});

// ── Ring-buffer wrap ──────────────────────────────────────────────────────────

describe("LatencyTracker — ring-buffer wrap", () => {
  it("overwrites oldest sample when buffer is full", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = 1_700_000_000_000;

    // Fill the buffer: BUCKET_SIZE samples at a known time
    for (let i = 0; i < BUCKET_SIZE; i++) {
      tracker.recordNormalised("/ring", i + 1, NOW);
    }

    // All BUCKET_SIZE samples should be in window
    const before = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    expect(before.routes[0].count).toBe(BUCKET_SIZE);

    // Write one more (wraps around, overwrites oldest slot)
    tracker.recordNormalised("/ring", 9999, NOW);

    // Still BUCKET_SIZE samples total (ring overwrite, not growth)
    const after = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    expect(after.routes[0].count).toBe(BUCKET_SIZE);

    // The overwrite value 9999 should now be tracked
    expect(after.routes[0].max).toBe(9999);
  });
});

// ── Memory cap ────────────────────────────────────────────────────────────────

describe("LatencyTracker — memory cap / route cardinality", () => {
  it("caps distinct route keys at MAX_ROUTES", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = Date.now();

    // Fill up to the cap
    for (let i = 0; i < MAX_ROUTES; i++) {
      tracker.recordNormalised(`/route/${i}`, 10, NOW);
    }
    expect(tracker.routeCount).toBe(MAX_ROUTES);

    // One more distinct key should NOT create a new bucket
    tracker.recordNormalised("/overflow/route", 10, NOW);
    expect(tracker.routeCount).toBe(MAX_ROUTES); // still at cap
  });

  it("routes excess to the OVERFLOW_KEY sentinel", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = Date.now();

    for (let i = 0; i < MAX_ROUTES; i++) {
      tracker.recordNormalised(`/route/${i}`, 10, NOW);
    }

    // Force overflow
    tracker.recordNormalised("/this/is/overflow", 77, NOW);

    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    expect(stats.overflowed).toBe(true);
    const overflow = stats.routes.find((r) => r.route === OVERFLOW_KEY);
    expect(overflow).toBeDefined();
    expect(overflow!.count).toBeGreaterThan(0);
  });

  it("reports overflowed=false when under cap", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    tracker.recordNormalised("/ok", 10, Date.now());
    const stats = tracker.getStats();
    expect(stats.overflowed).toBe(false);
  });

  it("getStats returns totalRoutes equal to routeCount", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = Date.now();
    feed(tracker, "/a", [1, 2], NOW);
    feed(tracker, "/b", [3, 4], NOW);
    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    expect(stats.totalRoutes).toBe(tracker.routeCount);
    expect(stats.maxRoutes).toBe(MAX_ROUTES);
  });
});

// ── Route key normalisation via record() ──────────────────────────────────────

describe("LatencyTracker — param route collapsing via record()", () => {
  let tracker: LatencyTracker;
  const NOW = Date.now();

  beforeEach(() => {
    tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
  });

  it("collapses ULID-keyed invoice routes into one bucket", () => {
    tracker.record("/api/v1/invoices/01HX1Y2Z3A4B5C6D7E8F9G0H1J", 10, NOW);
    tracker.record("/api/v1/invoices/01AAABBBCCCDDDEEEFFF000111", 20, NOW);
    // Both should land in the same normalised bucket
    expect(tracker.routeCount).toBe(1);
    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    expect(stats.routes[0].route).toBe("/api/v1/invoices/:id");
    expect(stats.routes[0].count).toBe(2);
  });

  it("collapses UUID-keyed bid routes", () => {
    tracker.record(
      "/api/v1/bids/550e8400-e29b-41d4-a716-446655440000",
      15,
      NOW
    );
    tracker.record(
      "/api/v1/bids/6ba7b810-9dad-11d1-80b4-00c04fd430c8",
      25,
      NOW
    );
    expect(tracker.routeCount).toBe(1);
    expect(tracker.getStats(DEFAULT_WINDOW_MS, NOW).routes[0].count).toBe(2);
  });

  it("keeps distinct routes as distinct buckets", () => {
    tracker.record("/api/v1/invoices", 10, NOW);
    tracker.record("/api/v1/bids", 20, NOW);
    tracker.record("/api/v1/settlements", 30, NOW);
    expect(tracker.routeCount).toBe(3);
  });
});

// ── reset() ───────────────────────────────────────────────────────────────────

describe("LatencyTracker — reset()", () => {
  it("clears all buckets", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    tracker.recordNormalised("/a", 10, Date.now());
    tracker.recordNormalised("/b", 20, Date.now());
    expect(tracker.routeCount).toBe(2);

    tracker.reset();
    expect(tracker.routeCount).toBe(0);
    expect(tracker.getStats().routes).toHaveLength(0);
  });
});

// ── getRouteStats() ───────────────────────────────────────────────────────────

describe("LatencyTracker — getRouteStats()", () => {
  it("returns null for an unknown route", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    expect(tracker.getRouteStats("/never/seen")).toBeNull();
  });

  it("returns stats for a known route via raw path", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = Date.now();
    tracker.record("/api/v1/invoices/01HX1Y2Z3A4B5C6D7E8F9G0H1J", 50, NOW);
    const stats = tracker.getRouteStats(
      "/api/v1/invoices/01HX1Y2Z3A4B5C6D7E8F9G0H1J",
      DEFAULT_WINDOW_MS,
      NOW
    );
    expect(stats).not.toBeNull();
    expect(stats!.route).toBe("/api/v1/invoices/:id");
    expect(stats!.p50).toBe(50);
  });
});

// ── TrackerStats shape ────────────────────────────────────────────────────────

describe("LatencyTracker — TrackerStats response shape", () => {
  it("includes all required fields", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = Date.now();
    tracker.recordNormalised("/api/v1/invoices", 100, NOW);

    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);

    expect(typeof stats.windowMs).toBe("number");
    expect(typeof stats.totalRoutes).toBe("number");
    expect(typeof stats.maxRoutes).toBe("number");
    expect(typeof stats.overflowed).toBe("boolean");
    expect(typeof stats.generatedAt).toBe("string");
    expect(Array.isArray(stats.routes)).toBe(true);

    const route = stats.routes[0];
    expect(typeof route.route).toBe("string");
    expect(typeof route.count).toBe("number");
    expect(typeof route.windowMs).toBe("number");
    // Numeric or null
    expect(route.p50 === null || typeof route.p50 === "number").toBe(true);
    expect(route.p95 === null || typeof route.p95 === "number").toBe(true);
    expect(route.p99 === null || typeof route.p99 === "number").toBe(true);
    expect(route.min === null || typeof route.min === "number").toBe(true);
    expect(route.max === null || typeof route.max === "number").toBe(true);
  });

  it("routes are sorted alphabetically by route key", () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = Date.now();
    tracker.recordNormalised("/zzz", 1, NOW);
    tracker.recordNormalised("/aaa", 2, NOW);
    tracker.recordNormalised("/mmm", 3, NOW);

    const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW);
    const keys = stats.routes.map((r) => r.route);
    expect(keys).toEqual([...keys].sort());
  });
});

// ── Concurrency safety ────────────────────────────────────────────────────────

describe("LatencyTracker — concurrency safety", () => {
  it("handles rapid interleaved record() and getStats() without error or NaN", async () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = Date.now();

    // Fire 500 concurrent-ish record + getStats calls
    const promises: Promise<void>[] = [];
    for (let i = 0; i < 500; i++) {
      promises.push(
        Promise.resolve().then(() => {
          tracker.recordNormalised("/concurrent", i % 200, NOW + i);
          const stats = tracker.getStats(DEFAULT_WINDOW_MS, NOW + i);
          const route = stats.routes.find((r) => r.route === "/concurrent");
          if (route && route.p50 !== null) {
            expect(Number.isFinite(route.p50)).toBe(true);
          }
        })
      );
    }
    await Promise.all(promises);
    expect(tracker.routeCount).toBeLessThanOrEqual(MAX_ROUTES);
  });

  it("never grows beyond MAX_ROUTES under concurrent writes to distinct keys", async () => {
    const tracker = new LatencyTracker(DEFAULT_WINDOW_MS);
    const NOW = Date.now();

    const writes = Array.from({ length: MAX_ROUTES + 50 }, (_, i) =>
      Promise.resolve().then(() =>
        tracker.recordNormalised(`/route-concurrent-${i}`, 10, NOW)
      )
    );
    await Promise.all(writes);
    expect(tracker.routeCount).toBeLessThanOrEqual(MAX_ROUTES);
  });
});

// ── Singleton export ──────────────────────────────────────────────────────────

describe("latencyTracker singleton", () => {
  afterEach(() => {
    latencyTracker.reset();
  });

  it("is a LatencyTracker instance", () => {
    expect(latencyTracker).toBeInstanceOf(LatencyTracker);
  });

  it("records and retrieves data via the singleton", () => {
    const NOW = Date.now();
    latencyTracker.recordNormalised("/singleton/test", 123, NOW);
    const stats = latencyTracker.getStats(DEFAULT_WINDOW_MS, NOW);
    expect(stats.routes.some((r) => r.route === "/singleton/test")).toBe(true);
  });
});

// ── Middleware integration — request-logger calls latencyTracker.record() ────

describe("request-logger middleware — latency tracker integration", () => {
  let mockRequest: any;
  let mockResponse: any;
  let nextFn: jest.Mock;

  beforeEach(() => {
    latencyTracker.reset();

    mockRequest = {
      path: "/api/v1/invoices",
      method: "GET",
      headers: {},
      query: {},
      body: null,
    };

    let finishCb: (() => void) | null = null;
    let jsonOverride: ((body: unknown) => any) | null = null;

    mockResponse = {
      statusCode: 200,
      headersSent: false,
      setHeader: jest.fn(),
      on: jest.fn((event: string, cb: () => void) => {
        if (event === "finish") finishCb = cb;
      }),
      json: jest.fn(function (body: unknown) {
        if (jsonOverride) return jsonOverride(body);
        return this;
      }),
      _triggerFinish: () => finishCb?.(),
    };

    nextFn = jest.fn();
  });

  it("records a latency sample after res.finish fires", async () => {
    const { createRequestLogger } = await import(
      "../middleware/request-logger"
    );

    // Minimal no-op logger to avoid stdout noise
    const noopLogger = { info: jest.fn(), error: jest.fn() };
    const middleware = createRequestLogger(noopLogger, { skipHealthCheck: false });

    middleware(mockRequest, mockResponse, nextFn);
    expect(nextFn).toHaveBeenCalledTimes(1);

    // Before finish — no samples recorded yet
    expect(latencyTracker.routeCount).toBe(0);

    // Trigger finish
    mockResponse._triggerFinish();

    // After finish — sample should be recorded
    expect(latencyTracker.routeCount).toBe(1);
    const stats = latencyTracker.getStats();
    const route = stats.routes.find((r) => r.route === "/api/v1/invoices");
    expect(route).toBeDefined();
    expect(route!.count).toBe(1);
    expect(route!.p50).toBeGreaterThanOrEqual(0);
  });

  it("does NOT record a sample for /health when skipHealthCheck=true", async () => {
    const { createRequestLogger } = await import(
      "../middleware/request-logger"
    );

    const noopLogger = { info: jest.fn(), error: jest.fn() };
    const middleware = createRequestLogger(noopLogger, { skipHealthCheck: true });

    mockRequest.path = "/health";
    middleware(mockRequest, mockResponse, nextFn);
    mockResponse._triggerFinish();

    // Health check skipped — no routes recorded
    expect(latencyTracker.routeCount).toBe(0);
  });
});
