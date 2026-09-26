import supertest from "supertest";
import app from "../app";
import { apiKeyService } from "../services/api-key-service";

/**
 * Cursor pagination endpoints.
 *
 * Bid reads are authenticated (a valid key holding `read:bids` is required —
 * see `middleware/bid-authorization.ts`), so the bid cases below attach a
 * bearer token. The pagination contract itself is unchanged: `limit` may
 * exceed MAX_LIMIT without a 400, and `has_more` is false on the last page.
 */
describe("Cursor pagination endpoints", () => {
  const INVESTOR = "inv_test_pagination_caller";
  const AUTH = "Bearer qlx_test_pagination_key";
  let originalVerify: typeof apiKeyService.verifyApiKey;

  beforeAll(() => {
    originalVerify = apiKeyService.verifyApiKey;
    (apiKeyService as any).verifyApiKey = async (key: string) => {
      if (!key.startsWith("qlx_")) return null;
      return {
        id: "test-key-id",
        prefix: key.slice(0, 15),
        name: "pagination-test",
        created_by: INVESTOR,
        scopes: ["read:bids", "write:bids"],
        revoked: false,
      } as any;
    };
  });

  afterAll(() => {
    (apiKeyService as any).verifyApiKey = originalVerify;
  });

  it("returns 400 for limit=0 on invoices", async () => {
    const res = await supertest(app).get("/api/v1/invoices?limit=0");
    expect(res.status).toBe(400);
    expect(res.body).toHaveProperty("error");
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("accepts limit over MAX_LIMIT for bids (no 400)", async () => {
    const res = await supertest(app)
      .get("/api/v1/bids?invoice_id=0xdead&limit=100000")
      .set("Authorization", AUTH);
    expect(res.status).toBe(200);
    expect(res.body).toHaveProperty("data");
    expect(res.body).toHaveProperty("next_cursor");
    expect(res.body).toHaveProperty("has_more");
  });

  it("returns 400 for tampered cursor on invoices", async () => {
    const res = await supertest(app).get("/api/v1/invoices?cursor=not-a-valid-cursor!@#");
    expect(res.status).toBe(400);
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("returns has_more=false on last page for bids when small dataset", async () => {
    const res = await supertest(app)
      .get("/api/v1/bids?invoice_id=0xdead&limit=50")
      .set("Authorization", AUTH);
    expect(res.status).toBe(200);
    expect(res.body.has_more).toBe(false);
  });
});
