import { describe, expect, it, beforeEach, afterEach } from "@jest/globals";
import request from "supertest";
import { createApp } from "../src/index";
import { statusService, StatusService } from "../src/services/statusService";
import {
  WebhookQueueService,
  webhookQueueService,
} from "../src/services/webhookQueueService";
import { resetApiKeys } from "../src/middleware/apiKeyAuth";

function setTestEnv() {
  process.env.SKIP_API_KEY_AUTH = "true";
  process.env.TEST_ACTOR = "test-admin";
}

function clearTestEnv() {
  delete process.env.SKIP_API_KEY_AUTH;
  delete process.env.TEST_ACTOR;
}

describe("Admin Monitoring Endpoints", () => {
  let app: ReturnType<typeof createApp>;

  beforeEach(() => {
    setTestEnv();
    resetApiKeys();
    StatusService.getInstance().setMaintenanceMode(false);
    StatusService.getInstance().updateLastIndexedLedger(100000);
    StatusService.getInstance().setMockCurrentLedger(100005);
    WebhookQueueService.resetInstance();
    app = createApp();
  });

  afterEach(() => {
    clearTestEnv();
  });

  describe("Auth — all endpoints require X-API-Key", () => {
    const endpoints = [
      { method: "get", path: "/api/v1/admin/monitoring/health" },
      { method: "get", path: "/api/v1/admin/monitoring/cursor" },
      { method: "get", path: "/api/v1/admin/monitoring/invariants" },
      { method: "get", path: "/api/v1/admin/monitoring/webhook" },
      { method: "post", path: "/api/v1/admin/monitoring/webhook", body: { type: "test" } },
      { method: "post", path: "/api/v1/admin/monitoring/webhook/abc123/success" },
      { method: "post", path: "/api/v1/admin/monitoring/webhook/abc123/fail" },
    ];

    for (const ep of endpoints) {
      it(`${ep.method.toUpperCase()} ${ep.path} → 401 without X-API-Key`, async () => {
        delete process.env.SKIP_API_KEY_AUTH;
        resetApiKeys();
        const res =
          ep.method === "get"
            ? request(app).get(ep.path)
            : request(app).post(ep.path).send(ep.body as Record<string, unknown>);
        await expect(res).toBeDefined();
        const response = await res;
        expect(response.status).toBe(401);
        expect(response.body.error.code).toBe("UNAUTHORIZED");
      });
    }
  });

  describe("GET /health", () => {
    it("returns health with all sub-systems ok", async () => {
      const res = await request(app).get("/api/v1/admin/monitoring/health");
      expect(res.status).toBe(200);
      expect(res.body).toHaveProperty("status");
      expect(res.body).toHaveProperty("statusService");
      expect(res.body).toHaveProperty("webhookQueue");
      expect(res.body).toHaveProperty("invariants");
      expect(res.body).toHaveProperty("timestamp");
      expect(["ok", "degraded", "maintenance", "unavailable"]).toContain(
        res.body.status
      );
      expect(["ok", "degraded", "unavailable"]).toContain(res.body.statusService);
      expect(["ok", "degraded", "unavailable"]).toContain(res.body.webhookQueue);
      expect(["ok", "degraded", "unavailable"]).toContain(res.body.invariants);
    });

    it("returns maintenance when maintenance mode is enabled", async () => {
      statusService.setMaintenanceMode(true);
      const res = await request(app).get("/api/v1/admin/monitoring/health");
      expect(res.status).toBe(200);
      expect(res.body.status).toBe("maintenance");
    });
  });

  describe("GET /cursor", () => {
    it("returns lastIndexedLedger, currentLedger, and ingestLag", async () => {
      const res = await request(app).get("/api/v1/admin/monitoring/cursor");
      expect(res.status).toBe(200);
      expect(res.body).toHaveProperty("lastIndexedLedger");
      expect(res.body).toHaveProperty("currentLedger");
      expect(res.body).toHaveProperty("ingestLag");
      expect(res.body).toHaveProperty("timestamp");
      expect(typeof res.body.lastIndexedLedger).toBe("number");
      expect(typeof res.body.currentLedger).toBe("number");
      expect(typeof res.body.ingestLag).toBe("number");
    });

    it("computes ingestLag = currentLedger - lastIndexedLedger", async () => {
      statusService.updateLastIndexedLedger(100000);
      statusService.setMockCurrentLedger(100050);
      const res = await request(app).get("/api/v1/admin/monitoring/cursor");
      expect(res.status).toBe(200);
      expect(res.body.lastIndexedLedger).toBe(100000);
      expect(res.body.ingestLag).toBe(50);
    });

    it("handles zero lag", async () => {
      statusService.updateLastIndexedLedger(100000);
      statusService.setMockCurrentLedger(100000);
      const res = await request(app).get("/api/v1/admin/monitoring/cursor");
      expect(res.status).toBe(200);
      expect(res.body.ingestLag).toBe(0);
    });
  });

  describe("GET /invariants", () => {
    it("returns all four counters with counts and sampleIds", async () => {
      const res = await request(app).get("/api/v1/admin/monitoring/invariants");
      expect(res.status).toBe(200);
      expect(res.body).toHaveProperty("orphanBids");
      expect(res.body).toHaveProperty("orphanSettlements");
      expect(res.body).toHaveProperty("orphanDisputes");
      expect(res.body).toHaveProperty("mismatchSettlements");
      expect(res.body).toHaveProperty("timestamp");

      for (const key of [
        "orphanBids",
        "orphanSettlements",
        "orphanDisputes",
        "mismatchSettlements",
      ]) {
        expect(res.body[key]).toHaveProperty("count");
        expect(res.body[key]).toHaveProperty("sampleIds");
        expect(Array.isArray(res.body[key].sampleIds)).toBe(true);
        expect(res.body[key].sampleIds.length).toBeLessThanOrEqual(5);
      }
    });

    it("returns correct zero counts for clean mock data", async () => {
      const res = await request(app).get("/api/v1/admin/monitoring/invariants");
      expect(res.status).toBe(200);
      expect(res.body.orphanBids.count).toBe(0);
      expect(res.body.orphanSettlements.count).toBe(0);
      expect(res.body.orphanDisputes.count).toBe(0);
      expect(res.body.mismatchSettlements.count).toBe(0);
    });
  });

  describe("Webhook queue", () => {
    it("GET /webhook returns stats with zero counts", async () => {
      const res = await request(app).get("/api/v1/admin/monitoring/webhook");
      expect(res.status).toBe(200);
      expect(res.body).toHaveProperty("depth");
      expect(res.body).toHaveProperty("successCount");
      expect(res.body).toHaveProperty("failureCount");
      expect(res.body).toHaveProperty("overflowCount");
      expect(res.body).toHaveProperty("oldestTimestamp");
      expect(res.body.depth).toBe(0);
      expect(res.body.successCount).toBe(0);
      expect(res.body.failureCount).toBe(0);
      expect(res.body.overflowCount).toBe(0);
      expect(res.body.oldestTimestamp).toBeNull();
    });

    it("POST /webhook enqueues and returns 201 with id", async () => {
      const res = await request(app)
        .post("/api/v1/admin/monitoring/webhook")
        .send({ type: "invoice.funded", payload: { invoiceId: "0xabc" } });
      expect(res.status).toBe(201);
      expect(res.body).toHaveProperty("id");
      expect(res.body).toHaveProperty("enqueuedAt");
      expect(typeof res.body.id).toBe("string");
      expect(res.body.id.length).toBeGreaterThan(0);
    });

    it("POST /webhook returns 400 for missing type", async () => {
      const res = await request(app)
        .post("/api/v1/admin/monitoring/webhook")
        .send({});
      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_WEBHOOK_PAYLOAD");
    });

    it("POST /webhook increases queue depth", async () => {
      await request(app)
        .post("/api/v1/admin/monitoring/webhook")
        .send({ type: "test" });
      const statsRes = await request(app).get("/api/v1/admin/monitoring/webhook");
      expect(statsRes.body.depth).toBeGreaterThan(0);
    });

    it("POST /webhook/:id/success marks pending event as success", async () => {
      const enqueueRes = await request(app)
        .post("/api/v1/admin/monitoring/webhook")
        .send({ type: "invoice.paid" });
      const id = enqueueRes.body.id;

      const res = await request(app)
        .post(`/api/v1/admin/monitoring/webhook/${id}/success`);
      expect(res.status).toBe(200);
      expect(res.body.outcome).toBe("success");
    });

    it("POST /webhook/:id/fail marks pending event as failed", async () => {
      const enqueueRes = await request(app)
        .post("/api/v1/admin/monitoring/webhook")
        .send({ type: "invoice.paid" });
      const id = enqueueRes.body.id;

      const res = await request(app)
        .post(`/api/v1/admin/monitoring/webhook/${id}/fail`);
      expect(res.status).toBe(200);
      expect(res.body.outcome).toBe("failed");
    });

    it("POST /webhook/:id/success returns not_found for invalid id", async () => {
      const res = await request(app)
        .post("/api/v1/admin/monitoring/webhook/nonexistent-id/success");
      expect(res.status).toBe(200);
      expect(res.body.outcome).toBe("not_found_or_already_resolved");
    });

    it("POST /webhook/:id/fail returns not_found for invalid id", async () => {
      const res = await request(app)
        .post("/api/v1/admin/monitoring/webhook/nonexistent-id/fail");
      expect(res.status).toBe(200);
      expect(res.body.outcome).toBe("not_found_or_already_resolved");
    });
  });

  describe("Security — no sensitive data in responses", () => {
    it("health endpoint never exposes raw error messages", async () => {
      const res = await request(app).get("/api/v1/admin/monitoring/health");
      expect(res.status).toBe(200);
      const body = JSON.stringify(res.body);
      expect(body).not.toContain("stack");
      expect(body).not.toContain("Error:");
    });

    it("invariants endpoint never exposes full record payloads", async () => {
      const res = await request(app).get("/api/v1/admin/monitoring/invariants");
      expect(res.status).toBe(200);
      const body = JSON.stringify(res.body);
      expect(body).not.toContain("metadata");
      expect(body).not.toContain("line_items");
    });

    it("webhook queue GET never exposes event payloads", async () => {
      await request(app)
        .post("/api/v1/admin/monitoring/webhook")
        .send({
          type: "invoice.funded",
          payload: { secretToken: "super-secret" },
        });

      const res = await request(app).get("/api/v1/admin/monitoring/webhook");
      expect(res.status).toBe(200);
      const body = JSON.stringify(res.body);
      expect(body).not.toContain("secretToken");
      expect(body).not.toContain("super-secret");
    });

    it("cursor endpoint never exposes internal state values", async () => {
      const res = await request(app).get("/api/v1/admin/monitoring/cursor");
      expect(res.status).toBe(200);
      expect(res.body).not.toHaveProperty("mockCurrentLedger");
      expect(res.body).not.toHaveProperty("isMaintenanceMode");
    });
  });
});