/**
 * Per-Route Latency Tracker
 *
 * Maintains a rolling window of request durations per route key and exposes
 * p50 / p95 / p99 percentiles for SLO reporting.
 *
 * Design decisions
 * ────────────────
 * • Pure in-process — no external APM dependency required.
 * • Ring-buffer per route: each bucket holds a fixed-size array of samples.
 *   When full, the oldest sample is overwritten (circular write pointer).
 *   This gives bounded memory regardless of route cardinality.
 * • Time-windowed snapshots: `getStats()` filters to samples recorded within
 *   the last `windowMs` milliseconds, so stale data naturally ages out.
 * • Route key normalisation: Express path params (`:id`, `:invoiceId`, etc.)
 *   and ULIDs / UUIDs / hex IDs embedded in paths are collapsed to `:id` so
 *   that high-cardinality paths don't create unbounded route key sets.
 * • Hard cap on distinct route keys (`MAX_ROUTES`): once the cap is reached,
 *   new unseen routes are recorded under the sentinel key "__overflow__".
 * • Concurrency: Node.js is single-threaded for synchronous JS; no locking is
 *   required. The `record()` and `getStats()` calls are synchronous and do not
 *   yield the event loop.
 *
 * Memory ceiling estimate
 * ───────────────────────
 * MAX_ROUTES (200) × BUCKET_SIZE (1024) × ~16 bytes per sample = ~3.3 MB
 * Plus the MAX_ROUTES × BUCKET_SIZE timestamp array = another ~3.3 MB
 * Total worst-case ≈ 7 MB — well within typical server budgets.
 */

// ── Constants ────────────────────────────────────────────────────────────────

/** Maximum number of distinct normalised route keys to track. */
export const MAX_ROUTES = 200;

/**
 * Samples stored per route (ring buffer size).
 * p99 accuracy requires at least ~100 samples; 1024 gives good statistical
 * resolution over a multi-minute window.
 */
export const BUCKET_SIZE = 1024;

/** Rolling window for percentile computation, in milliseconds. */
export const DEFAULT_WINDOW_MS = 5 * 60 * 1_000; // 5 minutes

/** Sentinel key used when MAX_ROUTES is exceeded. */
export const OVERFLOW_KEY = "__overflow__";

// ── Regex patterns for route normalisation ───────────────────────────────────

/** Matches ULID (26 uppercase base32 chars). */
const ULID_RE = /\b[0-9A-Z]{26}\b/gi;

/** Matches UUID v1–v5. */
const UUID_RE = /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi;

/** Matches long hex strings (≥ 8 hex chars, e.g. contract addresses). */
const HEX_RE = /\b[0-9a-f]{8,}\b/gi;

/** Matches bare integers that appear as path segments. */
const INT_SEGMENT_RE = /(?<=\/)\d+(?=\/|$)/g;

// ── Types ────────────────────────────────────────────────────────────────────

export interface PercentileStats {
  /** Normalised route key. */
  route: string;
  /** Number of samples in the current window. */
  count: number;
  /** p50 latency in milliseconds, or null if no samples. */
  p50: number | null;
  /** p95 latency in milliseconds, or null if no samples. */
  p95: number | null;
  /** p99 latency in milliseconds, or null if no samples. */
  p99: number | null;
  /** Minimum latency in current window, or null if no samples. */
  min: number | null;
  /** Maximum latency in current window, or null if no samples. */
  max: number | null;
  /** Width of the rolling window that was used, in milliseconds. */
  windowMs: number;
}

export interface TrackerStats {
  routes: PercentileStats[];
  windowMs: number;
  totalRoutes: number;
  maxRoutes: number;
  overflowed: boolean;
  /** ISO timestamp of when the stats were computed. */
  generatedAt: string;
}

// ── Internal data structures ─────────────────────────────────────────────────

interface RouteBucket {
  /** Circular buffer of duration samples (milliseconds). */
  durations: Float64Array;
  /** Circular buffer of recording timestamps (epoch ms). */
  timestamps: Float64Array;
  /** Next write position in the circular buffer. */
  writePos: number;
  /** True once the buffer has wrapped at least once. */
  full: boolean;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

function createBucket(): RouteBucket {
  return {
    durations: new Float64Array(BUCKET_SIZE),
    timestamps: new Float64Array(BUCKET_SIZE),
    writePos: 0,
    full: false,
  };
}

/**
 * Compute the requested percentile from a sorted array.
 * Uses the "nearest rank" method (same as the perf harness).
 */
function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

// ── Route key normalisation ──────────────────────────────────────────────────

/**
 * Normalise an Express `req.path` value into a stable route key.
 *
 * Rules applied (in order):
 * 1. Lowercase.
 * 2. Replace ULIDs with `:id`.
 * 3. Replace UUIDs with `:id`.
 * 4. Replace long hex strings with `:id`.
 * 5. Replace bare integer path segments with `:id`.
 * 6. Deduplicate consecutive `/:id/:id` into `/:id`.
 * 7. Trim trailing slash (except bare `/`).
 *
 * Examples:
 *   `/api/v1/invoices/01HXYZ123ABC456789012345` → `/api/v1/invoices/:id`
 *   `/api/v1/bids/550e8400-e29b-41d4-a716-446655440000` → `/api/v1/bids/:id`
 *   `/api/v1/admin/monitoring` → `/api/v1/admin/monitoring`
 */
export function normaliseRoute(path: string): string {
  let key = path.toLowerCase();

  // Reset lastIndex before each exec (global regex stateful)
  ULID_RE.lastIndex = 0;
  UUID_RE.lastIndex = 0;
  HEX_RE.lastIndex = 0;
  INT_SEGMENT_RE.lastIndex = 0;

  key = key.replace(ULID_RE, ":id");
  key = key.replace(UUID_RE, ":id");
  key = key.replace(HEX_RE, ":id");
  key = key.replace(INT_SEGMENT_RE, ":id");

  // Collapse repeated :id segments (e.g. /:id/:id → /:id)
  key = key.replace(/(\/:id)+/g, "/:id");

  // Remove trailing slash unless the whole path is "/"
  if (key.length > 1 && key.endsWith("/")) {
    key = key.slice(0, -1);
  }

  return key;
}

// ── Latency Tracker class ────────────────────────────────────────────────────

export class LatencyTracker {
  private buckets = new Map<string, RouteBucket>();
  private readonly windowMs: number;

  constructor(windowMs: number = DEFAULT_WINDOW_MS) {
    this.windowMs = windowMs;
  }

  /**
   * Record a single request duration for the given raw path.
   *
   * @param rawPath   The Express `req.path` value (not normalised yet).
   * @param durationMs  Wall-clock request duration in milliseconds.
   * @param nowMs     Override for `Date.now()` — useful in tests.
   */
  record(rawPath: string, durationMs: number, nowMs: number = Date.now()): void {
    const key = normaliseRoute(rawPath);
    this.recordNormalised(key, durationMs, nowMs);
  }

  /**
   * Record with a pre-normalised key. Used internally and in tests.
   */
  recordNormalised(
    key: string,
    durationMs: number,
    nowMs: number = Date.now()
  ): void {
    let bucket = this.buckets.get(key);

    if (!bucket) {
      // Enforce the route cap — new unseen keys go to the overflow bucket.
      // Bypass the cap check if we are already recording to the overflow key
      // itself to avoid infinite recursion when adding the overflow bucket.
      const hasOverflowKey = this.buckets.has(OVERFLOW_KEY);
      const cap = hasOverflowKey ? MAX_ROUTES : MAX_ROUTES - 1;

      if (key !== OVERFLOW_KEY && this.buckets.size >= cap) {
        this.recordNormalised(OVERFLOW_KEY, durationMs, nowMs);
        return;
      }
      bucket = createBucket();
      this.buckets.set(key, bucket);
    }

    const pos = bucket.writePos;
    bucket.durations[pos] = durationMs;
    bucket.timestamps[pos] = nowMs;
    bucket.writePos = (pos + 1) % BUCKET_SIZE;
    if (bucket.writePos === 0) {
      bucket.full = true;
    }
  }

  /**
   * Return percentile stats for all known routes, filtered to the rolling
   * window.  Routes with zero in-window samples are still listed so callers
   * can detect "quiet" routes.
   *
   * @param windowMs  Override for the window width (defaults to constructor value).
   * @param nowMs     Override for `Date.now()` — useful in tests.
   */
  getStats(
    windowMs: number = this.windowMs,
    nowMs: number = Date.now()
  ): TrackerStats {
    const cutoff = nowMs - windowMs;
    const routes: PercentileStats[] = [];

    for (const [route, bucket] of this.buckets) {
      const windowSamples: number[] = [];
      const capacity = bucket.full ? BUCKET_SIZE : bucket.writePos;

      for (let i = 0; i < capacity; i++) {
        if (bucket.timestamps[i] >= cutoff) {
          windowSamples.push(bucket.durations[i]);
        }
      }

      if (windowSamples.length === 0) {
        routes.push({
          route,
          count: 0,
          p50: null,
          p95: null,
          p99: null,
          min: null,
          max: null,
          windowMs,
        });
        continue;
      }

      windowSamples.sort((a, b) => a - b);

      routes.push({
        route,
        count: windowSamples.length,
        p50: percentile(windowSamples, 50),
        p95: percentile(windowSamples, 95),
        p99: percentile(windowSamples, 99),
        min: windowSamples[0],
        max: windowSamples[windowSamples.length - 1],
        windowMs,
      });
    }

    // Sort by route name for stable output
    routes.sort((a, b) => a.route.localeCompare(b.route));

    return {
      routes,
      windowMs,
      totalRoutes: this.buckets.size,
      maxRoutes: MAX_ROUTES,
      overflowed: this.buckets.has(OVERFLOW_KEY),
      generatedAt: new Date(nowMs).toISOString(),
    };
  }

  /**
   * Return stats for a single route (by its raw or normalised path).
   * Returns `null` if the route has never been recorded.
   */
  getRouteStats(
    rawPath: string,
    windowMs: number = this.windowMs,
    nowMs: number = Date.now()
  ): PercentileStats | null {
    const key = normaliseRoute(rawPath);
    if (!this.buckets.has(key)) return null;

    const all = this.getStats(windowMs, nowMs);
    return all.routes.find((r) => r.route === key) ?? null;
  }

  /** Reset all recorded data. Useful in tests or for manual resets. */
  reset(): void {
    this.buckets.clear();
  }

  /** Number of distinct route keys currently tracked. */
  get routeCount(): number {
    return this.buckets.size;
  }
}

// ── Singleton ────────────────────────────────────────────────────────────────

/**
 * Process-wide latency tracker singleton.
 * The middleware and admin endpoint both reference this instance.
 */
export const latencyTracker = new LatencyTracker(DEFAULT_WINDOW_MS);
