/**
 * HTTP caching tests for QuickLendX backend.
 *
 * Covers:
 *   1. Cache-Control policy per endpoint (correct directive, no leakage)
 *   2. ETag presence and format on cacheable responses
 *   3. Last-Modified presence on cacheable responses
 *   4. Vary header on all responses
 *   5. Conditional GET with If-None-Match → 304 Not Modified
 *   6. Conditional GET with If-Modified-Since → 304 Not Modified
 *   7. Stale-content prevention: no-store on bids and disputes
 *   8. No caching headers on error responses (404)
 *   9. Unit tests for cache-headers helpers
 */

import { describe, it, expect } from "@jest/globals";
import request from "supertest";
import app from "../src/app";
import {
  computeETag,
  extractLastModified,
  isNotModified,
  applyCacheHeaders,
  CC_SHORT,
  CC_LONG,
  CC_NO_STORE,
} from "../src/middleware/cache-headers";
import { createRequest, createResponse } from "node-mocks-http";

// ---------------------------------------------------------------------------
// Helper: assert a header matches a pattern
// ---------------------------------------------------------------------------
function expectHeader(
  headers: Record<string, string | string[]>,
  name: string,
  expected: string | RegExp
) {
  const value = headers[name.toLowerCase()];
  expect(value).toBeDefined();
  if (typeof expected === "string") {
    expect(value).toBe(expected);
  } else {
    expect(String(value)).toMatch(expected);
  }
}

// ---------------------------------------------------------------------------
// 1. Cache-Control policy per endpoint
// ---------------------------------------------------------------------------
describe("Cache-Control policy", () => {
  it("GET /api/v1/invoices returns CC_SHORT", async () => {
    const res = await request(app).get("/api/v1/invoices");
    expect(res.status).toBe(200);
    expectHeader(res.headers, "cache-control", CC_SHORT);
  });

  it("GET /api/v1/invoices/:id returns CC_SHORT", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/invoices/${id}`);
    expect(res.status).toBe(200);
    expectHeader(res.headers, "cache-control", CC_SHORT);
  });

  it("GET /api/v1/bids returns no-store (freshness critical)", async () => {
    const res = await request(app).get("/api/v1/bids");
    expect(res.status).toBe(200);
    expectHeader(res.headers, "cache-control", CC_NO_STORE);
  });

  it("GET /api/v1/settlements returns CC_LONG", async () => {
    const res = await request(app).get("/api/v1/settlements");
    expect(res.status).toBe(200);
    expectHeader(res.headers, "cache-control", CC_LONG);
  });

  it("GET /api/v1/settlements/:id returns CC_LONG", async () => {
    const res = await request(app).get("/api/v1/settlements/0xsettle123");
    expect(res.status).toBe(200);
    expectHeader(res.headers, "cache-control", CC_LONG);
  });

  it("GET /api/v1/invoices/:id/disputes returns no-store (legal sensitivity)", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/invoices/${id}/disputes`);
    expect(res.status).toBe(200);
    expectHeader(res.headers, "cache-control", CC_NO_STORE);
  });
});

// ---------------------------------------------------------------------------
// 2. ETag presence and format
// ---------------------------------------------------------------------------
describe("ETag header", () => {
  it("GET /api/v1/invoices includes a quoted ETag", async () => {
    const res = await request(app).get("/api/v1/invoices");
    expect(res.status).toBe(200);
    const etag = res.headers["etag"];
    expect(etag).toBeDefined();
    // Strong ETag: starts and ends with a double-quote
    expect(etag).toMatch(/^"[0-9a-f]+"$/);
  });

  it("GET /api/v1/invoices/:id includes a quoted ETag", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/invoices/${id}`);
    expect(res.status).toBe(200);
    expect(res.headers["etag"]).toMatch(/^"[0-9a-f]+"$/);
  });

  it("GET /api/v1/settlements includes a quoted ETag", async () => {
    const res = await request(app).get("/api/v1/settlements");
    expect(res.status).toBe(200);
    expect(res.headers["etag"]).toMatch(/^"[0-9a-f]+"$/);
  });

  it("GET /api/v1/settlements/:id includes a quoted ETag", async () => {
    const res = await request(app).get("/api/v1/settlements/0xsettle123");
    expect(res.status).toBe(200);
    expect(res.headers["etag"]).toMatch(/^"[0-9a-f]+"$/);
  });

  it("GET /api/v1/bids does NOT include an ETag (no-store)", async () => {
    const res = await request(app).get("/api/v1/bids");
    expect(res.status).toBe(200);
    expect(res.headers["etag"]).toBeUndefined();
  });

  it("GET /api/v1/invoices/:id/disputes does NOT include an ETag (no-store)", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/invoices/${id}/disputes`);
    expect(res.status).toBe(200);
    expect(res.headers["etag"]).toBeUndefined();
  });

  it("two identical requests produce the same ETag", async () => {
    const res1 = await request(app).get("/api/v1/invoices");
    const res2 = await request(app).get("/api/v1/invoices");
    expect(res1.headers["etag"]).toBe(res2.headers["etag"]);
  });
});

// ---------------------------------------------------------------------------
// 3. Last-Modified header
// ---------------------------------------------------------------------------
describe("Last-Modified header", () => {
  it("GET /api/v1/invoices includes Last-Modified", async () => {
    const res = await request(app).get("/api/v1/invoices");
    expect(res.status).toBe(200);
    const lm = res.headers["last-modified"];
    expect(lm).toBeDefined();
    expect(new Date(lm as string).getTime()).not.toBeNaN();
  });

  it("GET /api/v1/invoices/:id includes Last-Modified", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/invoices/${id}`);
    expect(res.status).toBe(200);
    expect(new Date(res.headers["last-modified"] as string).getTime()).not.toBeNaN();
  });

  it("GET /api/v1/settlements includes Last-Modified", async () => {
    const res = await request(app).get("/api/v1/settlements");
    expect(res.status).toBe(200);
    expect(new Date(res.headers["last-modified"] as string).getTime()).not.toBeNaN();
  });

  it("GET /api/v1/bids does NOT include Last-Modified (no-store)", async () => {
    const res = await request(app).get("/api/v1/bids");
    expect(res.status).toBe(200);
    expect(res.headers["last-modified"]).toBeUndefined();
  });
});

// ---------------------------------------------------------------------------
// 4. Vary header
// ---------------------------------------------------------------------------
describe("Vary header", () => {
  const cacheableEndpoints = [
    "/api/v1/invoices",
    "/api/v1/invoices/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    "/api/v1/settlements",
    "/api/v1/settlements/0xsettle123",
  ];

  const noStoreEndpoints = [
    "/api/v1/bids",
    "/api/v1/invoices/0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef/disputes",
  ];

  for (const endpoint of cacheableEndpoints) {
    it(`${endpoint} includes Vary: Accept-Encoding`, async () => {
      const res = await request(app).get(endpoint);
      expect(res.status).toBe(200);
      expectHeader(res.headers, "vary", "Accept-Encoding");
    });
  }

  for (const endpoint of noStoreEndpoints) {
    it(`${endpoint} includes Vary: Accept-Encoding (even no-store)`, async () => {
      const res = await request(app).get(endpoint);
      expect(res.status).toBe(200);
      expectHeader(res.headers, "vary", "Accept-Encoding");
    });
  }
});

// ---------------------------------------------------------------------------
// 5. Conditional GET: If-None-Match → 304
// ---------------------------------------------------------------------------
describe("Conditional GET – If-None-Match", () => {
  it("returns 304 when If-None-Match matches the current ETag (invoices list)", async () => {
    const first = await request(app).get("/api/v1/invoices");
    const etag = first.headers["etag"] as string;
    expect(etag).toBeDefined();

    const second = await request(app)
      .get("/api/v1/invoices")
      .set("If-None-Match", etag);
    expect(second.status).toBe(304);
    expect(second.text).toBe("");
  });

  it("returns 304 when If-None-Match matches the current ETag (invoice by id)", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const first = await request(app).get(`/api/v1/invoices/${id}`);
    const etag = first.headers["etag"] as string;

    const second = await request(app)
      .get(`/api/v1/invoices/${id}`)
      .set("If-None-Match", etag);
    expect(second.status).toBe(304);
  });

  it("returns 304 when If-None-Match matches the current ETag (settlements list)", async () => {
    const first = await request(app).get("/api/v1/settlements");
    const etag = first.headers["etag"] as string;

    const second = await request(app)
      .get("/api/v1/settlements")
      .set("If-None-Match", etag);
    expect(second.status).toBe(304);
  });

  it("returns 304 when If-None-Match matches the current ETag (settlement by id)", async () => {
    const first = await request(app).get("/api/v1/settlements/0xsettle123");
    const etag = first.headers["etag"] as string;

    const second = await request(app)
      .get("/api/v1/settlements/0xsettle123")
      .set("If-None-Match", etag);
    expect(second.status).toBe(304);
  });

  it("returns 200 when If-None-Match does not match (stale client ETag)", async () => {
    const res = await request(app)
      .get("/api/v1/invoices")
      .set("If-None-Match", '"stale-etag-that-does-not-match"');
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it("returns 304 when If-None-Match is the wildcard *", async () => {
    const res = await request(app)
      .get("/api/v1/invoices")
      .set("If-None-Match", "*");
    expect(res.status).toBe(304);
  });

  it("returns 304 when If-None-Match is a comma-separated list containing the ETag", async () => {
    const first = await request(app).get("/api/v1/invoices");
    const etag = first.headers["etag"] as string;

    const second = await request(app)
      .get("/api/v1/invoices")
      .set("If-None-Match", `"other-etag", ${etag}`);
    expect(second.status).toBe(304);
  });
});

// ---------------------------------------------------------------------------
// 6. Conditional GET: If-Modified-Since → 304
// ---------------------------------------------------------------------------
describe("Conditional GET – If-Modified-Since", () => {
  it("returns 304 when If-Modified-Since is after Last-Modified (invoices)", async () => {
    const first = await request(app).get("/api/v1/invoices");
    const lm = first.headers["last-modified"] as string;
    expect(lm).toBeDefined();

    // Use a date well in the future so the resource appears unmodified.
    const future = new Date(Date.now() + 86400 * 1000).toUTCString();
    const second = await request(app)
      .get("/api/v1/invoices")
      .set("If-Modified-Since", future);
    expect(second.status).toBe(304);
  });

  it("returns 304 when If-Modified-Since equals Last-Modified (invoices)", async () => {
    const first = await request(app).get("/api/v1/invoices");
    const lm = first.headers["last-modified"] as string;

    const second = await request(app)
      .get("/api/v1/invoices")
      .set("If-Modified-Since", lm);
    expect(second.status).toBe(304);
  });

  it("returns 200 when If-Modified-Since is before Last-Modified (invoices)", async () => {
    // Use a date well in the past so the resource appears modified.
    const past = new Date(0).toUTCString(); // 1970-01-01
    const res = await request(app)
      .get("/api/v1/invoices")
      .set("If-Modified-Since", past);
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it("returns 200 when If-Modified-Since is an invalid date string", async () => {
    const res = await request(app)
      .get("/api/v1/invoices")
      .set("If-Modified-Since", "not-a-date");
    expect(res.status).toBe(200);
  });

  it("returns 304 for settlements with If-Modified-Since in the future", async () => {
    const future = new Date(Date.now() + 86400 * 1000).toUTCString();
    const res = await request(app)
      .get("/api/v1/settlements")
      .set("If-Modified-Since", future);
    expect(res.status).toBe(304);
  });
});

// ---------------------------------------------------------------------------
// 7. Stale-content prevention: no-store endpoints never return 304
// ---------------------------------------------------------------------------
describe("Stale-content prevention (no-store endpoints)", () => {
  it("GET /api/v1/bids always returns 200 regardless of If-None-Match", async () => {
    // Even if the client sends a matching ETag, bids must always be fresh.
    const res = await request(app)
      .get("/api/v1/bids")
      .set("If-None-Match", "*");
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it("GET /api/v1/bids always returns 200 regardless of If-Modified-Since", async () => {
    const future = new Date(Date.now() + 86400 * 1000).toUTCString();
    const res = await request(app)
      .get("/api/v1/bids")
      .set("If-Modified-Since", future);
    expect(res.status).toBe(200);
  });

  it("GET /api/v1/invoices/:id/disputes always returns 200 regardless of If-None-Match", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app)
      .get(`/api/v1/invoices/${id}/disputes`)
      .set("If-None-Match", "*");
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
  });

  it("GET /api/v1/invoices/:id/disputes always returns 200 regardless of If-Modified-Since", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const future = new Date(Date.now() + 86400 * 1000).toUTCString();
    const res = await request(app)
      .get(`/api/v1/invoices/${id}/disputes`)
      .set("If-Modified-Since", future);
    expect(res.status).toBe(200);
  });
});

// ---------------------------------------------------------------------------
// 8. No caching on error responses
// ---------------------------------------------------------------------------
describe("No caching on error responses", () => {
  it("404 for unknown invoice does not set a cacheable Cache-Control", async () => {
    const res = await request(app).get("/api/v1/invoices/nonexistent");
    expect(res.status).toBe(404);
    const cc = res.headers["cache-control"] as string | undefined;
    // Must not be a cacheable directive
    if (cc) {
      expect(cc).not.toMatch(/max-age=(?!0)\d+/);
      expect(cc).not.toContain("public");
    }
  });

  it("404 for unknown settlement does not set a cacheable Cache-Control", async () => {
    const res = await request(app).get("/api/v1/settlements/nonexistent");
    expect(res.status).toBe(404);
    const cc = res.headers["cache-control"] as string | undefined;
    if (cc) {
      expect(cc).not.toMatch(/max-age=(?!0)\d+/);
      expect(cc).not.toContain("public");
    }
  });
});

// ---------------------------------------------------------------------------
// 9. Unit tests for cache-headers helpers
// ---------------------------------------------------------------------------
describe("cache-headers unit tests", () => {
  describe("computeETag", () => {
    it("returns a quoted hex string", () => {
      const etag = computeETag("hello");
      expect(etag).toMatch(/^"[0-9a-f]+"$/);
    });

    it("is deterministic for the same input", () => {
      expect(computeETag("abc")).toBe(computeETag("abc"));
    });

    it("differs for different inputs", () => {
      expect(computeETag("abc")).not.toBe(computeETag("xyz"));
    });

    it("handles empty string", () => {
      const etag = computeETag("");
      expect(etag).toMatch(/^"[0-9a-f]+"$/);
    });
  });

  describe("extractLastModified", () => {
    it("extracts updated_at from a single object", () => {
      const ts = Math.floor(Date.now() / 1000) - 100;
      const result = extractLastModified({ updated_at: ts });
      expect(result).toEqual(new Date(ts * 1000));
    });

    it("extracts timestamp from a single object", () => {
      const ts = Math.floor(Date.now() / 1000) - 200;
      const result = extractLastModified({ timestamp: ts });
      expect(result).toEqual(new Date(ts * 1000));
    });

    it("extracts created_at from a single object", () => {
      const ts = Math.floor(Date.now() / 1000) - 300;
      const result = extractLastModified({ created_at: ts });
      expect(result).toEqual(new Date(ts * 1000));
    });

    it("returns the most recent timestamp from an array", () => {
      const older = Math.floor(Date.now() / 1000) - 500;
      const newer = Math.floor(Date.now() / 1000) - 100;
      const result = extractLastModified([
        { updated_at: older },
        { updated_at: newer },
      ]);
      expect(result).toEqual(new Date(newer * 1000));
    });

    it("prefers updated_at over timestamp over created_at", () => {
      const ts = Math.floor(Date.now() / 1000);
      const result = extractLastModified({
        updated_at: ts,
        timestamp: ts - 10,
        created_at: ts - 20,
      });
      expect(result).toEqual(new Date(ts * 1000));
    });

    it("returns null when no timestamp fields are present", () => {
      expect(extractLastModified({ id: "abc" })).toBeNull();
    });

    it("returns null for an empty array", () => {
      expect(extractLastModified([])).toBeNull();
    });

    it("returns null for null input", () => {
      expect(extractLastModified(null)).toBeNull();
    });

    it("returns null for a primitive", () => {
      expect(extractLastModified("string")).toBeNull();
    });

    it("ignores non-numeric timestamp values", () => {
      expect(extractLastModified({ updated_at: "not-a-number" })).toBeNull();
    });
  });

  describe("isNotModified", () => {
    const makeReq = (headers: Record<string, string>) =>
      createRequest({ headers });

    it("returns true when If-None-Match matches the ETag", () => {
      const req = makeReq({ "if-none-match": '"abc123"' });
      expect(isNotModified(req as any, '"abc123"', null)).toBe(true);
    });

    it("returns false when If-None-Match does not match", () => {
      const req = makeReq({ "if-none-match": '"other"' });
      expect(isNotModified(req as any, '"abc123"', null)).toBe(false);
    });

    it("returns true for wildcard If-None-Match", () => {
      const req = makeReq({ "if-none-match": "*" });
      expect(isNotModified(req as any, '"abc123"', null)).toBe(true);
    });

    it("returns true when If-Modified-Since is after lastModified", () => {
      const req = makeReq({
        "if-modified-since": new Date(Date.now() + 10000).toUTCString(),
      });
      const lm = new Date(Date.now() - 10000);
      expect(isNotModified(req as any, '"x"', lm)).toBe(true);
    });

    it("returns false when If-Modified-Since is before lastModified", () => {
      const req = makeReq({
        "if-modified-since": new Date(Date.now() - 10000).toUTCString(),
      });
      const lm = new Date(Date.now());
      expect(isNotModified(req as any, '"x"', lm)).toBe(false);
    });

    it("returns false when no conditional headers are present", () => {
      const req = makeReq({});
      expect(isNotModified(req as any, '"abc"', new Date())).toBe(false);
    });

    it("returns false when If-Modified-Since is an invalid date", () => {
      const req = makeReq({ "if-modified-since": "not-a-date" });
      expect(isNotModified(req as any, '"x"', new Date())).toBe(false);
    });
  });

  describe("applyCacheHeaders", () => {
    it("sets Cache-Control and Vary on a no-store response and returns false", () => {
      const req = createRequest();
      const res = createResponse();
      const result = applyCacheHeaders(req as any, res as any, {
        cacheControl: CC_NO_STORE,
        body: [],
      });
      expect(result).toBe(false);
      expect(res.getHeader("Cache-Control")).toBe(CC_NO_STORE);
      expect(res.getHeader("Vary")).toBe("Accept-Encoding");
      expect(res.getHeader("ETag")).toBeUndefined();
    });

    it("removes If-None-Match from request headers for no-store responses", () => {
      const req = createRequest({ headers: { "if-none-match": "*" } });
      const res = createResponse();
      applyCacheHeaders(req as any, res as any, {
        cacheControl: CC_NO_STORE,
        body: [],
      });
      expect((req.headers as any)["if-none-match"]).toBeUndefined();
    });

    it("removes If-Modified-Since from request headers for no-store responses", () => {
      const future = new Date(Date.now() + 86400 * 1000).toUTCString();
      const req = createRequest({ headers: { "if-modified-since": future } });
      const res = createResponse();
      applyCacheHeaders(req as any, res as any, {
        cacheControl: CC_NO_STORE,
        body: [],
      });
      expect((req.headers as any)["if-modified-since"]).toBeUndefined();
    });

    it("sets ETag, Last-Modified, Cache-Control, Vary on a cacheable response", () => {
      const req = createRequest();
      const res = createResponse();
      const ts = Math.floor(Date.now() / 1000) - 100;
      const body = [{ updated_at: ts }];
      const result = applyCacheHeaders(req as any, res as any, {
        cacheControl: CC_SHORT,
        body,
      });
      expect(result).toBe(false); // no conditional headers on req
      expect(res.getHeader("Cache-Control")).toBe(CC_SHORT);
      expect(res.getHeader("ETag")).toMatch(/^"[0-9a-f]+"$/);
      expect(res.getHeader("Last-Modified")).toBeDefined();
      expect(res.getHeader("Vary")).toBe("Accept-Encoding");
    });

    it("returns true (304 signal) when If-None-Match matches", () => {
      // First, compute the ETag for the body.
      const body = [{ id: "x", updated_at: 1000 }];
      const etag = computeETag(JSON.stringify(body));

      const req = createRequest({ headers: { "if-none-match": etag } });
      const res = createResponse();
      const result = applyCacheHeaders(req as any, res as any, {
        cacheControl: CC_SHORT,
        body,
      });
      expect(result).toBe(true);
    });

    it("does not set Last-Modified when body has no timestamp fields", () => {
      const req = createRequest();
      const res = createResponse();
      applyCacheHeaders(req as any, res as any, {
        cacheControl: CC_LONG,
        body: { id: "no-timestamps" },
      });
      expect(res.getHeader("Last-Modified")).toBeUndefined();
    });
  });
});
