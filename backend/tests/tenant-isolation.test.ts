/**
 * Tenant Isolation Security Test Suite
 *
 * Purpose: Validate that QuickLendX enforces data separation between tenants,
 * preventing cross-tenant information leakage through data access and metadata
 * exposure.
 *
 * Security model (current backend contract):
 * - Invoice/bid read endpoints are public marketplace reads when unauthenticated.
 * - When a request is authenticated with an API key, reads are scoped to the
 *   key owner (req.apiKey.created_by):
 *     - Invoice list: auto-scoped to the owner's business; a foreign business
 *       filter yields an empty page.
 *     - Invoice detail: a foreign invoice returns 404 (NOT 403) to prevent
 *       resource enumeration.
 *     - Bid list: the invoice owner sees all bids on their invoice; any other
 *       authenticated caller is restricted to their own bids.
 * - Writes bind identity to the authenticated key (POST /bids cannot attribute a
 *   bid to another investor); cross-tenant credential mutation is FORBIDDEN (403).
 * - Export functionality strictly filters by authenticated user context.
 * - Pagination cursors are scope-bound and cannot be replayed across tenants.
 * - Error messages never expose database IDs or internal validation metadata.
 *
 * Isolation strategy:
 * - A dedicated SQLite database (config.DATABASE_PATH) is seeded with the tenant
 *   fixtures so invoice reads exercise the real invoiceStore.
 * - bidStore is backed by an in-memory fixture mock (the real store uses a pg
 *   pool that is stubbed empty under jest).
 * - API-key auth is driven through the real middleware via setApiKeyVerifier.
 */

import request from "supertest";
import crypto from "crypto";
import fs from "fs";
import path from "path";
import app from "../src/app";
import { config } from "../src/config";
import { getDatabase, closeDatabase } from "../src/lib/database";
import { invoiceStore } from "../src/services/invoiceStore";
import { bidStore } from "../src/services/bidStore";
import { MOCK_BIDS } from "../src/controllers/v1/bids";
import { MOCK_INVOICES } from "../src/controllers/v1/invoices";
import { exportService } from "../src/services/exportService";
import { exposureService } from "../src/services/exposureService";
import { MOCK_SETTLEMENTS } from "../src/controllers/v1/settlements";
import { setApiKeyVerifier } from "../src/middleware/api-key-auth";
import { AuthorizationService } from "../src/services/authorization";
import { ApiKeysController } from "../src/controllers/v1/api-keys";
import { FrozenClock } from "../src/services/clock";
import { ApiKeyError } from "../src/errors/api-key-error";
import {
  API_KEY_BUSINESS_A,
  API_KEY_BUSINESS_B,
  API_KEY_INVESTOR_A,
  API_KEY_INVESTOR_B,
  API_KEY_BY_TOKEN,
  TOKEN_BUSINESS_A,
  TOKEN_BUSINESS_B,
  TOKEN_INVESTOR_A,
  TOKEN_INVESTOR_B,
  TENANT_BUSINESS_A,
  TENANT_BUSINESS_B,
  TENANT_INVESTOR_A,
  TENANT_INVESTOR_B,
  INVOICE_BUSINESS_A_1,
  INVOICE_BUSINESS_A_2,
  INVOICE_BUSINESS_B_1,
  INVOICE_BUSINESS_B_2,
  BID_INVESTOR_A_ON_BUSINESS_A,
  BID_INVESTOR_B_ON_BUSINESS_B,
  BID_INVESTOR_A_ON_BUSINESS_B,
  ALL_TENANT_INVOICES,
  ALL_TENANT_BIDS,
  createTenantSchema,
  seedTenantInvoices,
} from "./fixtures/multi-tenant";

// ─── Fixture-backed bidStore mock ────────────────────────────────────────────
// The real bidStore reads from a pg pool that jest maps to an empty stub, so we
// back it with the tenant fixtures to exercise ranking/filtering/pagination.

jest.mock("../src/services/bidStore", () => {
  const { ALL_TENANT_BIDS } = require("./fixtures/multi-tenant");
  const { encodeCursor } = require("../src/utils/pagination");

  let bids: any[] = ALL_TENANT_BIDS.map((b: any) => ({ ...b }));

  const profit = (b: any) => BigInt(b.expected_return) - BigInt(b.bid_amount);
  const rank = (a: any, b: any) => {
    const pa = profit(a);
    const pb = profit(b);
    if (pa !== pb) return pa > pb ? -1 : 1;
    const ea = BigInt(a.expected_return);
    const eb = BigInt(b.expected_return);
    if (ea !== eb) return ea > eb ? -1 : 1;
    const ba = BigInt(a.bid_amount);
    const bb = BigInt(b.bid_amount);
    if (ba !== bb) return ba > bb ? -1 : 1;
    if (a.timestamp !== b.timestamp) return b.timestamp - a.timestamp;
    return a.bid_id < b.bid_id ? -1 : a.bid_id > b.bid_id ? 1 : 0;
  };

  const bidStore = {
    getBidsPaginated: jest.fn(
      async (invoiceId: string, limit = 20, cursor: any = null, filters: any = {}) => {
        let rows = bids.filter((b) => b.invoice_id === invoiceId);
        if (filters.investor) rows = rows.filter((b) => b.investor === filters.investor);
        const status = filters.status ?? "Placed";
        rows = rows.filter((b) => b.status === status);
        rows = [...rows].sort(rank);
        if (cursor) {
          rows = rows.filter(
            (b) =>
              b.timestamp < cursor.sort_val ||
              (b.timestamp === cursor.sort_val && b.bid_id > cursor.id)
          );
        }
        const hasMore = rows.length > limit;
        const data = rows.slice(0, limit);
        const last = data[data.length - 1];
        return {
          data,
          has_more: hasMore,
          next_cursor:
            hasMore && last
              ? encodeCursor({ id: last.bid_id, sort_val: Number(last.timestamp) })
              : null,
        };
      }
    ),
    createBid: jest.fn(async (input: any) => {
      const bid = {
        bid_id: input.bid_id,
        invoice_id: input.invoice_id,
        investor: input.investor,
        bid_amount: input.bid_amount,
        expected_return: input.expected_return,
        timestamp: input.timestamp,
        status: "Placed",
        expiration_timestamp: input.expiration_timestamp,
        contract_version: 1,
        event_schema_version: 1,
        indexed_at: new Date().toISOString(),
      };
      bids.push(bid);
      return bid;
    }),
    getBestBid: jest.fn(async (invoiceId: string) => {
      const rows = bids
        .filter((b) => b.invoice_id === invoiceId && b.status === "Placed")
        .sort(rank);
      return rows[0] ?? null;
    }),
    getRankedBids: jest.fn(async (invoiceId: string, limit = 100) =>
      bids
        .filter((b) => b.invoice_id === invoiceId && b.status === "Placed")
        .sort(rank)
        .slice(0, limit)
    ),
    __resetBids: () => {
      bids = ALL_TENANT_BIDS.map((b: any) => ({ ...b }));
    },
  };

  return { bidStore, BidStore: bidStore };
});

// ─── Test Setup & Teardown ───────────────────────────────────────────────────

const TEST_DB_DIR = path.resolve(__dirname, "../.data");
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-tenant-isolation-${crypto.randomUUID()}.db`);
const ORIGINAL_DATABASE_PATH = config.DATABASE_PATH;

beforeAll(() => {
  // Point the shared SQLite singleton at an isolated database and seed it.
  fs.mkdirSync(TEST_DB_DIR, { recursive: true });
  (config as { DATABASE_PATH: string }).DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();
  createTenantSchema();
  seedTenantInvoices();

  // Populate the legacy mock arrays used by the export pipeline / exposure gate.
  MOCK_INVOICES.length = 0;
  MOCK_INVOICES.push(...ALL_TENANT_INVOICES);
  MOCK_BIDS.length = 0;
  MOCK_BIDS.push(...ALL_TENANT_BIDS);

  // exposureService captures MOCK_BIDS at construction; because this suite
  // loads the app (and thus the bids controller) before exposureService, the
  // circular import leaves that capture undefined. Re-point the singleton at
  // the live fixtures via the designed dependency hook (same pattern as
  // src/tests/exposure.test.ts) so the POST /bids exposure gate reads them.
  exposureService.setDependencies({
    mockBids: MOCK_BIDS,
    mockSettlements: MOCK_SETTLEMENTS,
    fetchPersistedBids: async () => [],
    fetchPersistedSettlements: async () => [],
  });

  // Drive the real auth middleware with the fixture keys.
  setApiKeyVerifier(async (token: string) => API_KEY_BY_TOKEN[token] ?? null);
});

afterAll(() => {
  setApiKeyVerifier(null);
  closeDatabase();
  (config as { DATABASE_PATH: string }).DATABASE_PATH = ORIGINAL_DATABASE_PATH;
  MOCK_INVOICES.length = 0;
  MOCK_BIDS.length = 0;
  for (const suffix of ["", "-wal", "-shm"]) {
    try {
      fs.unlinkSync(TEST_DB_PATH + suffix);
    } catch {
      // best-effort cleanup
    }
  }
});

beforeEach(() => {
  (bidStore as any).__resetBids();
  (bidStore.createBid as jest.Mock).mockClear();
});

// ─── Invoice List Endpoint Isolation Tests ──────────────────────────────────

describe("GET /api/v1/invoices - List Endpoint Tenant Isolation", () => {
  test("Business A can only see their own invoices", async () => {
    const response = await request(app)
      .get("/api/v1/invoices")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`)
      .query({ business: TENANT_BUSINESS_A });

    expect(response.status).toBe(200);
    expect(Array.isArray(response.body.data)).toBe(true);

    const invoiceIds = response.body.data.map((inv: any) => inv.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_A_1.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_A_2.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_B_1.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_B_2.id);

    response.body.data.forEach((inv: any) => {
      expect(inv.business).toBe(TENANT_BUSINESS_A);
    });
  });

  test("Business B can only see their own invoices", async () => {
    const response = await request(app)
      .get("/api/v1/invoices")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_B}`)
      .query({ business: TENANT_BUSINESS_B });

    expect(response.status).toBe(200);
    const invoiceIds = response.body.data.map((inv: any) => inv.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_B_1.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_B_2.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_A_1.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_A_2.id);

    response.body.data.forEach((inv: any) => {
      expect(inv.business).toBe(TENANT_BUSINESS_B);
    });
  });

  test("Business A requesting Business B's invoices returns empty list (not error)", async () => {
    const response = await request(app)
      .get("/api/v1/invoices")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`)
      .query({ business: TENANT_BUSINESS_B });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual([]);
    expect(response.body.has_more).toBe(false);
  });

  test("Unfiltered authenticated list is auto-scoped to the caller's tenant", async () => {
    const response = await request(app)
      .get("/api/v1/invoices")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`);

    expect(response.status).toBe(200);
    const invoiceIds = response.body.data.map((inv: any) => inv.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_A_1.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_A_2.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_B_1.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_B_2.id);
    response.body.data.forEach((inv: any) => {
      expect(inv.business).toBe(TENANT_BUSINESS_A);
    });
  });

  test("Unauthenticated public list still returns the full marketplace", async () => {
    const response = await request(app).get("/api/v1/invoices");

    expect(response.status).toBe(200);
    const invoiceIds = response.body.data.map((inv: any) => inv.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_A_1.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_B_1.id);
    expect(response.body.data.length).toBe(ALL_TENANT_INVOICES.length);
  });
});

// ─── Invoice Detail Endpoint Isolation Tests ─────────────────────────────────

describe("GET /api/v1/invoices/:id - Detail Endpoint Tenant Isolation", () => {
  test("Business A can retrieve their own invoice by ID", async () => {
    const response = await request(app)
      .get(`/api/v1/invoices/${INVOICE_BUSINESS_A_1.id}`)
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`);

    expect(response.status).toBe(200);
    expect(response.body.id).toBe(INVOICE_BUSINESS_A_1.id);
    expect(response.body.business).toBe(TENANT_BUSINESS_A);
  });

  test("Business B can retrieve their own invoice by ID", async () => {
    const response = await request(app)
      .get(`/api/v1/invoices/${INVOICE_BUSINESS_B_1.id}`)
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_B}`);

    expect(response.status).toBe(200);
    expect(response.body.id).toBe(INVOICE_BUSINESS_B_1.id);
    expect(response.body.business).toBe(TENANT_BUSINESS_B);
  });

  test("Business A accessing Business B's invoice returns 404 (prevents enumeration)", async () => {
    const response = await request(app)
      .get(`/api/v1/invoices/${INVOICE_BUSINESS_B_1.id}`)
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`);

    expect(response.status).toBe(404);
    expect(response.body.error.code).toBe("INVOICE_NOT_FOUND");
    expect(response.body.error.message).toBe("Invoice not found");
    expect(response.body.error.message).not.toMatch(/forbidden|permission|unauthorized/i);
  });

  test("Business B accessing Business A's invoice returns 404", async () => {
    const response = await request(app)
      .get(`/api/v1/invoices/${INVOICE_BUSINESS_A_1.id}`)
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_B}`);

    expect(response.status).toBe(404);
    expect(response.body.error.code).toBe("INVOICE_NOT_FOUND");
  });

  test("Investor accessing a business invoice returns 404 (role-based isolation)", async () => {
    const response = await request(app)
      .get(`/api/v1/invoices/${INVOICE_BUSINESS_A_1.id}`)
      .set("Authorization", `Bearer ${TOKEN_INVESTOR_A}`);

    expect(response.status).toBe(404);
    expect(response.body.error.code).toBe("INVOICE_NOT_FOUND");
  });

  test("Accessing non-existent invoice returns 404 with sanitized error", async () => {
    const fakeInvoiceId = "0x" + "de".repeat(32);
    const response = await request(app)
      .get(`/api/v1/invoices/${fakeInvoiceId}`)
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`);

    expect(response.status).toBe(404);
    expect(response.body.error.code).toBe("INVOICE_NOT_FOUND");
    expect(response.body).not.toHaveProperty("stack");
    expect(JSON.stringify(response.body)).not.toContain(".ts:");
  });

  test("Unauthenticated public detail read still returns the invoice", async () => {
    const response = await request(app).get(`/api/v1/invoices/${INVOICE_BUSINESS_B_1.id}`);

    expect(response.status).toBe(200);
    expect(response.body.id).toBe(INVOICE_BUSINESS_B_1.id);
  });
});

// ─── Bid List Endpoint Isolation Tests ──────────────────────────────────────

describe("GET /api/v1/bids - Bid List Endpoint Tenant Isolation", () => {
  test("Investor A can only see their own bids", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .set("Authorization", `Bearer ${TOKEN_INVESTOR_A}`)
      .query({ invoice_id: INVOICE_BUSINESS_A_1.id, investor: TENANT_INVESTOR_A });

    expect(response.status).toBe(200);
    const bidIds = response.body.data.map((bid: any) => bid.bid_id);
    expect(bidIds).toContain(BID_INVESTOR_A_ON_BUSINESS_A.bid_id);
    expect(bidIds).not.toContain(BID_INVESTOR_B_ON_BUSINESS_B.bid_id);
    response.body.data.forEach((bid: any) => {
      expect(bid.investor).toBe(TENANT_INVESTOR_A);
    });
  });

  test("Investor B can only see their own bids", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .set("Authorization", `Bearer ${TOKEN_INVESTOR_B}`)
      .query({ invoice_id: INVOICE_BUSINESS_B_1.id, investor: TENANT_INVESTOR_B });

    expect(response.status).toBe(200);
    const bidIds = response.body.data.map((bid: any) => bid.bid_id);
    expect(bidIds).toContain(BID_INVESTOR_B_ON_BUSINESS_B.bid_id);
    expect(bidIds).not.toContain(BID_INVESTOR_A_ON_BUSINESS_B.bid_id);
    response.body.data.forEach((bid: any) => {
      expect(bid.investor).toBe(TENANT_INVESTOR_B);
    });
  });

  test("Investor A querying Investor B's bids returns empty results", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .set("Authorization", `Bearer ${TOKEN_INVESTOR_A}`)
      .query({ invoice_id: INVOICE_BUSINESS_B_1.id, investor: TENANT_INVESTOR_B });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual([]);
  });

  test("Business owner querying bids on their invoice sees all bids (authorized)", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`)
      .query({ invoice_id: INVOICE_BUSINESS_A_1.id });

    expect(response.status).toBe(200);
    const bidIds = response.body.data.map((bid: any) => bid.bid_id);
    expect(bidIds).toContain(BID_INVESTOR_A_ON_BUSINESS_A.bid_id);
  });

  test("Business owner querying bids on another business's invoice returns empty list", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`)
      .query({ invoice_id: INVOICE_BUSINESS_B_1.id });

    expect(response.status).toBe(200);
    expect(response.body.data).toEqual([]);
  });

  test("Missing invoice_id parameter returns 400 error", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .set("Authorization", `Bearer ${TOKEN_INVESTOR_A}`);

    expect(response.status).toBe(400);
    expect(response.body.error.message).toContain("invoice_id");
  });

  test("Unauthenticated public bid list returns all bids for the invoice", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .query({ invoice_id: INVOICE_BUSINESS_B_1.id });

    expect(response.status).toBe(200);
    const bidIds = response.body.data.map((bid: any) => bid.bid_id);
    expect(bidIds).toContain(BID_INVESTOR_B_ON_BUSINESS_B.bid_id);
    expect(bidIds).toContain(BID_INVESTOR_A_ON_BUSINESS_B.bid_id);
  });
});

// ─── Export Endpoint Isolation Tests ─────────────────────────────────────────

describe("Export Data Tenant Isolation", () => {
  test("Business A export contains only their invoices", async () => {
    const data = await exportService.getUserData(TENANT_BUSINESS_A);

    expect(data.invoices.length).toBeGreaterThan(0);
    const invoiceIds = data.invoices.map((inv: any) => inv.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_A_1.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_A_2.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_B_1.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_B_2.id);
    data.invoices.forEach((inv: any) => {
      expect(inv.business).toBe(TENANT_BUSINESS_A);
    });
  });

  test("Business B export contains only their invoices", async () => {
    const data = await exportService.getUserData(TENANT_BUSINESS_B);

    const invoiceIds = data.invoices.map((inv: any) => inv.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_B_1.id);
    expect(invoiceIds).toContain(INVOICE_BUSINESS_B_2.id);
    expect(invoiceIds).not.toContain(INVOICE_BUSINESS_A_1.id);
    data.invoices.forEach((inv: any) => {
      expect(inv.business).toBe(TENANT_BUSINESS_B);
    });
  });

  test("Investor A export contains only their bids", async () => {
    const data = await exportService.getUserData(TENANT_INVESTOR_A);

    expect(data.bids.length).toBeGreaterThan(0);
    const bidIds = data.bids.map((bid: any) => bid.bid_id);
    expect(bidIds).toContain(BID_INVESTOR_A_ON_BUSINESS_A.bid_id);
    expect(bidIds).toContain(BID_INVESTOR_A_ON_BUSINESS_B.bid_id);
    expect(bidIds).not.toContain(BID_INVESTOR_B_ON_BUSINESS_B.bid_id);
    data.bids.forEach((bid: any) => {
      expect(bid.investor).toBe(TENANT_INVESTOR_A);
    });
  });

  test("Investor B export contains only their bids", async () => {
    const data = await exportService.getUserData(TENANT_INVESTOR_B);

    const bidIds = data.bids.map((bid: any) => bid.bid_id);
    expect(bidIds).toContain(BID_INVESTOR_B_ON_BUSINESS_B.bid_id);
    expect(bidIds).not.toContain(BID_INVESTOR_A_ON_BUSINESS_A.bid_id);
    data.bids.forEach((bid: any) => {
      expect(bid.investor).toBe(TENANT_INVESTOR_B);
    });
  });

  test("Export with context injection protection throws error", async () => {
    await expect(
      exportService.getUserData(TENANT_BUSINESS_B, {
        authenticatedUserId: TENANT_BUSINESS_A,
      })
    ).rejects.toThrow("Security violation");
  });

  test("Export with invalid userId throws error", async () => {
    await expect(exportService.getUserData("")).rejects.toThrow("Invalid userId");
    await expect(exportService.getUserData("   ")).rejects.toThrow("Invalid userId");
  });
});

// ─── Pagination Cursor Isolation Tests ──────────────────────────────────────

describe("Pagination Cursor Tenant Isolation", () => {
  test("Public cursor cannot be replayed against a different business filter", async () => {
    const responseA = await request(app)
      .get("/api/v1/invoices")
      .query({ business: TENANT_BUSINESS_A, limit: 1 });

    expect(responseA.status).toBe(200);
    const cursorA = responseA.body.next_cursor;
    expect(cursorA).toBeTruthy();

    const responseB = await request(app)
      .get("/api/v1/invoices")
      .query({ business: TENANT_BUSINESS_B, cursor: cursorA });

    expect(responseB.status).toBe(400);
    expect(responseB.body.error.code).toBe("INVALID_PAGINATION");
    expect(responseB.body.error.message).toMatch(/scope/i);
  });

  test("Authenticated cursor cannot be replayed across tenants", async () => {
    const responseA = await request(app)
      .get("/api/v1/invoices")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`)
      .query({ limit: 1 });

    expect(responseA.status).toBe(200);
    const cursorA = responseA.body.next_cursor;
    expect(cursorA).toBeTruthy();

    const responseB = await request(app)
      .get("/api/v1/invoices")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_B}`)
      .query({ cursor: cursorA });

    expect(responseB.status).toBe(400);
    expect(responseB.body.error.code).toBe("INVALID_PAGINATION");
  });

  test("Malformed cursor returns validation error without leaking system details", async () => {
    const response = await request(app)
      .get("/api/v1/invoices")
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`)
      .query({ cursor: "malformed_cursor_data" });

    expect(response.status).toBe(400);
    expect(response.body.error.code).toBe("INVALID_PAGINATION");
    expect(response.body).not.toHaveProperty("stack");
  });
});

// ─── Write-Path Tenant Binding Tests ─────────────────────────────────────────

describe("Write-Path Tenant Isolation", () => {
  test("POST /api/v1/bids without authentication returns 401", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .send({
        invoice_id: INVOICE_BUSINESS_A_1.id,
        bid_amount: "90000",
        expected_return: "100000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      });

    expect(response.status).toBe(401);
    expect(response.body.error.code).toBe("UNAUTHORIZED");
  });

  test("POST /api/v1/bids binds investor to the authenticated key (no spoofing)", async () => {
    const response = await request(app)
      .post("/api/v1/bids")
      .set("Authorization", `Bearer ${TOKEN_INVESTOR_A}`)
      .send({
        invoice_id: INVOICE_BUSINESS_A_1.id,
        bid_amount: "90000",
        expected_return: "100000",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
        // Attempt to attribute the bid to a different investor; must be ignored.
        investor: TENANT_INVESTOR_B,
      });

    expect(response.status).toBe(201);
    expect(response.body.data.investor).toBe(TENANT_INVESTOR_A);
    expect(response.body.data.investor).not.toBe(TENANT_INVESTOR_B);

    const createInput = (bidStore.createBid as jest.Mock).mock.calls[0][0];
    expect(createInput.investor).toBe(TENANT_INVESTOR_A);
    expect(createInput.created_by).toBe(TENANT_INVESTOR_A);
  });

  test("Owner-based authorization denies cross-tenant key revocation (FORBIDDEN → 403)", async () => {
    const registry = new Map<string, any>([
      [API_KEY_BUSINESS_A.id, API_KEY_BUSINESS_A],
      [API_KEY_BUSINESS_B.id, API_KEY_BUSINESS_B],
    ]);
    const authz = AuthorizationService.ownerBased(
      async (keyId: string) => registry.get(keyId)?.created_by ?? null
    );
    const revokeSpy = jest.fn(async (id: string, actor: string) => ({
      ...registry.get(id),
      status: "revoked",
      revokedAt: Date.now(),
      revokedBy: actor,
    }));
    const controller = new ApiKeysController(
      {
        getApiKey: async (id: string) => registry.get(id) ?? null,
        revokeApiKey: revokeSpy,
      },
      authz,
      new FrozenClock()
    );

    // Cross-tenant actor is denied terminally, and no mutation occurs.
    await expect(
      controller.revokeApiKey({
        apiKeyId: API_KEY_BUSINESS_A.id,
        actorId: TENANT_BUSINESS_B,
      })
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    expect(revokeSpy).not.toHaveBeenCalled();

    // The owner may revoke their own key.
    const result = await controller.revokeApiKey({
      apiKeyId: API_KEY_BUSINESS_A.id,
      actorId: TENANT_BUSINESS_A,
    });
    expect(result.success).toBe(true);
    expect(revokeSpy).toHaveBeenCalledTimes(1);
  });

  test("Owner-based policy only permits the revoke action for the owner", async () => {
    const authz = AuthorizationService.ownerBased(async () => TENANT_BUSINESS_A);

    expect(await authz.canAct(TENANT_BUSINESS_A, "revoke", API_KEY_BUSINESS_A.id)).toBe(true);
    expect(await authz.canAct(TENANT_BUSINESS_B, "revoke", API_KEY_BUSINESS_A.id)).toBe(false);
    expect(await authz.canAct(TENANT_BUSINESS_A, "rotate", API_KEY_BUSINESS_A.id)).toBe(false);
  });
});

// ─── Error Message Sanitization Tests ───────────────────────────────────────

describe("Error Message Sanitization - No Metadata Leakage", () => {
  test("404 errors never expose database IDs or foreign key constraints", async () => {
    const fakeId = "0x" + "99".repeat(32);
    const response = await request(app)
      .get(`/api/v1/invoices/${fakeId}`)
      .set("Authorization", `Bearer ${TOKEN_BUSINESS_A}`);

    expect(response.status).toBe(404);
    const responseText = JSON.stringify(response.body);
    expect(responseText).not.toMatch(/constraint/i);
    expect(responseText).not.toMatch(/foreign key/i);
    expect(responseText).not.toMatch(/primary key/i);
    expect(responseText).not.toMatch(/violation/i);
    expect(responseText).not.toMatch(/database/i);
    expect(responseText).not.toMatch(/table/i);
    expect(responseText).not.toMatch(/column/i);
  });

  test("Validation errors don't expose internal field names or schema details", async () => {
    const response = await request(app)
      .get("/api/v1/bids")
      .set("Authorization", `Bearer ${TOKEN_INVESTOR_A}`);

    expect(response.status).toBe(400);
    const responseText = JSON.stringify(response.body);
    expect(response.body.error.message).toContain("invoice_id");
    expect(responseText).not.toMatch(/schema/i);
    expect(responseText).not.toMatch(/validator/i);
    expect(responseText).not.toMatch(/zod/i);
  });

  test("Invalid token does not leak secret material in the response", async () => {
    const response = await request(app)
      .get("/api/v1/invoices")
      .set("Authorization", "Bearer invalid_token");

    const responseText = JSON.stringify(response.body);
    expect(responseText).not.toMatch(/hash/i);
    expect(responseText).not.toMatch(/secret/i);
    expect(responseText).not.toMatch(/password/i);
  });
});