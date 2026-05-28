import express, { Request, Response } from "express";
import supertest from "supertest";
import { createRequestLogger } from "../middleware/request-logger";
import {
  getCorrelationId,
  sanitizeCorrelationId,
  withCorrelationId,
} from "../lib/requestContext";
import { deliverWebhookJson } from "../services/webhook/delivery";
import { WebhookEgressPolicy } from "../services/webhook/egressPolicy";
import { eventProcessor } from "../services/eventProcessor";

describe("Correlation ID support", () => {
  describe("request header handling", () => {
    it("accepts and echoes a valid client-supplied X-Request-Id", async () => {
      const app = express();
      app.use(express.json());
      app.use(createRequestLogger());
      app.get("/context", (req: Request, res: Response) => {
        res.json({
          correlationId: getCorrelationId(),
          requestId: (req as any).correlationId,
        });
      });

      const res = await supertest(app)
        .get("/context")
        .set("X-Request-Id", "client-123")
        .expect(200);

      expect(res.headers["x-request-id"]).toBe("client-123");
      expect(res.body.correlationId).toBe("client-123");
      expect(res.body.requestId).toBe("client-123");
    });

    it("rejects invalid client-supplied IDs and generates a new ULID", async () => {
      const app = express();
      app.use(express.json());
      app.use((req, _res, next) => {
        // Simulate a malformed header arriving from the raw request pipeline.
        (req.headers as any)["x-request-id"] = "bad\r\nid";
        next();
      });
      app.use(createRequestLogger());
      app.get("/context", (_req: Request, res: Response) => {
        res.json({ correlationId: getCorrelationId() });
      });

      const res = await supertest(app).get("/context").expect(200);

      expect(res.headers["x-request-id"]).toBeDefined();
      expect(res.headers["x-request-id"]).not.toBe("bad\r\nid");
      expect(typeof res.body.correlationId).toBe("string");
      expect(res.body.correlationId).toMatch(/^[A-Z0-9]{26}$/);
    });

    it("preserves asynchronous request context across async handlers", async () => {
      const app = express();
      app.use(express.json());
      app.use(createRequestLogger());
      app.get("/async-context", async (_req: Request, res: Response) => {
        await new Promise((resolve) => setTimeout(resolve, 5));
        res.json({ correlationId: getCorrelationId() });
      });

      const res = await supertest(app)
        .get("/async-context")
        .set("X-Request-Id", "async-test-1")
        .expect(200);

      expect(res.body.correlationId).toBe("async-test-1");
    });

    it("isolates correlation IDs between concurrent requests", async () => {
      const app = express();
      app.use(express.json());
      app.use(createRequestLogger());
      app.get("/async-context", async (_req: Request, res: Response) => {
        await new Promise((resolve) => setTimeout(resolve, 5));
        res.json({ correlationId: getCorrelationId() });
      });

      const [first, second] = await Promise.all([
        supertest(app).get("/async-context").set("X-Request-Id", "concurrent-1"),
        supertest(app).get("/async-context").set("X-Request-Id", "concurrent-2"),
      ]);

      expect(first.body.correlationId).toBe("concurrent-1");
      expect(second.body.correlationId).toBe("concurrent-2");
      expect(first.body.correlationId).not.toBe(second.body.correlationId);
    });
  });

  describe("context helpers", () => {
    it("sanitizes valid client-supplied correlation IDs", () => {
      expect(sanitizeCorrelationId("  client-123 ")).toBe("client-123");
    });

    it("rejects invalid correlation IDs with unsafe characters", () => {
      expect(sanitizeCorrelationId("bad\nvalue")).toBeNull();
      expect(sanitizeCorrelationId("bad|value")).toBeNull();
    });

    it("rejects oversized correlation IDs", () => {
      const longId = "a".repeat(129);
      expect(sanitizeCorrelationId(longId)).toBeNull();
    });
  });

  describe("service propagation", () => {
    it("propagates the current correlation ID into event processor logs", async () => {
      const logs: string[] = [];
      const originalConsoleLog = console.log;
      (console as any).log = (message?: any) => {
        logs.push(String(message));
      };

      try {
        await withCorrelationId("event-prop-123", async () => {
          await eventProcessor.processEvent({
            id: "evt_ignored",
            type: "UnsupportedEventType",
            timestamp: Date.now(),
          } as any);
        });

        expect(logs.some((line) => line.includes("[event-prop-123]") && line.includes("Unhandled event type"))).toBe(true);
      } finally {
        (console as any).log = originalConsoleLog;
      }
    });

    it("includes the correlation ID in outbound webhook delivery headers", async () => {
      const policy: WebhookEgressPolicy = {
        hostAllowRules: [],
        hostDenyRules: [],
        maxRedirects: 3,
        timeoutMs: 1000,
        maxResponseBytes: 65536,
      };

      await withCorrelationId("webhook-prop-123", async () => {
        await deliverWebhookJson(
          "https://example.com/webhook",
          { status: "ok" },
          policy,
          {
            createAgent: () => new (require("https").Agent)({ keepAlive: false, maxSockets: 1 }),
            requestImpl: async (target, jsonBody, _policy, agent) => {
              expect(target.href).toBe("https://example.com/webhook");
              expect(jsonBody).toContain('"status":"ok"');
              if (agent && typeof (agent as any).destroy === "function") {
                (agent as any).destroy();
              }
              return {
                statusCode: 200,
                headers: {},
                body: Buffer.from(""),
              };
            },
          },
        );
      });
    });
  });
});
