/**
 * Comprehensive pagination tests.
 *
 * Covers:
 *  - Unit: encodeCursor / decodeCursor round-trip and rejection of malformed input
 *  - Unit: parsePaginationParams — limit clamping, default, invalid values, cursor parsing
 *  - Unit: applyPagination — stable ordering, cursor seek, has_more, boundary conditions
 *  - Integration: /invoices, /bids, /settlements, /portfolio endpoints via supertest
 *  - Security: no unbounded scans (limit always clamped), opaque cursor, invalid cursor → 400
 */

import { describe, expect, it } from "@jest/globals";
import request from "supertest";
import app from "../app";
import {
  encodeCursor,
  decodeCursor,
  parsePaginationParams,
  applyPagination,
  PaginationError,
  DEFAULT_LIMIT,
  MAX_LIMIT,
} from "../utils/pagination";

// ---------------------------------------------------------------------------
// Unit: cursor encode / decode
// ---------------------------------------------------------------------------
describe("encodeCursor / decodeCursor", () => {
  it("round-trips a valid payload", () => {
    const payload = { id: "abc", sort_val: 12345 };
    expect(decodeCursor(encodeCursor(payload))).toEqual(payload);
  });

  it("returns null for empty string", () => {
    expect(decodeCursor("")).toBeNull();
  });

  it("returns null for non-base64url garbage", () => {
    expect(decodeCursor("!!!not-valid!!!")).toBeNull();
  });

  it("returns null when id is missing", () => {
    const raw = Buffer.from(JSON.stringify({ sort_val: 1 })).toString("base64url");
    expect(decodeCursor(raw)).toBeNull();
  });

  it("returns null when sort_val is not a number", () => {
    const raw = Buffer.from(JSON.stringify({ id: "x", sort_val: "bad" })).toString("base64url");
    expect(decodeCursor(raw)).toBeNull();
  });

  it("returns null when sort_val is Infinity", () => {
    const raw = Buffer.from(JSON.stringify({ id: "x", sort_val: Infinity })).toString("base64url");
    // JSON.stringify(Infinity) → "null", so sort_val will be null → not a number
    expect(decodeCursor(raw)).toBeNull();
  });

  it("returns null for valid base64url but non-object JSON", () => {
    const raw = Buffer.from(JSON.stringify([1, 2, 3])).toString("base64url");
    expect(decodeCursor(raw)).toBeNull();
  });

  it("returns null for null JSON value", () => {
    const raw = Buffer.from("null").toString("base64url");
    expect(decodeCursor(raw)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Unit: parsePaginationParams
// ---------------------------------------------------------------------------
describe("parsePaginationParams", () => {
  it("returns defaults when no params given", () => {
    const p = parsePaginationParams({});
    expect(p.limit).toBe(DEFAULT_LIMIT);
    expect(p.cursor).toBeNull();
  });

  it("parses a valid limit", () => {
    expect(parsePaginationParams({ limit: "10" }).limit).toBe(10);
  });

  it("clamps limit to MAX_LIMIT", () => {
    expect(parsePaginationParams({ limit: "9999" }).limit).toBe(MAX_LIMIT);
  });

  it("clamps limit of exactly MAX_LIMIT", () => {
    expect(parsePaginationParams({ limit: String(MAX_LIMIT) }).limit).toBe(MAX_LIMIT);
  });

  it("throws PaginationError for limit=0", () => {
    expect(() => parsePaginationParams({ limit: "0" })).toThrow(PaginationError);
  });

  it("throws PaginationError for negative limit", () => {
    expect(() => parsePaginationParams({ limit: "-5" })).toThrow(PaginationError);
  });

  it("throws PaginationError for non-integer limit", () => {
    expect(() => parsePaginationParams({ limit: "1.5" })).toThrow(PaginationError);
  });

  it("throws PaginationError for non-numeric limit", () => {
    expect(() => parsePaginationParams({ limit: "abc" })).toThrow(PaginationError);
  });

  it("ignores empty cursor string", () => {
    expect(parsePaginationParams({ cursor: "" }).cursor).toBeNull();
  });

  it("throws PaginationError for non-string cursor", () => {
    expect(() => parsePaginationParams({ cursor: 123 })).toThrow(PaginationError);
  });

  it("throws PaginationError for malformed cursor", () => {
    expect(() => parsePaginationParams({ cursor: "not-valid-base64url-json" })).toThrow(PaginationError);
  });

  it("parses a valid cursor", () => {
    const payload = { id: "x", sort_val: 100 };
    const p = parsePaginationParams({ cursor: encodeCursor(payload) });
    expect(p.cursor).toEqual(payload);
  });
});

// ---------------------------------------------------------------------------
// Unit: applyPagination
// ---------------------------------------------------------------------------
describe("applyPagination", () => {
  type Item = { id: string; ts: number };

  const makeItems = (n: number): Item[] =>
    Array.from({ length: n }, (_, i) => ({
      id: String(i).padStart(4, "0"),
      ts: 1000 - i, // descending timestamps so sort order is 0,1,2,...
    }));

  it("returns all items when count < limit", () => {
    const items = makeItems(3);
    const result = applyPagination(items, "ts", { limit: 10, cursor: null });
    expect(result.data).toHaveLength(3);
    expect(result.has_more).toBe(false);
    expect(result.next_cursor).toBeNull();
  });

  it("returns first page and sets has_more", () => {
    const items = makeItems(5);
    const result = applyPagination(items, "ts", { limit: 2, cursor: null });
    expect(result.data).toHaveLength(2);
    expect(result.has_more).toBe(true);
    expect(result.next_cursor).not.toBeNull();
  });

  it("stable ordering: sort DESC by sort field, ASC by id as tiebreaker", () => {
    const items: Item[] = [
      { id: "b", ts: 100 },
      { id: "a", ts: 100 },
      { id: "c", ts: 200 },
    ];
    const result = applyPagination(items, "ts", { limit: 10, cursor: null });
    expect(result.data.map((x: Item) => x.id)).toEqual(["c", "a", "b"]);
  });

  it("cursor seek returns correct next page", () => {
    const items = makeItems(5);
    const page1 = applyPagination(items, "ts", { limit: 2, cursor: null });
    const cursor = parsePaginationParams({ cursor: page1.next_cursor! }).cursor;
    const page2 = applyPagination(items, "ts", { limit: 2, cursor });
    expect(page2.data[0].id).toBe("0002");
    expect(page2.data[1].id).toBe("0003");
    expect(page2.has_more).toBe(true);
  });

  it("last page has_more=false and next_cursor=null", () => {
    const items = makeItems(4);
    const page1 = applyPagination(items, "ts", { limit: 2, cursor: null });
    const cursor = parsePaginationParams({ cursor: page1.next_cursor! }).cursor;
    const page2 = applyPagination(items, "ts", { limit: 2, cursor });
    expect(page2.has_more).toBe(false);
    expect(page2.next_cursor).toBeNull();
  });

  it("cursor past end returns empty page", () => {
    const items = makeItems(2);
    // Cursor pointing to a sort_val lower than all items
    const cursor = { id: "zzzz", sort_val: -1 };
    const result = applyPagination(items, "ts", { limit: 10, cursor });
    expect(result.data).toHaveLength(0);
    expect(result.has_more).toBe(false);
  });

  it("stability under inserts: inserting item before cursor does not duplicate", () => {
    const items = makeItems(6);
    const page1 = applyPagination(items, "ts", { limit: 3, cursor: null });
    // Simulate a new item inserted with a high ts (before cursor position)
    const newItem: Item = { id: "new0", ts: 1500 };
    const itemsWithInsert = [newItem, ...items];
    const cursor = parsePaginationParams({ cursor: page1.next_cursor! }).cursor;
    const page2 = applyPagination(itemsWithInsert, "ts", { limit: 3, cursor });
    // page2 should not contain any items from page1
    const page1Ids = new Set(page1.data.map((x: Item) => x.id));
    for (const item of page2.data) {
      expect(page1Ids.has(item.id)).toBe(false);
    }
  });

  it("empty dataset returns empty result", () => {
    const result = applyPagination([], "ts", { limit: 10, cursor: null });
    expect(result.data).toHaveLength(0);
    expect(result.has_more).toBe(false);
    expect(result.next_cursor).toBeNull();
  });

  it("limit=1 pages through all items one by one", () => {
    const items = makeItems(3);
    const ids: string[] = [];
    let cursor: ReturnType<typeof parsePaginationParams>["cursor"] = null;
    for (let i = 0; i < 3; i++) {
      const result: ReturnType<typeof applyPagination<Item>> = applyPagination(items, "ts", { limit: 1, cursor });
      expect(result.data).toHaveLength(1);
      ids.push(result.data[0].id);
      cursor = result.next_cursor
        ? parsePaginationParams({ cursor: result.next_cursor }).cursor
        : null;
    }
    expect(ids).toEqual(["0000", "0001", "0002"]);
  });
});

// ---------------------------------------------------------------------------
// Integration: /api/v1/invoices
// ---------------------------------------------------------------------------
describe("GET /api/v1/invoices (pagination)", () => {
  it("returns PageResult shape", async () => {
    const res = await request(app).get("/api/v1/invoices");
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("data");
    expect(res.body).toHaveProperty("has_more");
    expect(res.body).toHaveProperty("next_cursor");
    expect(Array.isArray(res.body.data)).toBe(true);
  });

  it("respects limit param", async () => {
    const res = await request(app).get("/api/v1/invoices?limit=1");
    expect(res.status).toBe(200);
    expect(res.body.data.length).toBeLessThanOrEqual(1);
  });

  it("clamps limit above MAX_LIMIT", async () => {
    const res = await request(app).get("/api/v1/invoices?limit=9999");
    expect(res.status).toBe(200);
    expect(res.body.data.length).toBeLessThanOrEqual(MAX_LIMIT);
  });

  it("returns 400 for invalid limit", async () => {
    const res = await request(app).get("/api/v1/invoices?limit=0");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("returns 400 for malformed cursor", async () => {
    const res = await request(app).get("/api/v1/invoices?cursor=!!!bad!!!");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("filters by business and returns PageResult", async () => {
    const res = await request(app).get("/api/v1/invoices?business=GDVLRH4G4...7Y");
    expect(res.status).toBe(200);
    expect(res.body.data.every((i: any) => i.business === "GDVLRH4G4...7Y")).toBe(true);
  });

  it("filters by status and returns PageResult", async () => {
    const res = await request(app).get("/api/v1/invoices?status=Verified");
    expect(res.status).toBe(200);
    expect(res.body.data.every((i: any) => i.status === "Verified")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Integration: /api/v1/bids
// ---------------------------------------------------------------------------
describe("GET /api/v1/bids (pagination)", () => {
  it("returns PageResult shape", async () => {
    const res = await request(app).get("/api/v1/bids");
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("data");
    expect(res.body).toHaveProperty("has_more");
    expect(res.body).toHaveProperty("next_cursor");
  });

  it("bid items do not expose synthetic id field", async () => {
    const res = await request(app).get("/api/v1/bids");
    expect(res.status).toBe(200);
    for (const bid of res.body.data) {
      expect(bid).not.toHaveProperty("id");
      expect(bid).toHaveProperty("bid_id");
    }
  });

  it("returns 400 for invalid limit", async () => {
    const res = await request(app).get("/api/v1/bids?limit=-1");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("returns 400 for malformed cursor", async () => {
    const res = await request(app).get("/api/v1/bids?cursor=garbage");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("filters by invoice_id", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/bids?invoice_id=${id}`);
    expect(res.status).toBe(200);
    expect(res.body.data.every((b: any) => b.invoice_id === id)).toBe(true);
  });

  it("filters by investor", async () => {
    const res = await request(app).get("/api/v1/bids?investor=GA...ABC");
    expect(res.status).toBe(200);
    expect(res.body.data.every((b: any) => b.investor === "GA...ABC")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Integration: /api/v1/settlements
// ---------------------------------------------------------------------------
describe("GET /api/v1/settlements (pagination)", () => {
  it("returns PageResult shape", async () => {
    const res = await request(app).get("/api/v1/settlements");
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("data");
    expect(res.body).toHaveProperty("has_more");
    expect(res.body).toHaveProperty("next_cursor");
  });

  it("returns 400 for invalid limit", async () => {
    const res = await request(app).get("/api/v1/settlements?limit=abc");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("returns 400 for malformed cursor", async () => {
    const res = await request(app).get("/api/v1/settlements?cursor=bad");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("filters by invoice_id", async () => {
    const id = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const res = await request(app).get(`/api/v1/settlements?invoice_id=${id}`);
    expect(res.status).toBe(200);
    expect(res.body.data.every((s: any) => s.invoice_id === id)).toBe(true);
  });

  it("GET /settlements/:id still works (non-paginated)", async () => {
    const res = await request(app).get("/api/v1/settlements/0xsettle123");
    expect(res.status).toBe(200);
    expect(res.body.id).toBe("0xsettle123");
  });

  it("GET /settlements/:id returns 404 for unknown id", async () => {
    const res = await request(app).get("/api/v1/settlements/unknown");
    expect(res.status).toBe(404);
    expect(res.body.error.code).toBe("SETTLEMENT_NOT_FOUND");
  });
});

// ---------------------------------------------------------------------------
// Integration: /api/v1/portfolio
// ---------------------------------------------------------------------------
describe("GET /api/v1/portfolio (pagination)", () => {
  it("returns 400 when investor param is missing", async () => {
    const res = await request(app).get("/api/v1/portfolio");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("MISSING_INVESTOR");
  });

  it("returns PageResult shape for known investor", async () => {
    const res = await request(app).get("/api/v1/portfolio?investor=GA...ABC");
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("data");
    expect(res.body).toHaveProperty("has_more");
    expect(res.body).toHaveProperty("next_cursor");
  });

  it("returns empty data for unknown investor", async () => {
    const res = await request(app).get("/api/v1/portfolio?investor=UNKNOWN");
    expect(res.status).toBe(200);
    expect(res.body.data).toHaveLength(0);
    expect(res.body.has_more).toBe(false);
  });

  it("returns 400 for invalid limit", async () => {
    const res = await request(app).get("/api/v1/portfolio?investor=GA...ABC&limit=0");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("returns 400 for malformed cursor", async () => {
    const res = await request(app).get("/api/v1/portfolio?investor=GA...ABC&cursor=bad");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });
});

// ---------------------------------------------------------------------------
// Branch coverage: applyPagination edge cases
// ---------------------------------------------------------------------------
describe("applyPagination branch coverage", () => {
  type Item = { id: string; ts: number };

  it("sort tiebreaker: equal ids (same item) returns 0", () => {
    // Two items with same ts and same id — sort is stable, no crash
    const items: Item[] = [
      { id: "same", ts: 100 },
      { id: "same", ts: 100 },
    ];
    const result = applyPagination(items, "ts", { limit: 10, cursor: null });
    expect(result.data).toHaveLength(2);
  });

  it("cursor seek: item at cursor position (id <= cursorId, same sort_val) is skipped", () => {
    // cursor points to id="b", sort_val=100; item "a" has same sort_val but id < cursorId
    // so "a" should NOT be returned (it comes before "b" in id-ASC order)
    const items: Item[] = [
      { id: "a", ts: 100 },
      { id: "b", ts: 100 },
      { id: "c", ts: 100 },
    ];
    const cursor = { id: "b", sort_val: 100 };
    const result = applyPagination(items, "ts", { limit: 10, cursor });
    // Only "c" should appear (id > "b")
    expect(result.data.map((x: Item) => x.id)).toEqual(["c"]);
  });
});

// ---------------------------------------------------------------------------
// Branch coverage: controller error paths (next(error) catch)
// ---------------------------------------------------------------------------
describe("Controller error propagation (unit)", () => {
  // Test the catch(error) → next(error) path by calling controllers directly
  // with a req that causes an error inside the try block.

  it("invoices getInvoices: calls next(error) on unexpected throw", async () => {
    const { getInvoices } = require("../controllers/v1/invoices");
    const req = { query: { limit: "1" } } as any;
    // Make res.json throw to trigger catch
    const res = { json: () => { throw new Error("res.json failed"); }, status: () => res } as any;
    const next = jest.fn();
    await getInvoices(req, res, next);
    expect(next).toHaveBeenCalledWith(expect.any(Error));
  });

  it("bids getBids: calls next(error) on unexpected throw", async () => {
    const { getBids } = require("../controllers/v1/bids");
    const req = { query: { limit: "1" } } as any;
    const res = { json: () => { throw new Error("res.json failed"); }, status: () => res } as any;
    const next = jest.fn();
    await getBids(req, res, next);
    expect(next).toHaveBeenCalledWith(expect.any(Error));
  });

  it("settlements getSettlements: calls next(error) on unexpected throw", async () => {
    const { getSettlements } = require("../controllers/v1/settlements");
    const req = { query: { limit: "1" } } as any;
    const res = { json: () => { throw new Error("res.json failed"); }, status: () => res } as any;
    const next = jest.fn();
    await getSettlements(req, res, next);
    expect(next).toHaveBeenCalledWith(expect.any(Error));
  });

  it("portfolio getPortfolio: calls next(error) on unexpected throw", async () => {
    const { getPortfolio } = require("../controllers/v1/portfolio");
    const req = { query: { investor: "GA...ABC", limit: "1" } } as any;
    const res = { json: () => { throw new Error("res.json failed"); }, status: () => res } as any;
    const next = jest.fn();
    await getPortfolio(req, res, next);
    expect(next).toHaveBeenCalledWith(expect.any(Error));
  });

  it("disputes getDisputes: calls next(error) on unexpected throw", async () => {
    const { getDisputes } = require("../controllers/v1/disputes");
    const req = { params: { id: "0x1234" } } as any;
    const res = { json: () => { throw new Error("res.json failed"); } } as any;
    const next = jest.fn();
    await getDisputes(req, res, next);
    expect(next).toHaveBeenCalledWith(expect.any(Error));
  });

  it("settlements getSettlementById: calls next(error) on unexpected throw", async () => {
    const { getSettlementById } = require("../controllers/v1/settlements");
    const req = { params: { id: "0xsettle123" } } as any;
    const res = { json: () => { throw new Error("res.json failed"); }, status: () => res } as any;
    const next = jest.fn();
    await getSettlementById(req, res, next);
    expect(next).toHaveBeenCalledWith(expect.any(Error));
  });

  it("invoices getInvoiceById: calls next(error) on unexpected throw", async () => {
    const { getInvoiceById } = require("../controllers/v1/invoices");
    const req = { params: { id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef" } } as any;
    const res = { json: () => { throw new Error("res.json failed"); }, status: () => res } as any;
    const next = jest.fn();
    await getInvoiceById(req, res, next);
    expect(next).toHaveBeenCalledWith(expect.any(Error));
  });
});
describe("Security properties", () => {
  it("limit is always bounded — response never exceeds MAX_LIMIT items", async () => {
    const res = await request(app).get(`/api/v1/invoices?limit=${MAX_LIMIT + 1000}`);
    expect(res.status).toBe(200);
    expect(res.body.data.length).toBeLessThanOrEqual(MAX_LIMIT);
  });

  it("cursor is opaque — raw cursor string is not a plain ID", async () => {
    const res = await request(app).get("/api/v1/invoices");
    // next_cursor may be null if only 1 item; just verify it's not a plain id string
    if (res.body.next_cursor !== null) {
      expect(res.body.next_cursor).not.toBe(
        "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
      );
    }
  });

  it("tampered cursor returns 400", async () => {
    const tampered = Buffer.from('{"id":1,"sort_val":"bad"}').toString("base64url");
    const res = await request(app).get(`/api/v1/invoices?cursor=${tampered}`);
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("cursor with extra fields is accepted (forward-compat)", () => {
    // Extra fields are ignored; only id and sort_val are extracted
    const raw = Buffer.from(
      JSON.stringify({ id: "abc", sort_val: 100, extra: "ignored" })
    ).toString("base64url");
    const decoded = decodeCursor(raw);
    expect(decoded).not.toBeNull();
    expect(decoded!.id).toBe("abc");
    expect(decoded!.sort_val).toBe(100);
  });
});
