import supertest from "supertest";
import app from "../app";
import { apiKeyService } from "../services/api-key-service";
import { bidStore } from "../services/bidStore";
import {
  BID_READ_SCOPES,
  BID_WRITE_SCOPES,
  isPrivilegedBidReader,
  resolveBidReadIdentity,
} from "../middleware/bid-authorization";

/**
 * Authorization-boundary coverage for bid submission and auction selection.
 *
 * Regression context: `GET /api/v1/bids`, `/:invoiceId/best` and
 * `/:invoiceId/ranked` used to be served anonymously, so anyone could
 * enumerate every bid (amounts, expected returns, investor identity) on any
 * invoice, and could additionally pass an arbitrary `?investor=` value to
 * target a specific tenant's book. `POST /api/v1/bids` authenticated callers
 * but never checked that the key actually held `write:bids`, so a read-only
 * key could submit bids.
 *
 * These tests pin the boundary at both layers:
 *   - route middleware (401 anonymous, 403 wrong scope)
 *   - handler identity binding (403 cross-tenant filter, implicit self-scoping)
 */

// `getBidsQuerySchema` validates `investor` as a Stellar public key and
// `invoice_id` as hex, so the fixtures below must satisfy those shapes.
const INVOICE_HEX = "0xdead";
const INVOICE_ULID = "inv_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const CALLER = "G" + "A".repeat(55);
const OTHER_INVESTOR = "G" + "B".repeat(55);
const OPS_KEY_OWNER = "G" + "C".repeat(55);
const AUTH = "Bearer qlx_test_bid_read_key";

interface StubKey {
  created_by: string;
  scopes: string[];
}

let originalVerify: typeof apiKeyService.verifyApiKey;

function stubKey(key: StubKey) {
  (apiKeyService as any).verifyApiKey = async (candidate: string) => {
    if (!candidate.startsWith("qlx_")) return null;
    return {
      id: "test-key-id",
      key_hash: "hash",
      signing_secret_hash: null,
      prev_signing_secret_hash: null,
      prefix: candidate.slice(0, 15),
      name: "bid-authz-test",
      scopes: key.scopes,
      created_at: new Date(0).toISOString(),
      last_used_at: null,
      expires_at: null,
      prev_secret_expires_at: null,
      revoked: false,
      created_by: key.created_by,
    };
  };
}

beforeEach(() => {
  originalVerify = apiKeyService.verifyApiKey;
  stubKey({ created_by: CALLER, scopes: ["read:bids", "write:bids"] });
});

afterEach(() => {
  (apiKeyService as any).verifyApiKey = originalVerify;
  jest.restoreAllMocks();
});

// ───────────────────────────────────────────────────────────────────────────
// Pure boundary helper
// ───────────────────────────────────────────────────────────────────────────

describe("resolveBidReadIdentity", () => {
  const key = (created_by: string, scopes: string[]) =>
    ({ created_by, scopes } as any);

  it("rejects anonymous callers with 401", () => {
    const result = resolveBidReadIdentity(undefined);
    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected failure");
    expect(result.status).toBe(401);
    expect(result.code).toBe("UNAUTHORIZED");
  });

  it("rejects a key with no owner identity", () => {
    const result = resolveBidReadIdentity({ scopes: ["read:bids"] } as any);
    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected failure");
    expect(result.status).toBe(401);
  });

  it("scopes an unprivileged caller to itself even without a filter", () => {
    const result = resolveBidReadIdentity(key(CALLER, ["read:bids"]));
    expect(result).toEqual({
      ok: true,
      caller: CALLER,
      privileged: false,
      investorFilter: CALLER,
    });
  });

  it("allows an unprivileged caller to filter by its own identity", () => {
    const result = resolveBidReadIdentity(key(CALLER, ["read:bids"]), CALLER);
    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected success");
    expect(result.investorFilter).toBe(CALLER);
  });

  it("refuses identity spoofing with 403", () => {
    const result = resolveBidReadIdentity(
      key(CALLER, ["read:bids"]),
      OTHER_INVESTOR
    );
    expect(result.ok).toBe(false);
    if (result.ok) throw new Error("expected failure");
    expect(result.status).toBe(403);
    expect(result.code).toBe("FORBIDDEN");
  });

  it("lets a privileged caller read another investor", () => {
    const result = resolveBidReadIdentity(
      key(OPS_KEY_OWNER, ["read:*"]),
      OTHER_INVESTOR
    );
    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected success");
    expect(result.privileged).toBe(true);
    expect(result.investorFilter).toBe(OTHER_INVESTOR);
  });

  it("leaves a privileged caller unscoped when no filter is given", () => {
    const result = resolveBidReadIdentity(key(OPS_KEY_OWNER, ["admin:*"]));
    expect(result.ok).toBe(true);
    if (!result.ok) throw new Error("expected success");
    expect(result.investorFilter).toBeUndefined();
  });

  it("treats empty and whitespace filters as absent", () => {
    for (const value of ["", "   ", null, undefined]) {
      const result = resolveBidReadIdentity(key(CALLER, ["read:bids"]), value);
      expect(result.ok).toBe(true);
      if (!result.ok) throw new Error("expected success");
      expect(result.investorFilter).toBe(CALLER);
    }
  });

  it("does not treat read:bids as privileged", () => {
    expect(isPrivilegedBidReader(key(CALLER, ["read:bids"]))).toBe(false);
    expect(isPrivilegedBidReader(key(CALLER, ["write:bids"]))).toBe(false);
    expect(isPrivilegedBidReader(key(CALLER, ["read:*"]))).toBe(true);
    expect(isPrivilegedBidReader(key(CALLER, ["admin:keys"]))).toBe(true);
    expect(isPrivilegedBidReader(undefined)).toBe(false);
  });

  it("exports the scopes the routes require", () => {
    expect(BID_READ_SCOPES).toEqual(["read:bids"]);
    expect(BID_WRITE_SCOPES).toEqual(["write:bids"]);
  });
});

// ───────────────────────────────────────────────────────────────────────────
// Route-level enforcement
// ───────────────────────────────────────────────────────────────────────────

describe("bid read routes require authentication", () => {
  it("rejects an anonymous list read with 401", async () => {
    const res = await supertest(app).get(`/api/v1/bids?invoice_id=${INVOICE_HEX}`);
    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("UNAUTHORIZED");
  });

  it("rejects an anonymous best-bid read with 401", async () => {
    const res = await supertest(app).get(`/api/v1/bids/${INVOICE_ULID}/best`);
    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("UNAUTHORIZED");
  });

  it("rejects an anonymous ranked read with 401", async () => {
    const res = await supertest(app).get(`/api/v1/bids/${INVOICE_ULID}/ranked`);
    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("UNAUTHORIZED");
  });

  it("rejects a malformed Authorization header with 401", async () => {
    const res = await supertest(app)
      .get(`/api/v1/bids?invoice_id=${INVOICE_HEX}`)
      .set("Authorization", "Token qlx_test_bid_read_key");
    expect(res.status).toBe(401);
    expect(res.body.error.code).toBe("INVALID_AUTH_FORMAT");
  });
});

describe("bid read routes require the read:bids scope", () => {
  beforeEach(() => {
    stubKey({ created_by: CALLER, scopes: ["write:bids"] });
  });

  it("rejects a write-only key with 403", async () => {
    const res = await supertest(app)
      .get(`/api/v1/bids?invoice_id=${INVOICE_HEX}`)
      .set("Authorization", AUTH);
    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("FORBIDDEN");
  });

  it("rejects a write-only key on the auction-selection routes", async () => {
    const best = await supertest(app)
      .get(`/api/v1/bids/${INVOICE_ULID}/best`)
      .set("Authorization", AUTH);
    expect(best.status).toBe(403);
    const ranked = await supertest(app)
      .get(`/api/v1/bids/${INVOICE_ULID}/ranked`)
      .set("Authorization", AUTH);
    expect(ranked.status).toBe(403);
  });
});

describe("bid read identity binding", () => {
  it("binds an unfiltered query to the caller's own identity", async () => {
    const spy = jest
      .spyOn(bidStore, "getBidsPaginated")
      .mockResolvedValue({ data: [], next_cursor: null, has_more: false } as any);

    const res = await supertest(app)
      .get(`/api/v1/bids?invoice_id=${INVOICE_HEX}`)
      .set("Authorization", AUTH);

    expect(res.status).toBe(200);
    expect(spy).toHaveBeenCalledTimes(1);
    // 4th argument is the filter bag — it must be scoped to the caller, not
    // left open, otherwise the whole invoice bid book would be returned.
    expect(spy.mock.calls[0][3]).toMatchObject({ investor: CALLER });
  });

  it("rejects a cross-tenant investor filter with 403 and reads nothing", async () => {
    const spy = jest.spyOn(bidStore, "getBidsPaginated");

    const res = await supertest(app)
      .get(`/api/v1/bids?invoice_id=${INVOICE_HEX}&investor=${OTHER_INVESTOR}`)
      .set("Authorization", AUTH);

    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("FORBIDDEN");
    expect(spy).not.toHaveBeenCalled();
  });

  it("honours the filter when it matches the caller", async () => {
    const spy = jest
      .spyOn(bidStore, "getBidsPaginated")
      .mockResolvedValue({ data: [], next_cursor: null, has_more: false } as any);

    const res = await supertest(app)
      .get(`/api/v1/bids?invoice_id=${INVOICE_HEX}&investor=${CALLER}`)
      .set("Authorization", AUTH);

    expect(res.status).toBe(200);
    expect(spy.mock.calls[0][3]).toMatchObject({ investor: CALLER });
  });

  it("allows a privileged key to read another investor", async () => {
    stubKey({ created_by: OPS_KEY_OWNER, scopes: ["read:*"] });
    const spy = jest
      .spyOn(bidStore, "getBidsPaginated")
      .mockResolvedValue({ data: [], next_cursor: null, has_more: false } as any);

    const res = await supertest(app)
      .get(`/api/v1/bids?invoice_id=${INVOICE_HEX}&investor=${OTHER_INVESTOR}`)
      .set("Authorization", AUTH);

    expect(res.status).toBe(200);
    expect(spy.mock.calls[0][3]).toMatchObject({ investor: OTHER_INVESTOR });
  });

  it("still validates pagination for an authenticated caller", async () => {
    const res = await supertest(app)
      .get(`/api/v1/bids?invoice_id=${INVOICE_HEX}&limit=0`)
      .set("Authorization", AUTH);
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("still requires invoice_id", async () => {
    const res = await supertest(app)
      .get("/api/v1/bids")
      .set("Authorization", AUTH);
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("MISSING_REQUIRED_FIELD");
  });
});

describe("auction-selection routes for an authenticated caller", () => {
  it("returns 404 (not 401) when the invoice has no placed bids", async () => {
    const res = await supertest(app)
      .get(`/api/v1/bids/${INVOICE_ULID}/best`)
      .set("Authorization", AUTH);
    expect(res.status).toBe(404);
  });

  it("returns an empty ranked list rather than an auth error", async () => {
    const res = await supertest(app)
      .get(`/api/v1/bids/${INVOICE_ULID}/ranked`)
      .set("Authorization", AUTH);
    expect(res.status).toBe(200);
    expect(res.body.data).toEqual([]);
  });
});

describe("bid submission requires the write:bids scope", () => {
  beforeEach(() => {
    stubKey({ created_by: CALLER, scopes: ["read:bids"] });
  });

  it("rejects a read-only key with 403 before the body is processed", async () => {
    const res = await supertest(app)
      .post("/api/v1/bids")
      .set("Authorization", AUTH)
      .send({
        invoice_id: INVOICE_HEX,
        bid_amount: "100",
        expected_return: "150",
        expiration_timestamp: Math.floor(Date.now() / 1000) + 86400,
      });
    expect(res.status).toBe(403);
    expect(res.body.error.code).toBe("FORBIDDEN");
  });
});
