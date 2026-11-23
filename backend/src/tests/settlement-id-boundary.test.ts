import express from "express";
import request from "supertest";
import { getSettlementById } from "../controllers/v1/settlements";
import { errorHandler } from "../middleware/error-handler";
import { settlementOrchestrator } from "../services/settlementOrchestrator";

jest.mock("../services/settlementOrchestrator", () => ({
  settlementOrchestrator: { getById: jest.fn() },
}));
jest.mock("../services/freshnessService", () => ({ freshnessService: {} }));

const ID = "stl_01ARZ3NDEKTSV4RRFFQ69G5FAV";
const settlement = Object.freeze({
  id: ID,
  status: "Pending",
  amount: "100",
  timestamp: 1700000000,
});
const lookup = jest.mocked(settlementOrchestrator.getById);

// Exercise the assertion's controller caller and real error/cache middleware.
// The legacy production route's separate hex-only Zod schema is not part of
// this fixture; this suite is intentionally a controller-boundary integration.
function createApp() {
  const app = express();
  app.get("/settlements/:id", getSettlementById);
  app.use(errorHandler);
  return app;
}

beforeEach(() => {
  lookup.mockReset();
  lookup.mockImplementation((id) =>
    id === ID
      ? (settlement as ReturnType<typeof settlementOrchestrator.getById>)
      : null,
  );
  jest.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => jest.restoreAllMocks());

describe("settlement ID controller failure boundaries", () => {
  it("looks up a valid settlement once and preserves the stored record", async () => {
    const response = await request(createApp()).get(`/settlements/${ID}`);
    expect(response.status).toBe(200);
    expect(response.body).toEqual(settlement);
    expect(lookup).toHaveBeenCalledTimes(1);
    expect(lookup).toHaveBeenCalledWith(ID);
    expect(settlement).toEqual({
      id: ID,
      status: "Pending",
      amount: "100",
      timestamp: 1700000000,
    });
  });

  it("uses the same trimmed ID for validation and lookup", async () => {
    const response = await request(createApp()).get(
      `/settlements/${encodeURIComponent(` \t${ID} \n`)}`,
    );
    expect(response.status).toBe(200);
    expect(lookup).toHaveBeenCalledWith(ID);
  });

  it.each([
    "private-invalid-id",
    "stl_80000000000000000000000000",
    "stl_' OR secret='private'",
    `${ID}\0`,
  ])(
    "rejects invalid input %# before storage and keeps errors safe",
    async (id) => {
      const response = await request(createApp()).get(
        `/settlements/${encodeURIComponent(id)}`,
      );
      expect(response.status).toBe(400);
      expect(response.body).toEqual({
        error: { message: "Invalid entity ID", code: "INVALID_ENTITY_ID" },
      });
      expect(lookup).not.toHaveBeenCalled();
      expect(JSON.stringify(response.body)).not.toContain("private");
      expect(
        JSON.stringify(jest.mocked(console.error).mock.calls),
      ).not.toContain("private");
    },
  );

  it("distinguishes a missing record from an invalid ID and allows recovery", async () => {
    lookup.mockReturnValueOnce(null);
    const app = createApp();
    const missing = await request(app).get(`/settlements/${ID}`);
    expect(missing.status).toBe(404);
    expect(missing.body.error.code).toBe("SETTLEMENT_NOT_FOUND");
    expect(missing.headers["cache-control"]).toBeUndefined();
    const recovered = await request(app).get(`/settlements/${ID}`);
    expect(recovered.status).toBe(200);
    expect(recovered.body).toEqual(settlement);
  });

  it("recovers after storage failure without mutating the settlement", async () => {
    lookup.mockImplementationOnce(() => {
      throw new Error("Storage temporarily unavailable");
    });
    const app = createApp();
    const failed = await request(app).get(`/settlements/${ID}`);
    expect(failed.status).toBe(500);
    const retried = await request(app).get(`/settlements/${ID}`);
    expect(retried.status).toBe(200);
    expect(retried.body).toEqual(settlement);
    expect(lookup).toHaveBeenCalledTimes(2);
  });

  it("keeps concurrent valid, duplicate and rejected lookups independent", async () => {
    const app = createApp();
    const responses = await Promise.all(
      Array.from({ length: 20 }, (_, index) =>
        request(app).get(
          `/settlements/${index % 2 ? "private-invalid-id" : ID}`,
        ),
      ),
    );
    responses.forEach((response, index) => {
      expect(response.status).toBe(index % 2 ? 400 : 200);
      if (index % 2 === 0) expect(response.body).toEqual(settlement);
    });
    expect(lookup).toHaveBeenCalledTimes(10);
    for (const [id] of lookup.mock.calls) expect(id).toBe(ID);
  });

  it("validates before conditional caching and leaves retry data intact", async () => {
    const app = createApp();
    const initial = await request(app).get(`/settlements/${ID}`);
    expect(initial.headers.etag).toBeDefined();
    lookup.mockClear();
    const invalid = await request(app)
      .get("/settlements/private-invalid-id")
      .set("If-None-Match", initial.headers.etag);
    expect(invalid.status).toBe(400);
    expect(lookup).not.toHaveBeenCalled();
    const unchanged = await request(app)
      .get(`/settlements/${ID}`)
      .set("If-None-Match", initial.headers.etag);
    expect(unchanged.status).toBe(304);
    expect(lookup).toHaveBeenCalledWith(ID);
    const full = await request(app).get(`/settlements/${ID}`);
    expect(full.body).toEqual(settlement);
  });
});
