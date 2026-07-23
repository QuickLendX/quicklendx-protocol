import request from "supertest";
import app from "../src/app";
import { RATE_LIMIT_POLICIES } from "../src/middleware/rate-limit";
import { loadOpenApiDoc } from "../src/tests/helpers/openapi-loader";

const HTTP_METHODS = new Set(["get", "post", "put", "patch", "delete", "head", "options"]);

type RateLimitExtension = {
  policies: Array<keyof typeof RATE_LIMIT_POLICIES>;
};

function operations() {
  const doc = loadOpenApiDoc();
  return Object.entries(doc.paths).flatMap(([path, pathItem]) =>
    Object.entries(pathItem)
      .filter(([method]) => HTTP_METHODS.has(method))
      .map(([method, operation]) => ({
        path,
        method,
        operation: operation as { "x-rate-limit"?: RateLimitExtension },
      }))
  );
}

describe("rate-limit OpenAPI conformance", () => {
  it("documents an x-rate-limit extension on every operation", () => {
    for (const { path, method, operation } of operations()) {
      expect(operation["x-rate-limit"]).toBeDefined();
      expect(operation["x-rate-limit"]?.policies?.length).toBeGreaterThan(0);

      for (const policy of operation["x-rate-limit"]?.policies ?? []) {
        expect(RATE_LIMIT_POLICIES[policy]).toBeDefined();
      }

      expect(`${method.toUpperCase()} ${path}`).toBeTruthy();
    }
  });

  it("marks export operations with the stricter export policy", () => {
    const exportOperations = operations().filter(({ path }) => path.startsWith("/exports/"));
    expect(exportOperations.length).toBeGreaterThan(0);

    for (const { operation } of exportOperations) {
      expect(operation["x-rate-limit"]?.policies).toEqual(
        expect.arrayContaining(["global", "perKey", "export"])
      );
    }
  });

  it("exposes live limiter thresholds in GET /api/v1/status without bucket details", async () => {
    const res = await request(app).get("/api/v1/status");

    expect(res.status).toBe(200);
    expect(res.body.rateLimits.global.limit).toBe(RATE_LIMIT_POLICIES.global.limit);
    expect(res.body.rateLimits.perKey.limit).toBe(RATE_LIMIT_POLICIES.perKey.limit);
    expect(res.body.rateLimits.export.limit).toBe(RATE_LIMIT_POLICIES.export.limit);

    const serialized = JSON.stringify(res.body.rateLimits);
    expect(serialized).not.toContain("127.0.0.1");
    expect(serialized).not.toContain("remainingPoints");
    expect(serialized).not.toContain("consumedPoints");
    expect(serialized).not.toContain("msBeforeNext");
  });
});
