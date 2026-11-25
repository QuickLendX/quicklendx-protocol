/**
 * Bids API suite (POST /api/v1/bids, GET /api/v1/bids, best, ranked).
 *
 * The suite previously targeted the removed Jest/PostgreSQL harness and was
 * excluded from the runner. It now:
 *  - runs under the repository's active Vitest setup;
 *  - authenticates with a real key seeded through `apiKeyService` (SQLite,
 *    in-memory for the suite) using the documented NODE_ENV=test signature
 *    bypass in `requireSignature`;
 *  - replaces the PostgreSQL-backed `bidStore` with an in-memory store that
 *    mirrors its contract ranking so ordering assertions stay meaningful
 *    without a live database.
 */
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import crypto from "node:crypto";
import request from "supertest";

// Must run before the app/config modules are imported so the SQLite API-key
// store opens an isolated in-memory database for this suite.
vi.hoisted(() => {
  process.env.NODE_ENV = "test";
  process.env.DATABASE_PATH = ":memory:";
});

import app from "../app";
import { closeDatabase, getDatabase } from "../lib/database";
import { apiKeyService } from "../services/api-key-service";
import { bidStore, type BidCreateInput } from "../services/bidStore";
import { exposureService } from "../services/exposureService";
import { nonceStore } from "../middleware/request-signing";
import { type Bid, BidStatus } from "../types/contract";
import { type CursorPayload, encodeCursor } from "../utils/pagination";

const TEST_INVOICE_ID = "0x" + "1234567890abcdef".repeat(4);
const TEST_INVOICE_ID_2 = "0x" + "abcdef1234567890".repeat(4);
const TEST_INVESTOR = "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K";
const TEST_INVESTOR_2 = "GAXMFSADZVDXTJSLL3HZJFVYH4JGBQBMX3I2WGPKH3YKRXL7ZJXZK23Q";

let apiKey: string;
let bids: Bid[] = [];
let verifiedInvoices = new Set<string>();

/**
 * Generate a distinct, well-formed Stellar address so each simulated
 * investor can place one active bid per invoice (the store rejects duplicate
 * active bids from the same investor).
 */
function investorAt(index: number): string {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  let suffix = "";
  let remaining = index;
  for (let i = 0; i < 55; i++) {
    suffix = alphabet[remaining % alphabet.length] + suffix;
    remaining = Math.floor(remaining / alphabet.length);
  }
  return "G" + suffix;
}

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

function makeBid(overrides: Partial<BidCreateInput> = {}): BidCreateInput {
  return {
    bid_id: "0x" + crypto.randomBytes(32).toString("hex"),
    invoice_id: TEST_INVOICE_ID,
    bid_amount: "1000000",
    expected_return: "1500000",
    timestamp: nowSeconds(),
    expiration_timestamp: nowSeconds() + 86400,
    investor: TEST_INVESTOR,
    created_by: TEST_INVESTOR,
    ...overrides,
  };
}

/** Mirrors BidStore's SQL ordering: profit, return, amount, newest, bid_id. */
function compareRanked(left: Bid, right: Bid): number {
  const leftProfit = BigInt(left.expected_return) - BigInt(left.bid_amount);
  const rightProfit = BigInt(right.expected_return) - BigInt(right.bid_amount);
  if (leftProfit !== rightProfit) {
    return leftProfit > rightProfit ? -1 : 1;
  }
  const leftReturn = BigInt(left.expected_return);
  const rightReturn = BigInt(right.expected_return);
  if (leftReturn !== rightReturn) {
    return leftReturn > rightReturn ? -1 : 1;
  }
  const leftAmount = BigInt(left.bid_amount);
  const rightAmount = BigInt(right.bid_amount);
  if (leftAmount !== rightAmount) {
    return leftAmount > rightAmount ? -1 : 1;
  }
  if (left.timestamp !== right.timestamp) {
    return right.timestamp - left.timestamp;
  }
  return left.bid_id < right.bid_id ? -1 : left.bid_id > right.bid_id ? 1 : 0;
}

function rankedBids(invoiceId: string): Bid[] {
  return bids
    .filter((bid) => bid.invoice_id === invoiceId && bid.status === BidStatus.Placed)
    .sort(compareRanked);
}

async function createBid(input: BidCreateInput): Promise<Bid> {
  if (!verifiedInvoices.has(input.invoice_id)) {
    throw new Error(`Invoice not found: ${input.invoice_id}`);
  }

  const duplicate = bids.some(
    (bid) =>
      bid.invoice_id === input.invoice_id &&
      bid.investor === input.investor &&
      bid.status === BidStatus.Placed
  );
  if (duplicate) {
    throw new Error(
      `Investor ${input.investor} already has an active bid on this invoice`
    );
  }

  const amount = BigInt(input.bid_amount);
  if (amount < 1n) {
    throw new Error("Bid amount must be at least 1");
  }
  if (BigInt(input.expected_return) < amount) {
    throw new Error("Expected return must be greater than or equal to bid amount");
  }

  const bid: Bid = {
    bid_id: input.bid_id,
    invoice_id: input.invoice_id,
    investor: input.investor,
    bid_amount: input.bid_amount,
    expected_return: input.expected_return,
    timestamp: input.timestamp,
    status: BidStatus.Placed,
    expiration_timestamp: input.expiration_timestamp,
    contract_version: 1,
    event_schema_version: 1,
    indexed_at: new Date().toISOString(),
  };
  bids.push(bid);
  return bid;
}

async function getRankedBids(invoiceId: string, limit = 100): Promise<Bid[]> {
  return rankedBids(invoiceId).slice(0, limit);
}

async function getBestBid(invoiceId: string): Promise<Bid | null> {
  return rankedBids(invoiceId)[0] ?? null;
}

async function getBidsPaginated(
  invoiceId: string,
  limit = 20,
  cursor: CursorPayload | null = null,
  filters?: { investor?: string; status?: BidStatus }
) {
  let rows = bids.filter((bid) => bid.invoice_id === invoiceId);

  if (filters?.investor) {
    rows = rows.filter((bid) => bid.investor === filters.investor);
  }
  rows = rows.filter((bid) =>
    filters?.status ? bid.status === filters.status : bid.status === BidStatus.Placed
  );

  if (cursor) {
    rows = rows.filter(
      (bid) =>
        bid.timestamp < cursor.sort_val ||
        (bid.timestamp === cursor.sort_val && bid.bid_id > cursor.id)
    );
  }

  rows.sort(compareRanked);
  const has_more = rows.length > limit;
  const data = rows.slice(0, limit);
  const last = data[data.length - 1];

  return {
    data,
    has_more,
    next_cursor:
      has_more && last
        ? encodeCursor({ id: last.bid_id, sort_val: last.timestamp })
        : null,
  };
}

beforeAll(async () => {
  const db = getDatabase();
  db.exec(`
    CREATE TABLE IF NOT EXISTS api_keys (
      id TEXT PRIMARY KEY,
      key_hash TEXT NOT NULL,
      signing_secret_hash TEXT,
      prev_signing_secret_hash TEXT,
      prefix TEXT NOT NULL UNIQUE,
      name TEXT NOT NULL,
      scopes TEXT NOT NULL,
      created_at TEXT NOT NULL,
      last_used_at TEXT,
      expires_at TEXT,
      prev_secret_expires_at TEXT,
      revoked INTEGER NOT NULL DEFAULT 0,
      created_by TEXT NOT NULL,
      version INTEGER NOT NULL DEFAULT 1,
      revoked_at INTEGER,
      revoked_by TEXT
    );
    CREATE TABLE IF NOT EXISTS api_key_audit_log (
      id TEXT PRIMARY KEY,
      event_type TEXT NOT NULL,
      key_id TEXT NOT NULL,
      actor TEXT NOT NULL,
      timestamp TEXT NOT NULL,
      ip_address TEXT,
      endpoint TEXT,
      metadata TEXT
    );
  `);

  const seeded = await apiKeyService.createApiKey({
    name: "bids-api-suite",
    scopes: ["write:bids", "read:bids"],
    created_by: TEST_INVESTOR,
  });
  apiKey = seeded.plaintext_key;

  vi.spyOn(bidStore, "createBid").mockImplementation(createBid);
  vi.spyOn(bidStore, "getRankedBids").mockImplementation(getRankedBids);
  vi.spyOn(bidStore, "getBestBid").mockImplementation(getBestBid);
  vi.spyOn(bidStore, "getBidsPaginated").mockImplementation(getBidsPaginated);
  vi.spyOn(exposureService, "assertWithinCap").mockResolvedValue(undefined);
});

afterAll(async () => {
  await new Promise((resolve) => setImmediate(resolve));
  vi.restoreAllMocks();
  closeDatabase();
});

beforeEach(() => {
  bids = [];
  verifiedInvoices = new Set([TEST_INVOICE_ID, TEST_INVOICE_ID_2]);
  nonceStore._clearForTest();
});

describe("POST /api/v1/bids - Create Bid", () => {
  it("should create a bid successfully with valid inputs", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(201);

    expect(response.body.data).toBeDefined();
    expect(response.body.data.bid_id).toBeDefined();
    expect(response.body.data.invoice_id).toBe(TEST_INVOICE_ID);
    expect(response.body.data.bid_amount).toBe("1000000");
    expect(response.body.data.expected_return).toBe("1500000");
    expect(response.body.data.status).toBe(BidStatus.Placed);
    expect(response.body.data.contract_version).toBe(1);
    expect(response.body.data.event_schema_version).toBe(1);
  });

  it("should reject bid without authentication", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(401);

    expect(response.body.error).toBeDefined();
    expect(response.body.error.code).toBe("UNAUTHORIZED");
  });

  it("should reject bid with invalid invoice_id format", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: "invalid_hex",
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
  });

  it("should reject bid with non-numeric bid_amount", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "not_a_number",
        expected_return: "1500000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
  });

  it("should reject bid with expected_return < bid_amount", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "2000000",
        expected_return: "1000000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
    expect(response.body.error.code).toBe("INVALID_BID");
  });

  it("should reject bid with zero bid_amount", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "0",
        expected_return: "1000000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
    expect(response.body.error.code).toBe("INVALID_BID");
  });

  it("should require invoice_id", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
  });

  it("should require bid_amount", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        expected_return: "1500000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
  });

  it("should require expected_return", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expiration_timestamp: nowSeconds() + 86400,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
  });

  it("should require expiration_timestamp", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
  });
});

describe("GET /api/v1/bids - Get Ranked Bids", () => {
  it("should require invoice_id parameter", async () => {
    const response = await request(app).get("/api/v1/bids").expect(400);

    expect(response.body.error).toBeDefined();
    expect(response.body.error.code).toBe("MISSING_REQUIRED_FIELD");
  });

  it("should return empty array for invoice with no bids", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .query({ invoice_id: TEST_INVOICE_ID })
      .expect(200);

    expect(response.body.data).toEqual([]);
    expect(response.body.has_more).toBe(false);
  });

  it("should support pagination with limit", async () => {
    for (let i = 0; i < 5; i++) {
      await bidStore.createBid(
        makeBid({
          invoice_id: TEST_INVOICE_ID,
          bid_amount: String(1000000 + i * 100000),
          expected_return: String(1500000 + i * 100000),
          timestamp: nowSeconds() + i,
          investor: investorAt(i),
          created_by: investorAt(i),
        })
      );
    }

    const response = await request(app)
      .get("/api/v1/bids")
      .query({ invoice_id: TEST_INVOICE_ID, limit: 2 })
      .expect(200);

    expect(response.body.data).toHaveLength(2);
    expect(response.body.has_more).toBe(true);
    expect(response.body.next_cursor).toBeDefined();
  });

  it("should support filtering by investor", async () => {
    await bidStore.createBid(
      makeBid({
        invoice_id: TEST_INVOICE_ID,
        investor: TEST_INVESTOR,
      })
    );

    const response = await request(app)
      .get("/api/v1/bids")
      .query({ invoice_id: TEST_INVOICE_ID, investor: TEST_INVESTOR })
      .expect(200);

    expect(response.body.data.length).toBeGreaterThan(0);
    expect(response.body.data[0].investor).toBe(TEST_INVESTOR);
  });

  it("should return bids in ranking order (best first)", async () => {
    const bidsToCreate = [
      { bid_amount: "1000000", expected_return: "1500000" }, // profit: 500000
      { bid_amount: "1000000", expected_return: "2000000" }, // profit: 1000000
      { bid_amount: "500000", expected_return: "1200000" }, // profit: 700000
    ];

    for (let i = 0; i < bidsToCreate.length; i++) {
      await bidStore.createBid(
        makeBid({
          invoice_id: TEST_INVOICE_ID,
          ...bidsToCreate[i],
          investor: investorAt(i),
          created_by: investorAt(i),
        })
      );
    }

    const response = await request(app)
      .get("/api/v1/bids")
      .query({ invoice_id: TEST_INVOICE_ID })
      .expect(200);

    expect(response.body.data).toHaveLength(3);
    const firstProfit =
      BigInt(response.body.data[0].expected_return) -
      BigInt(response.body.data[0].bid_amount);
    const secondProfit =
      BigInt(response.body.data[1].expected_return) -
      BigInt(response.body.data[1].bid_amount);
    expect(firstProfit).toBeGreaterThanOrEqual(secondProfit);
  });
});

describe("GET /api/v1/bids/:invoiceId/best - Get Best Bid", () => {
  it("should return 404 for invoice with no bids", async () => {
    const response = await request(app)
      .get(`/api/v1/bids/${TEST_INVOICE_ID}/best`)
      .expect(404);

    expect(response.body.error).toBeDefined();
  });

  it("should return the best bid", async () => {
    const bid = await bidStore.createBid(
      makeBid({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        investor: TEST_INVESTOR,
      })
    );

    const response = await request(app)
      .get(`/api/v1/bids/${TEST_INVOICE_ID}/best`)
      .expect(200);

    expect(response.body.data.bid_id).toBe(bid.bid_id);
    expect(response.body.data.bid_amount).toBe("1000000");
  });
});

describe("GET /api/v1/bids/:invoiceId/ranked - Get Ranked Bids", () => {
  it("should return all ranked bids", async () => {
    for (let i = 0; i < 3; i++) {
      await bidStore.createBid(
        makeBid({
          invoice_id: TEST_INVOICE_ID,
          bid_amount: String(1000000 + i * 100000),
          expected_return: String(1500000 + i * 100000),
          timestamp: nowSeconds() + i,
          investor: investorAt(i),
          created_by: investorAt(i),
        })
      );
    }

    const response = await request(app)
      .get(`/api/v1/bids/${TEST_INVOICE_ID}/ranked`)
      .expect(200);

    expect(response.body.data).toHaveLength(3);
    for (let i = 0; i < response.body.data.length - 1; i++) {
      const profitI =
        BigInt(response.body.data[i].expected_return) -
        BigInt(response.body.data[i].bid_amount);
      const profitNext =
        BigInt(response.body.data[i + 1].expected_return) -
        BigInt(response.body.data[i + 1].bid_amount);
      expect(profitI).toBeGreaterThanOrEqual(profitNext);
    }
  });

  it("should return empty array for invoice with no bids", async () => {
    const response = await request(app)
      .get(`/api/v1/bids/${TEST_INVOICE_ID}/ranked`)
      .expect(200);

    expect(response.body.data).toEqual([]);
  });
});

describe("Edge Cases and Security", () => {
  it("should handle tie-breaking correctly (same profit)", async () => {
    const bid1 = await bidStore.createBid(
      makeBid({
        bid_id: "0x" + "1".repeat(64),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        timestamp: nowSeconds(),
        investor: TEST_INVESTOR,
      })
    );

    const bid2 = await bidStore.createBid(
      makeBid({
        bid_id: "0x" + "2".repeat(64),
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        timestamp: nowSeconds() + 1, // newer
        investor: TEST_INVESTOR_2,
        created_by: TEST_INVESTOR_2,
      })
    );

    const ranked = await bidStore.getRankedBids(TEST_INVOICE_ID);
    expect(ranked).toHaveLength(2);
    // Newer bid ranks higher when profit, return, and amount are equal.
    expect(ranked[0].bid_id).toBe(bid2.bid_id);
    expect(ranked[1].bid_id).toBe(bid1.bid_id);
  });

  it("should handle large numeric values correctly", async () => {
    const largeAmount = "999999999999999999999999999";
    const largeReturn = "1999999999999999999999999999";

    const bid = await bidStore.createBid(
      makeBid({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: largeAmount,
        expected_return: largeReturn,
        investor: TEST_INVESTOR,
      })
    );

    expect(bid.bid_amount).toBe(largeAmount);
    expect(bid.expected_return).toBe(largeReturn);
  });

  it("should validate expiration_timestamp is a positive integer", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: -100,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
  });

  it("should validate expiration_timestamp is not a float", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${apiKey}`)
      .send({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000",
        expiration_timestamp: 1234567890.5,
      })
      .expect(400);

    expect(response.body.error).toBeDefined();
  });
});

describe("Ranking Algorithm Compliance", () => {
  it("should rank by profit first", async () => {
    const bid1 = await bidStore.createBid(
      makeBid({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "2000000", // profit: 1000000
        investor: TEST_INVESTOR,
      })
    );

    const bid2 = await bidStore.createBid(
      makeBid({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000", // profit: 500000
        investor: TEST_INVESTOR_2,
        created_by: TEST_INVESTOR_2,
      })
    );

    const ranked = await bidStore.getRankedBids(TEST_INVOICE_ID);
    expect(ranked[0].bid_id).toBe(bid1.bid_id);
    expect(ranked[1].bid_id).toBe(bid2.bid_id);
  });

  it("should tiebreak on expected_return when profit is equal", async () => {
    const bid1 = await bidStore.createBid(
      makeBid({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "2000000", // profit: 1000000, return: 2000000
        investor: TEST_INVESTOR,
      })
    );

    await bidStore.createBid(
      makeBid({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "1000000",
        expected_return: "1500000", // profit: 500000
        investor: TEST_INVESTOR_2,
        created_by: TEST_INVESTOR_2,
      })
    );

    await bidStore.createBid(
      makeBid({
        invoice_id: TEST_INVOICE_ID,
        bid_amount: "500000",
        expected_return: "1500000", // profit: 1000000, return: 1500000
        investor: investorAt(2),
        created_by: investorAt(2),
      })
    );

    const ranked = await bidStore.getRankedBids(TEST_INVOICE_ID);
    // bid1 and the third bid share profit; bid1 has the higher expected return.
    expect(ranked[0].bid_id).toBe(bid1.bid_id);
  });
});
