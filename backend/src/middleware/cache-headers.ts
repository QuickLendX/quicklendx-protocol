/**
 * HTTP caching middleware for QuickLendX backend.
 *
 * Provides:
 *   - ETag generation (SHA-1 of the serialised response body)
 *   - Last-Modified header derived from the newest record in the payload
 *   - Cache-Control policies per endpoint category
 *   - Conditional-request handling (If-None-Match / If-Modified-Since → 304)
 *
 * Correctness policy
 * ------------------
 * Different endpoints have different staleness tolerances:
 *
 *   CACHEABLE_SHORT  (invoices list / single invoice)
 *     Cache-Control: public, max-age=10, stale-while-revalidate=30
 *     Rationale: invoice status (Pending → Verified → Funded → Paid) changes
 *     infrequently but must not be stale for more than ~10 s so that a newly
 *     funded invoice is visible quickly.
 *
 *   CACHEABLE_LONG   (settlements list / single settlement)
 *     Cache-Control: public, max-age=60, stale-while-revalidate=120
 *     Rationale: a Paid settlement record is immutable on-chain; 60 s is safe.
 *
 *   NO_STORE         (bids list, disputes)
 *     Cache-Control: no-store
 *     Rationale:
 *       - Bids: the best-bid amount changes every time a new bid is placed.
 *         Serving a stale bid list could mislead an investor into placing a
 *         sub-optimal bid or believing they are the best bidder when they are
 *         not.  Freshness is critical for financial correctness.
 *       - Disputes: dispute status has legal / compliance implications.
 *         A cached "UnderReview" response when the dispute is already
 *         "Resolved" could cause incorrect UI decisions.
 *
 * ETag / conditional-request flow
 * --------------------------------
 *   1. Handler builds the response body.
 *   2. `applyCacheHeaders` computes ETag = `"<sha1-hex>"`.
 *   3. If the request carries `If-None-Match` matching the ETag → 304.
 *   4. If the request carries `If-Modified-Since` and the resource has not
 *      changed since that date → 304.
 *   5. Otherwise the full 200 response is sent with ETag + Last-Modified.
 *
 * Cache-poisoning mitigations
 * ---------------------------
 *   - ETags are computed server-side from the actual response body; they
 *     cannot be influenced by request headers.
 *   - `Vary: Accept-Encoding` is set so that compressed and uncompressed
 *     variants are stored separately by shared caches.
 *   - No-store responses carry `Cache-Control: no-store` which prevents
 *     any intermediate cache from storing the response.
 *   - ETags are never derived from user-supplied input.
 */

import { createHash } from "crypto";
import { Request, Response } from "express";

// ---------------------------------------------------------------------------
// Cache-Control policy constants
// ---------------------------------------------------------------------------

/** Short-lived public cache: invoice lists and single invoices. */
export const CC_SHORT = "public, max-age=10, stale-while-revalidate=30";

/** Long-lived public cache: settlement records (immutable once Paid). */
export const CC_LONG = "public, max-age=60, stale-while-revalidate=120";

/**
 * No caching: bids (best-bid freshness) and disputes (legal sensitivity).
 * Also used for all error responses.
 */
export const CC_NO_STORE = "no-store";

// ---------------------------------------------------------------------------
// ETag helpers
// ---------------------------------------------------------------------------

/**
 * Computes a strong ETag for a serialised response body.
 * Format: `"<sha1-hex>"` (quoted per RFC 7232 §2.3).
 */
export function computeETag(body: string): string {
  const hash = createHash("sha1").update(body).digest("hex");
  return `"${hash}"`;
}

/**
 * Extracts the most recent timestamp (seconds since epoch) from an array of
 * records that may carry `updated_at`, `timestamp`, or `created_at` fields.
 * Returns `null` when no timestamp can be found.
 */
export function extractLastModified(
  data: unknown
): Date | null {
  const records = Array.isArray(data) ? data : [data];
  let maxTs = 0;

  for (const record of records) {
    if (record === null || typeof record !== "object") continue;
    const r = record as Record<string, unknown>;
    for (const field of ["updated_at", "timestamp", "created_at"] as const) {
      const v = r[field];
      if (typeof v === "number" && v > maxTs) {
        maxTs = v;
      }
    }
  }

  return maxTs > 0 ? new Date(maxTs * 1000) : null;
}

// ---------------------------------------------------------------------------
// Conditional-request evaluation
// ---------------------------------------------------------------------------

/**
 * Returns `true` when the response can be short-circuited with 304.
 *
 * Checks (in order):
 *   1. `If-None-Match` — compared against the computed ETag.
 *   2. `If-Modified-Since` — compared against `lastModified` when present.
 */
export function isNotModified(
  req: Request,
  etag: string,
  lastModified: Date | null
): boolean {
  const ifNoneMatch = req.headers["if-none-match"];
  if (ifNoneMatch) {
    // Support comma-separated list of ETags and the wildcard "*".
    const tags = ifNoneMatch.split(",").map((t) => t.trim());
    if (tags.includes("*") || tags.includes(etag)) {
      return true;
    }
  }

  const ifModifiedSince = req.headers["if-modified-since"];
  if (ifModifiedSince && lastModified) {
    const since = new Date(ifModifiedSince);
    if (!isNaN(since.getTime()) && lastModified <= since) {
      return true;
    }
  }

  return false;
}

// ---------------------------------------------------------------------------
// Main helper: apply cache headers and handle conditional requests
// ---------------------------------------------------------------------------

export interface CacheOptions {
  /** Cache-Control directive string (use the CC_* constants). */
  cacheControl: string;
  /** The response body that will be sent (used to compute ETag). */
  body: unknown;
}

/**
 * Applies caching headers to `res` and returns `true` when the caller
 * should send a 304 Not Modified response instead of the full body.
 *
 * Usage in a controller:
 * ```ts
 * const body = buildResponseData();
 * if (applyCacheHeaders(req, res, { cacheControl: CC_SHORT, body })) {
 *   res.status(304).end();
 *   return;
 * }
 * res.json(body);
 * ```
 */
export function applyCacheHeaders(
  req: Request,
  res: Response,
  options: CacheOptions
): boolean {
  const { cacheControl, body } = options;

  res.setHeader("Cache-Control", cacheControl);
  res.setHeader("Vary", "Accept-Encoding");

  // No-store responses skip ETag / Last-Modified — there is nothing to
  // revalidate because the client must always fetch fresh data.
  // We also remove conditional-request headers from the request object so
  // that Express's built-in freshness check (req.fresh) does not
  // short-circuit the response with a 304.  Serving a 304 on a no-store
  // endpoint would allow a client to use a cached copy of bid or dispute
  // data, which violates the correctness policy for those endpoints.
  if (cacheControl === CC_NO_STORE) {
    delete (req.headers as Record<string, unknown>)["if-none-match"];
    delete (req.headers as Record<string, unknown>)["if-modified-since"];
    return false;
  }

  const serialised = JSON.stringify(body);
  const etag = computeETag(serialised);
  const lastModified = extractLastModified(body);

  res.setHeader("ETag", etag);
  if (lastModified) {
    res.setHeader("Last-Modified", lastModified.toUTCString());
  }

  return isNotModified(req, etag, lastModified);
}
