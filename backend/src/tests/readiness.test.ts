/**
 * Liveness and readiness probe tests.
 *
 * Covers:
 *  - Liveness (/health, /livez) is cheap, always 200, dependency-free.
 *  - Readiness (/readyz) probes DB connectivity, ingest lag, and the webhook
 *    queue, and honours maintenance mode.
 *  - Edge cases: DB down, high (critical) lag, maintenance mode, partial
 *    dependency failure, queue saturation.
 *  - Security: probes do not leak internal hostnames, versions, or error
 *    details to unauthenticated callers.
 */

import express from "express";
import supertest from "supertest";
import healthRoutes from "../routes/health";
import { statusService } from "../services/statusService";
import { lagMonitor } from "../services/lagMonitor";
import { webhookQueueService } from "../services/webhookQueueService";
import * as database from "../lib/database";

// Mount the health router the same way app.ts does: at the root, with no auth.
// Probes are unauthenticated, so no X-API-Key header is sent anywhere here.
// (We mount the router in isolation rather than importing the full app so the
// probe behaviour is exercised independently of the rest of the route graph.)
const app = express();
app.use(express.json());
app.use(healthRoutes);

const HEALTHY_QUEUE_STATS = {
  depth: 0,
  size: 0,
  capacity: 5000,
  overflowCount: 0,
  pendingCount: 0,
  successCount: 0,
  failureCount: 0,
  oldestTimestamp: null,
};

beforeEach(() => {
  // Healthy baseline: maintenance off, lag well under the warn threshold.
  statusService.setMaintenanceMode(false);
  statusService.updateLastIndexedLedger(100000);
  statusService.setMockCurrentLedger(100002); // lag = 2

  // The test database has no webhook_queue schema, so stub the queue stats to
  // a healthy value by default. Individual tests override this to exercise the
  // saturated / unavailable paths. This keeps the suite focused on probe logic
  // rather than queue persistence (covered by webhookQueue.persist.test.ts).
  jest
    .spyOn(webhookQueueService, "getStats")
    .mockReturnValue(HEALTHY_QUEUE_STATS as any);
});

afterEach(() => {
  statusService.setMaintenanceMode(false);
  statusService.setMockCurrentLedger(null);
  jest.restoreAllMocks();
});

// ---------------------------------------------------------------------------
// Liveness
// ---------------------------------------------------------------------------

describe("Liveness probe", () => {
  it("GET /health returns 200 with status ok", async () => {
    const res = await supertest(app).get("/health");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("ok");
    expect(res.body.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("GET /livez returns 200 with status ok", async () => {
    const res = await supertest(app).get("/livez");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("ok");
  });

  it("liveness stays up even when a dependency is down", async () => {
    jest.spyOn(database, "pingDatabase").mockReturnValue(false);
    const res = await supertest(app).get("/health");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("ok");
  });

  it("liveness is dependency-free (does not call pingDatabase)", async () => {
    const spy = jest.spyOn(database, "pingDatabase");
    await supertest(app).get("/livez");
    expect(spy).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Readiness — happy path
// ---------------------------------------------------------------------------

describe("Readiness probe — ready", () => {
  it("GET /readyz returns 200 when all dependencies are healthy", async () => {
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("ready");
    expect(res.body.database).toBe("ok");
    expect(res.body.ingest).toBe("ok");
    expect(res.body.webhookQueue).toBe("ok");
    expect(res.body.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it("stays ready with warn-level (degraded) lag", async () => {
    statusService.setMockCurrentLedger(100020); // lag = 20, >= warn(10), < critical(50)
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("ready");
    expect(res.body.ingest).toBe("degraded");
  });
});

// ---------------------------------------------------------------------------
// Readiness — maintenance mode
// ---------------------------------------------------------------------------

describe("Readiness probe — maintenance mode", () => {
  it("returns 503 with maintenance status when maintenance is enabled", async () => {
    statusService.setMaintenanceMode(true);
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(503);
    expect(res.body.status).toBe("maintenance");
  });

  it("short-circuits before probing dependencies", async () => {
    statusService.setMaintenanceMode(true);
    const spy = jest.spyOn(database, "pingDatabase");
    await supertest(app).get("/readyz");
    expect(spy).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Readiness — dependency failures
// ---------------------------------------------------------------------------

describe("Readiness probe — DB down", () => {
  it("returns 503 not_ready when the database is unreachable", async () => {
    jest.spyOn(database, "pingDatabase").mockReturnValue(false);
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(503);
    expect(res.body.status).toBe("not_ready");
    expect(res.body.database).toBe("unavailable");
  });
});

describe("Readiness probe — high lag", () => {
  it("returns 503 not_ready when ingest lag is critical", async () => {
    statusService.setMockCurrentLedger(100100); // lag = 100, >= critical(50)
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(503);
    expect(res.body.status).toBe("not_ready");
    expect(res.body.ingest).toBe("unavailable");
  });

  it("returns 503 not_ready when the lag probe throws", async () => {
    jest.spyOn(lagMonitor, "getLagStatus").mockRejectedValue(new Error("rpc down"));
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(503);
    expect(res.body.ingest).toBe("unavailable");
  });
});

describe("Readiness probe — webhook queue", () => {
  it("returns 503 not_ready when the queue store is unreachable", async () => {
    jest.spyOn(webhookQueueService, "getStats").mockImplementation(() => {
      throw new Error("queue store unavailable");
    });
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(503);
    expect(res.body.status).toBe("not_ready");
    expect(res.body.webhookQueue).toBe("unavailable");
  });

  it("stays ready (degraded) when the queue is saturated", async () => {
    jest.spyOn(webhookQueueService, "getStats").mockReturnValue({
      depth: 5000,
      size: 5000,
      capacity: 5000,
      overflowCount: 3,
      pendingCount: 5000,
      successCount: 0,
      failureCount: 0,
      oldestTimestamp: null,
    } as any);
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(200);
    expect(res.body.status).toBe("ready");
    expect(res.body.webhookQueue).toBe("degraded");
  });
});

describe("Readiness probe — partial dependency failure", () => {
  it("a single unavailable dependency fails readiness while others stay ok", async () => {
    jest.spyOn(database, "pingDatabase").mockReturnValue(false);
    statusService.setMockCurrentLedger(100002); // lag = 2, healthy
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(503);
    expect(res.body.status).toBe("not_ready");
    expect(res.body.database).toBe("unavailable");
    expect(res.body.ingest).toBe("ok");
    expect(res.body.webhookQueue).toBe("ok");
  });

  it("degraded lag plus DB down still reports both sub-statuses", async () => {
    jest.spyOn(database, "pingDatabase").mockReturnValue(false);
    statusService.setMockCurrentLedger(100020); // lag = 20, degraded
    const res = await supertest(app).get("/readyz");
    expect(res.status).toBe(503);
    expect(res.body.database).toBe("unavailable");
    expect(res.body.ingest).toBe("degraded");
  });
});

// ---------------------------------------------------------------------------
// Security — no information leakage to unauthenticated callers
// ---------------------------------------------------------------------------

describe("Readiness probe — does not leak internal details", () => {
  const sub = ["ok", "degraded", "unavailable"];

  it("readiness response contains only coarse status fields", async () => {
    const res = await supertest(app).get("/readyz");
    expect(Object.keys(res.body).sort()).toEqual(
      ["database", "ingest", "status", "timestamp", "webhookQueue"].sort()
    );
    // No version, hostname, ledger numbers, queue depths, or error strings.
    expect(res.body).not.toHaveProperty("version");
    expect(res.body).not.toHaveProperty("host");
    expect(res.body).not.toHaveProperty("lag");
    expect(res.body).not.toHaveProperty("error");
    expect(sub).toContain(res.body.database);
    expect(sub).toContain(res.body.ingest);
    expect(sub).toContain(res.body.webhookQueue);
  });

  it("does not surface the underlying error message when a dependency throws", async () => {
    jest
      .spyOn(lagMonitor, "getLagStatus")
      .mockRejectedValue(new Error("postgres://secret-host:5432 refused"));
    const res = await supertest(app).get("/readyz");
    const serialized = JSON.stringify(res.body);
    expect(serialized).not.toContain("secret-host");
    expect(serialized).not.toContain("postgres");
  });

  it("liveness response contains only status and timestamp", async () => {
    const res = await supertest(app).get("/health");
    expect(Object.keys(res.body).sort()).toEqual(["status", "timestamp"].sort());
    expect(res.body).not.toHaveProperty("version");
  });
});
