import { describe, it, expect } from "@jest/globals";
import { Request } from "express";
import { computeETag, isNotModified } from "../middleware/cache-headers";

// ---------------------------------------------------------------------------
// Helpers
//
// `isNotModified` only reads `req.headers`, so a plain object with a headers
// map is enough. Keeping the fake minimal makes the intent of each case obvious
// and keeps the suite independent of the Express app, the database, and the
// clock.
// ---------------------------------------------------------------------------

function mockReq(headers: Record<string, string> = {}): Request {
  return { headers } as unknown as Request;
}

const BODY_ETAG = computeETag(JSON.stringify({ id: "invoice-1", status: "Pending" }));
const OTHER_ETAG = computeETag(JSON.stringify({ id: "invoice-1", status: "Paid" }));
const LAST_MODIFIED = new Date("2026-06-15T12:00:00Z");
const BEFORE_LAST_MODIFIED = new Date("2026-01-01T00:00:00Z").toUTCString();
const AFTER_LAST_MODIFIED = new Date("2027-01-01T00:00:00Z").toUTCString();

// ---------------------------------------------------------------------------
// No conditional headers
// ---------------------------------------------------------------------------

describe("isNotModified — no conditional headers", () => {
  it("returns false when neither If-None-Match nor If-Modified-Since is present", () => {
    expect(isNotModified(mockReq(), BODY_ETAG, LAST_MODIFIED)).toBe(false);
  });

  it("returns false when only If-Modified-Since is present but the resource has no Last-Modified", () => {
    const req = mockReq({ "if-modified-since": AFTER_LAST_MODIFIED });
    expect(isNotModified(req, BODY_ETAG, null)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// If-None-Match
// ---------------------------------------------------------------------------

describe("isNotModified — If-None-Match", () => {
  it("returns true on an exact ETag match", () => {
    const req = mockReq({ "if-none-match": BODY_ETAG });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(true);
  });

  it("returns false on an ETag mismatch", () => {
    const req = mockReq({ "if-none-match": OTHER_ETAG });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(false);
  });

  it("returns true for the wildcard validator *", () => {
    const req = mockReq({ "if-none-match": "*" });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(true);
  });

  it("returns true when the matching tag is one entry of a comma-separated list", () => {
    const req = mockReq({ "if-none-match": `"stale", ${BODY_ETAG}, "unrelated"` });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(true);
  });

  it("returns false when none of the comma-separated tags match", () => {
    const req = mockReq({ "if-none-match": '"stale", "unrelated"' });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(false);
  });

  it("strips optional whitespace around each list entry before comparing", () => {
    const req = mockReq({ "if-none-match": `   "stale" ,\t${BODY_ETAG}  ,  "other" ` });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(true);
  });

  it("treats a weak validator as distinct from its strong form", () => {
    // `W/"..."` and `"..."` are different validators; the implementation does a
    // literal string comparison rather than RFC 7232 weak comparison.
    const req = mockReq({ "if-none-match": `W/${BODY_ETAG}` });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(false);
  });

  it("is authoritative over If-Modified-Since when the ETag does not match", () => {
    // RFC 7232 §3.3: a recipient MUST ignore If-Modified-Since when the request
    // contains If-None-Match. The ETag mismatch decides the response even
    // though the client's If-Modified-Since date is newer than the resource.
    const req = mockReq({
      "if-none-match": OTHER_ETAG,
      "if-modified-since": AFTER_LAST_MODIFIED,
    });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(false);
  });

  it("still honours If-Modified-Since when If-None-Match is an empty string", () => {
    // An empty header carries no validator, so the older precondition applies.
    const req = mockReq({
      "if-none-match": "",
      "if-modified-since": AFTER_LAST_MODIFIED,
    });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// If-Modified-Since
// ---------------------------------------------------------------------------

describe("isNotModified — If-Modified-Since", () => {
  it("returns true when the resource was last modified before the client's copy", () => {
    const req = mockReq({ "if-modified-since": AFTER_LAST_MODIFIED });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(true);
  });

  it("returns false when the resource was modified after the client's copy", () => {
    const req = mockReq({ "if-modified-since": BEFORE_LAST_MODIFIED });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(false);
  });

  it("returns true on the exact-second boundary", () => {
    const req = mockReq({ "if-modified-since": LAST_MODIFIED.toUTCString() });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(true);
  });

  it("ignores an unparseable date and reports the response as modified", () => {
    const req = mockReq({ "if-modified-since": "not-a-date" });
    expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(false);
  });

  it("ignores If-Modified-Since when the resource has no Last-Modified value", () => {
    const req = mockReq({ "if-modified-since": AFTER_LAST_MODIFIED });
    expect(isNotModified(req, BODY_ETAG, null)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Determinism / regression
//
// `applyCacheHeaders` derives the ETag from the serialised body, so the same
// body must always produce the same validator and therefore the same decision.
// ---------------------------------------------------------------------------

describe("isNotModified — determinism", () => {
  it("derives a stable ETag for an identical body", () => {
    expect(computeETag('{"a":1}')).toBe(computeETag('{"a":1}'));
  });

  it("produces different validators for different bodies", () => {
    expect(computeETag('{"a":1}')).not.toBe(computeETag('{"a":2}'));
  });

  it("repeats the same decision for the same request across calls", () => {
    const req = mockReq({ "if-none-match": BODY_ETAG });
    for (let i = 0; i < 5; i += 1) {
      expect(isNotModified(req, BODY_ETAG, LAST_MODIFIED)).toBe(true);
    }
  });
});
