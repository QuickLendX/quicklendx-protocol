import supertest from "supertest";
import app from "../app";

jest.mock("../middleware/api-key-auth", () => ({
  apiKeyAuthMiddleware: (req: any, res: any, next: any) => {
    req.apiKey = {
      id: "mock-key-id",
      key_hash: "mock-hash",
      scopes: ["read:invoices", "read:bids"],
      created_by: "mock-requester",
    };
    next();
  },
  optionalApiKeyAuth: (req: any, res: any, next: any) => {
    req.apiKey = {
      id: "mock-key-id",
      key_hash: "mock-hash",
      scopes: ["read:invoices", "read:bids"],
      created_by: "mock-requester",
    };
    next();
  },
  requireScopes: () => (req: any, res: any, next: any) => next(),
}));

describe("Cursor pagination endpoints", () => {
  it("returns 400 for limit=0 on invoices", async () => {
    const res = await supertest(app).get("/api/v1/invoices?limit=0");
    expect(res.status).toBe(400);
    expect(res.body).toHaveProperty("error");
    expect(res.body.error.code).toBe("INVALID_PAGINATION");
  });

  it("accepts limit over MAX_LIMIT for bids (no 400)", async () => {
    const res = await supertest(app).get("/api/v1/bids?invoice_id=0xdead&limit=100000");
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
    const res = await supertest(app).get("/api/v1/bids?invoice_id=0xdead&limit=50");
    expect(res.status).toBe(200);
    expect(res.body.has_more).toBe(false);
  });
});
