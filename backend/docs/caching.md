# HTTP Caching — QuickLendX Backend

This document describes the HTTP caching strategy for the QuickLendX backend
API, including the correctness policy that governs which endpoints may be
cached, the ETag / Last-Modified implementation, conditional-request handling,
and cache-poisoning mitigations.

---

## Table of contents

1. [Why caching matters here](#1-why-caching-matters-here)
2. [Correctness policy](#2-correctness-policy)
3. [Cache-Control directives per endpoint](#3-cache-control-directives-per-endpoint)
4. [ETag generation](#4-etag-generation)
5. [Last-Modified generation](#5-last-modified-generation)
6. [Conditional-request flow](#6-conditional-request-flow)
7. [Vary header](#7-vary-header)
8. [Cache-poisoning mitigations](#8-cache-poisoning-mitigations)
9. [Implementation reference](#9-implementation-reference)
10. [Testing](#10-testing)
11. [Future work](#11-future-work)

---

## 1. Why caching matters here

The QuickLendX backend is a read-heavy indexer layer that sits in front of
on-chain Soroban contract state.  Most reads are idempotent and the data
changes at a rate determined by on-chain ledger progression (~5 s per ledger).
Appropriate HTTP caching:

- Reduces redundant round-trips for clients polling invoice status.
- Protects the indexer from thundering-herd reads during high-traffic periods.
- Allows CDN / reverse-proxy layers to serve static settlement records without
  hitting the origin.

However, **not all data has the same staleness tolerance**.  The correctness
policy below defines which endpoints may be cached and for how long.

---

## 2. Correctness policy

### 2.1 Cacheable endpoints (short TTL)

**Invoices** — `GET /api/v1/invoices`, `GET /api/v1/invoices/:id`

Invoice status transitions (`Pending → Verified → Funded → Paid → Defaulted`)
are infrequent but must be visible within a short window.  A 10-second
`max-age` with a 30-second `stale-while-revalidate` window means:

- Clients see fresh data within 10 s of a status change in the common case.
- During the revalidation window (10–40 s) a stale response may be served
  while a background revalidation is in flight — acceptable for display
  purposes.
- After 40 s the cache must revalidate before serving.

### 2.2 Cacheable endpoints (long TTL)

**Settlements** — `GET /api/v1/settlements`, `GET /api/v1/settlements/:id`

A `Paid` settlement record is written once to the blockchain and never
mutated.  A 60-second `max-age` with a 120-second `stale-while-revalidate`
window is safe because:

- The underlying on-chain record is immutable once confirmed.
- The only state change is `Pending → Paid → Defaulted`, which happens at
  most once per settlement.

### 2.3 Non-cacheable endpoints (no-store)

**Bids** — `GET /api/v1/bids`

> **Correctness invariant**: a client must never act on a stale bid list.

The best-bid amount changes every time a new bid is placed or an existing bid
is withdrawn.  Serving a cached bid list could:

- Lead an investor to believe they hold the best bid when they do not.
- Cause a business to accept a bid that has since been withdrawn.
- Mislead the UI about the current funding progress of an invoice.

`Cache-Control: no-store` is mandatory.  The middleware also removes
`If-None-Match` and `If-Modified-Since` from the request before Express
processes the response, preventing any intermediate cache or the Express
freshness check from short-circuiting the response with a 304.

**Disputes** — `GET /api/v1/invoices/:id/disputes`

> **Correctness invariant**: dispute status has legal and compliance
> implications and must always reflect the current on-chain state.

A cached `UnderReview` response when the dispute is already `Resolved` could
cause incorrect UI decisions or compliance failures.  `Cache-Control: no-store`
is mandatory for the same reasons as bids.

---

## 3. Cache-Control directives per endpoint

| Endpoint | Directive | Constant |
|----------|-----------|----------|
| `GET /api/v1/invoices` | `public, max-age=10, stale-while-revalidate=30` | `CC_SHORT` |
| `GET /api/v1/invoices/:id` | `public, max-age=10, stale-while-revalidate=30` | `CC_SHORT` |
| `GET /api/v1/bids` | `no-store` | `CC_NO_STORE` |
| `GET /api/v1/settlements` | `public, max-age=60, stale-while-revalidate=120` | `CC_LONG` |
| `GET /api/v1/settlements/:id` | `public, max-age=60, stale-while-revalidate=120` | `CC_LONG` |
| `GET /api/v1/invoices/:id/disputes` | `no-store` | `CC_NO_STORE` |
| Error responses (4xx, 5xx) | *(no directive set by caching layer)* | — |

The constants are exported from `src/middleware/cache-headers.ts` so that
controllers import them by name rather than hard-coding strings.

---

## 4. ETag generation

ETags are computed as a **strong ETag** using SHA-1 of the JSON-serialised
response body:

```
ETag: "<sha1-hex-of-json-body>"
```

Properties:
- **Deterministic**: the same data always produces the same ETag.
- **Content-addressed**: any change to any field in the response changes the
  ETag, including nested metadata.
- **Cheap**: SHA-1 is fast for the payload sizes involved (~1–50 KB).
- **Not user-influenced**: the ETag is computed from the server-side data, not
  from any request header, preventing ETag injection.

ETags are **only set on cacheable responses** (`CC_SHORT`, `CC_LONG`).
`no-store` responses carry no ETag.

Express's built-in ETag generation is disabled (`app.set("etag", false)`) so
that the caching middleware has exclusive control over which responses receive
ETags.

---

## 5. Last-Modified generation

`Last-Modified` is derived from the most recent timestamp field found in the
response body.  The middleware inspects each record for the following fields
(in priority order):

1. `updated_at` — preferred; reflects the last mutation.
2. `timestamp` — used for records that carry a single event timestamp (bids,
   settlements).
3. `created_at` — fallback for records that are never mutated after creation.

For list responses the maximum timestamp across all records is used.

`Last-Modified` is only set when at least one timestamp field is found.  If no
timestamp is present the header is omitted (the ETag alone is sufficient for
conditional requests).

---

## 6. Conditional-request flow

```
Client                          Server
  |                               |
  |  GET /api/v1/invoices         |
  |------------------------------>|
  |                               |  200 OK
  |                               |  ETag: "abc123"
  |                               |  Last-Modified: Tue, 01 Apr 2026 12:00:00 GMT
  |<------------------------------|
  |                               |
  |  GET /api/v1/invoices         |
  |  If-None-Match: "abc123"      |
  |------------------------------>|
  |                               |  304 Not Modified
  |<------------------------------|
  |                               |
  |  GET /api/v1/invoices         |
  |  If-Modified-Since: Tue, ...  |
  |------------------------------>|
  |                               |  304 Not Modified (if unchanged)
  |<------------------------------|
```

### 304 conditions

A 304 is returned when **either** of the following is true:

1. `If-None-Match` is present and:
   - equals `*` (wildcard), **or**
   - contains the current ETag in a comma-separated list.

2. `If-Modified-Since` is present and the resource's `Last-Modified` date is
   ≤ the supplied date (i.e. the resource has not changed since the client's
   cached copy).

`If-None-Match` takes precedence over `If-Modified-Since` when both are
present (per RFC 7232 §6).

### 304 response body

A 304 response has an empty body.  The client uses its cached copy.

### No-store endpoints never return 304

For `no-store` endpoints (`/bids`, `/disputes`), the middleware removes
`If-None-Match` and `If-Modified-Since` from the request headers before
Express evaluates freshness.  This ensures that even a client sending
`If-None-Match: *` receives a full 200 response with fresh data.

---

## 7. Vary header

All responses (cacheable and no-store) include:

```
Vary: Accept-Encoding
```

This ensures that compressed (`gzip`, `br`) and uncompressed variants of the
same URL are stored as separate cache entries by shared caches (CDNs, proxies).
Without this header a CDN might serve a compressed response to a client that
does not support compression, or vice versa.

---

## 8. Cache-poisoning mitigations

| Threat | Mitigation |
|--------|-----------|
| ETag injection via request headers | ETags are computed from the server-side response body only; no request header influences the ETag value |
| Compressed/uncompressed cache confusion | `Vary: Accept-Encoding` on all responses |
| Stale bid data served from shared cache | `Cache-Control: no-store` prevents any intermediate cache from storing bid responses |
| Stale dispute data served from shared cache | `Cache-Control: no-store` on dispute responses |
| Client forcing 304 on no-store endpoint | `If-None-Match` and `If-Modified-Since` headers removed from request before Express freshness check |
| Express auto-ETag on no-store responses | `app.set("etag", false)` disables Express's built-in ETag generation globally |
| Cache serving error responses | No `Cache-Control` directive is set by the caching layer on 4xx/5xx responses; browsers default to `no-store` for error responses |

---

## 9. Implementation reference

All caching logic lives in a single file:

```
backend/src/middleware/cache-headers.ts
```

### Exported API

```typescript
// Cache-Control directive constants
export const CC_SHORT   = "public, max-age=10, stale-while-revalidate=30";
export const CC_LONG    = "public, max-age=60, stale-while-revalidate=120";
export const CC_NO_STORE = "no-store";

// Compute a strong ETag from a serialised body string
export function computeETag(body: string): string;

// Extract the most recent timestamp from a record or array of records
export function extractLastModified(data: unknown): Date | null;

// Evaluate conditional-request headers
export function isNotModified(
  req: Request,
  etag: string,
  lastModified: Date | null
): boolean;

// Apply all caching headers; returns true when the caller should send 304
export function applyCacheHeaders(
  req: Request,
  res: Response,
  options: { cacheControl: string; body: unknown }
): boolean;
```

### Controller usage pattern

```typescript
import { applyCacheHeaders, CC_SHORT } from "../../middleware/cache-headers";

export const getInvoices = async (req, res, next) => {
  try {
    const data = await fetchData();

    if (applyCacheHeaders(req, res, { cacheControl: CC_SHORT, body: data })) {
      res.status(304).end();   // conditional request matched — no body
      return;
    }

    res.json(data);            // full response with ETag + Last-Modified
  } catch (err) {
    next(err);
  }
};
```

For `no-store` endpoints, `applyCacheHeaders` always returns `false` so the
`if` branch is never taken:

```typescript
import { applyCacheHeaders, CC_NO_STORE } from "../../middleware/cache-headers";

export const getBids = async (req, res, next) => {
  try {
    const data = await fetchBids();
    applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body: data });
    res.json(data);            // always a full 200 response
  } catch (err) {
    next(err);
  }
};
```

---

## 10. Testing

Security and correctness regression tests live in:

```
backend/tests/caching.test.ts
```

Coverage:

| Test group | What is verified |
|------------|-----------------|
| Cache-Control policy | Each endpoint returns the correct directive |
| ETag presence and format | Cacheable endpoints have a quoted hex ETag; no-store endpoints do not |
| Last-Modified presence | Cacheable endpoints include a valid HTTP date |
| Vary header | All endpoints include `Vary: Accept-Encoding` |
| Conditional GET – If-None-Match | 304 on match; 200 on mismatch; 304 on `*`; 304 on list containing ETag |
| Conditional GET – If-Modified-Since | 304 when future date; 304 on exact match; 200 when past date; 200 on invalid date |
| Stale-content prevention | Bids and disputes return 200 regardless of `If-None-Match: *` or `If-Modified-Since` |
| No caching on errors | 404 responses do not carry a cacheable `Cache-Control` |
| Unit: `computeETag` | Determinism, format, empty string |
| Unit: `extractLastModified` | Field priority, array max, null cases |
| Unit: `isNotModified` | All conditional-header combinations |
| Unit: `applyCacheHeaders` | No-store header removal, ETag/LM set, 304 signal |

Run:

```bash
cd backend
npx jest tests/caching.test.ts --verbose
```

Expected: **68 tests, 0 failures**.

---

## 11. Future work

| Item | Notes |
|------|-------|
| Weak ETags for partial matches | When pagination is added, consider `W/"etag"` for list responses where only the page changes |
| `Cache-Control: private` for authenticated endpoints | When per-user invoice views are added, switch from `public` to `private` to prevent shared-cache storage of user-specific data |
| `s-maxage` for CDN differentiation | Add `s-maxage` to allow CDNs to cache longer than browsers |
| Redis-backed ETag store | For multi-instance deployments, ETags should be computed from the DB record's version/updated_at rather than the serialised body to avoid inconsistency across instances |
| `Surrogate-Control` / `CDN-Cache-Control` | For Fastly/Cloudflare, use vendor-specific headers to set CDN TTLs independently of browser TTLs |
