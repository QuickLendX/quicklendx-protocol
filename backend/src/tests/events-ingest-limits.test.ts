// Virtually mock the 'pg' module to prevent errors on environments where postgres is not installed
jest.mock("pg", () => {
  const mClient = {
    query: jest.fn().mockResolvedValue({ rows: [] }),
    release: jest.fn(),
  };
  const mPool = {
    connect: jest.fn().mockResolvedValue(mClient),
    query: jest.fn().mockResolvedValue({ rows: [] }),
    on: jest.fn(),
    end: jest.fn(),
  };
  return {
    Pool: jest.fn(() => mPool),
  };
}, { virtual: true });

import express, { NextFunction, Request, RequestHandler, Response } from "express";
import supertest from "supertest";
import app from "../app";
import { statusService } from "../services/statusService";
import {
  CHUNKED_PROXY_ALLOWLIST_ENV,
  CHUNKED_PROXY_HEADER,
  EVENT_INGEST_MAX_BODY_BYTES,
  EventIngestLimitsOptions,
  createEventIngestBodyParser,
  createEventIngestLimitsMiddleware,
  eventIngestBodyParser,
  eventIngestLimits,
  eventIngestLimitsMiddleware,
  isEventIngestRequest,
} from "../middleware/event-ingest-limits";

// Any string that appears in a rejection response would mean payload bytes are
// being echoed back to the caller.
const PAYLOAD_CANARY = "canary-payload-marker";

// The ingest endpoint is machine-to-machine; an API key exempts it from CSRF.
const INDEXER_API_KEY = "qlx_test_indexer_key";

interface GuardOutcome {
  status?: number;
  body?: { error?: { code?: string; message?: string } };
  nextCalled: boolean;
}

const runGuard = (
  headers: Record<string, string | string[]>,
  options?: EventIngestLimitsOptions
): GuardOutcome => {
  const middleware = options
    ? createEventIngestLimitsMiddleware(options)
    : eventIngestLimitsMiddleware;

  const outcome: GuardOutcome = { nextCalled: false };
  const res = {
    status(code: number) {
      outcome.status = code;
      return this;
    },
    json(body: unknown) {
      outcome.body = body as GuardOutcome["body"];
      return this;
    },
  };

  middleware({ method: "POST", headers } as unknown as Request, res as unknown as Response, () => {
    outcome.nextCalled = true;
  });

  return outcome;
};

const buildParserApp = (parser: RequestHandler) => {
  const testApp = express();
  testApp.post("/events", parser, (req: Request, res: Response) => {
    res.status(200).json({ parsed: true, rawBodyBytes: req.rawBody?.length ?? 0 });
  });
  testApp.use((err: Error, _req: Request, res: Response, _next: NextFunction) => {
    res.status(500).json({ error: { code: "FORWARDED_TO_ERROR_HANDLER" } });
  });
  return testApp;
};

const validEvent = (id: string) => ({
  id,
  ledger: 42,
  txHash: `tx-${id}`,
  timestamp: 1700000000,
  complianceHold: false,
  indexedAt: "2026-01-01T00:00:00.000Z",
  type: "InvoiceSettled",
  payload: {
    invoice_id: `inv-${id}`,
    business: "business-1",
    investor: "investor-1",
    amount: "1000",
  },
});

describe("Event ingest limits", () => {
  const originalAllowlist = process.env[CHUNKED_PROXY_ALLOWLIST_ENV];

  beforeEach(() => {
    delete process.env[CHUNKED_PROXY_ALLOWLIST_ENV];
    statusService.setMockCurrentLedger(100000);
    statusService.updateLastIndexedLedger(100000);
  });

  afterEach(() => {
    statusService.setMockCurrentLedger(null);
  });

  afterAll(() => {
    if (originalAllowlist === undefined) {
      delete process.env[CHUNKED_PROXY_ALLOWLIST_ENV];
      return;
    }
    process.env[CHUNKED_PROXY_ALLOWLIST_ENV] = originalAllowlist;
  });

  describe("content type", () => {
    it("rejects a missing Content-Type with 415", () => {
      const outcome = runGuard({ "content-length": "20" });

      expect(outcome.status).toBe(415);
      expect(outcome.body?.error?.code).toBe("INVALID_CONTENT_TYPE");
      expect(outcome.nextCalled).toBe(false);
    });

    it("rejects a non-JSON Content-Type with 415", () => {
      const outcome = runGuard({
        "content-type": "text/plain",
        "content-length": "20",
      });

      expect(outcome.status).toBe(415);
      expect(outcome.body?.error?.code).toBe("INVALID_CONTENT_TYPE");
    });

    it("rejects media types that merely start with application/json", () => {
      const outcome = runGuard({
        "content-type": "application/jsonrequest",
        "content-length": "20",
      });

      expect(outcome.status).toBe(415);
      expect(outcome.body?.error?.code).toBe("INVALID_CONTENT_TYPE");
    });

    it("accepts application/json with charset parameters", () => {
      const outcome = runGuard({
        "content-type": "Application/JSON; charset=utf-8",
        "content-length": "20",
      });

      expect(outcome.nextCalled).toBe(true);
      expect(outcome.status).toBeUndefined();
    });
  });

  describe("content length", () => {
    it("rejects a missing Content-Length with 411", () => {
      const outcome = runGuard({ "content-type": "application/json" });

      expect(outcome.status).toBe(411);
      expect(outcome.body?.error?.code).toBe("CONTENT_LENGTH_REQUIRED");
      expect(outcome.nextCalled).toBe(false);
    });

    it("rejects a non-numeric Content-Length with 400", () => {
      const outcome = runGuard({
        "content-type": "application/json",
        "content-length": "not-a-number",
      });

      expect(outcome.status).toBe(400);
      expect(outcome.body?.error?.code).toBe("INVALID_CONTENT_LENGTH");
    });

    it("rejects duplicated Content-Length headers with 400", () => {
      const outcome = runGuard({
        "content-type": "application/json",
        "content-length": ["12", "999"],
      });

      expect(outcome.status).toBe(400);
      expect(outcome.body?.error?.code).toBe("INVALID_CONTENT_LENGTH");
    });

    it("rejects a Content-Length above the 256KB budget with 413", () => {
      const outcome = runGuard({
        "content-type": "application/json",
        "content-length": String(EVENT_INGEST_MAX_BODY_BYTES + 1),
      });

      expect(outcome.status).toBe(413);
      expect(outcome.body?.error?.code).toBe("BODY_LIMIT_EXCEEDED");
    });

    it("accepts a Content-Length exactly at the budget", () => {
      const outcome = runGuard({
        "content-type": "application/json",
        "content-length": String(EVENT_INGEST_MAX_BODY_BYTES),
      });

      expect(outcome.nextCalled).toBe(true);
    });

    it("honours a caller-supplied budget", () => {
      const outcome = runGuard(
        { "content-type": "application/json", "content-length": "2048" },
        { maxBodyBytes: 1024 }
      );

      expect(outcome.status).toBe(413);
      expect(outcome.body?.error?.code).toBe("BODY_LIMIT_EXCEEDED");
    });
  });

  describe("transfer encoding", () => {
    it("rejects chunked encoding when no proxy header is present", () => {
      const outcome = runGuard({
        "content-type": "application/json",
        "transfer-encoding": "chunked",
      });

      expect(outcome.status).toBe(400);
      expect(outcome.body?.error?.code).toBe("CHUNKED_ENCODING_NOT_ALLOWED");
      expect(outcome.nextCalled).toBe(false);
    });

    it("rejects chunked encoding from a proxy that is not on the allowlist", () => {
      const outcome = runGuard(
        {
          "content-type": "application/json",
          "transfer-encoding": "chunked",
          [CHUNKED_PROXY_HEADER]: "rogue-proxy",
        },
        { allowedChunkedProxies: ["edge-proxy-1"] }
      );

      expect(outcome.status).toBe(400);
      expect(outcome.body?.error?.code).toBe("CHUNKED_ENCODING_NOT_ALLOWED");
    });

    it("accepts chunked encoding from an allowlisted proxy", () => {
      const outcome = runGuard(
        {
          "content-type": "application/json",
          "transfer-encoding": "gzip, chunked",
          [CHUNKED_PROXY_HEADER]: " edge-proxy-1 ",
        },
        { allowedChunkedProxies: ["edge-proxy-1"] }
      );

      expect(outcome.nextCalled).toBe(true);
      expect(outcome.status).toBeUndefined();
    });

    it("reads the allowlist from the environment when no option is given", () => {
      process.env[CHUNKED_PROXY_ALLOWLIST_ENV] = " edge-proxy-1 , edge-proxy-2 ";

      const outcome = runGuard({
        "content-type": "application/json",
        "transfer-encoding": ["chunked"],
        [CHUNKED_PROXY_HEADER]: "edge-proxy-2",
      });

      expect(outcome.nextCalled).toBe(true);
    });

    it("rejects chunked encoding combined with Content-Length as ambiguous framing", () => {
      const outcome = runGuard(
        {
          "content-type": "application/json",
          "transfer-encoding": "chunked",
          "content-length": "20",
          [CHUNKED_PROXY_HEADER]: "edge-proxy-1",
        },
        { allowedChunkedProxies: ["edge-proxy-1"] }
      );

      expect(outcome.status).toBe(400);
      expect(outcome.body?.error?.code).toBe("AMBIGUOUS_REQUEST_FRAMING");
      expect(outcome.nextCalled).toBe(false);
    });

    it("still requires Content-Length for non-chunked transfer encodings", () => {
      const outcome = runGuard({
        "content-type": "application/json",
        "transfer-encoding": "gzip",
      });

      expect(outcome.status).toBe(411);
      expect(outcome.body?.error?.code).toBe("CONTENT_LENGTH_REQUIRED");
    });
  });

  describe("body parser budget", () => {
    it("rejects a body above the budget with 413", async () => {
      const oversized = JSON.stringify([{ pad: "x".repeat(EVENT_INGEST_MAX_BODY_BYTES) }]);

      const res = await supertest(buildParserApp(eventIngestBodyParser))
        .post("/events")
        .set("Content-Type", "application/json")
        .send(oversized);

      expect(res.status).toBe(413);
      expect(res.body.error.code).toBe("BODY_LIMIT_EXCEEDED");
    });

    it("rejects malformed JSON without echoing payload bytes", async () => {
      const res = await supertest(buildParserApp(eventIngestBodyParser))
        .post("/events")
        .set("Content-Type", "application/json")
        .send(PAYLOAD_CANARY);

      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_JSON_BODY");
      expect(JSON.stringify(res.body)).not.toContain(PAYLOAD_CANARY);
    });

    it("parses a valid body and exposes the raw bytes", async () => {
      const body = JSON.stringify([validEvent("parser-ok")]);

      const res = await supertest(buildParserApp(eventIngestBodyParser))
        .post("/events")
        .set("Content-Type", "application/json")
        .send(body);

      expect(res.status).toBe(200);
      expect(res.body.rawBodyBytes).toBe(Buffer.byteLength(body));
    });

    it("maps a Content-Length mismatch to 400", async () => {
      const failingParser = createEventIngestBodyParser((_req, _res, next) => {
        next(Object.assign(new Error("size mismatch"), { type: "request.size.invalid" }));
      });

      const res = await supertest(buildParserApp(failingParser))
        .post("/events")
        .set("Content-Type", "application/json")
        .send("{}");

      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("CONTENT_LENGTH_MISMATCH");
    });

    it("forwards unrecognised parser errors to the error handler", async () => {
      const failingParser = createEventIngestBodyParser((_req, _res, next) => {
        next(new Error("stream aborted"));
      });

      const res = await supertest(buildParserApp(failingParser))
        .post("/events")
        .set("Content-Type", "application/json")
        .send("{}");

      expect(res.status).toBe(500);
      expect(res.body.error.code).toBe("FORWARDED_TO_ERROR_HANDLER");
    });
  });

  describe("guard chain", () => {
    const buildGuardedApp = () => {
      const testApp = express();
      testApp.post("/events", ...eventIngestLimits, (req: Request, res: Response) => {
        res.status(200).json({ accepted: true, events: req.body });
      });
      return testApp;
    };

    it("passes a valid, correctly framed request through to the handler", async () => {
      const events = [validEvent("chain-ok")];

      const res = await supertest(buildGuardedApp())
        .post("/events")
        .set("Content-Type", "application/json")
        .send(JSON.stringify(events));

      expect(res.status).toBe(200);
      expect(res.body).toEqual({ accepted: true, events });
    });

    it("rejects before parsing when the declared size exceeds the budget", async () => {
      const res = await supertest(buildGuardedApp())
        .post("/events")
        .set("Content-Type", "application/json")
        .send(JSON.stringify([{ pad: `${PAYLOAD_CANARY}${"x".repeat(EVENT_INGEST_MAX_BODY_BYTES)}` }]));

      expect(res.status).toBe(413);
      expect(res.body.error.code).toBe("BODY_LIMIT_EXCEEDED");
      expect(JSON.stringify(res.body)).not.toContain(PAYLOAD_CANARY);
    });
  });

  describe("isEventIngestRequest", () => {
    it.each([
      [{ method: "POST", path: "/api/v1/events" }, true],
      [{ method: "POST", path: "/api/v1/events/" }, true],
      [{ method: "POST", path: "/API/V1/EVENTS" }, true],
      [{ method: "GET", path: "/api/v1/events" }, false],
      [{ method: "POST", path: "/api/v1/invoices" }, false],
      [{ method: "POST", path: undefined }, false],
    ])("matches %p as %p", (candidate, expected) => {
      expect(isEventIngestRequest(candidate as unknown as Request)).toBe(expected);
    });
  });

  describe("POST /api/v1/events", () => {
    it("rejects a non-JSON content type with 415", async () => {
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("x-api-key", INDEXER_API_KEY)
        .set("Content-Type", "text/plain")
        .send(PAYLOAD_CANARY);

      expect(res.status).toBe(415);
      expect(res.body.error.code).toBe("INVALID_CONTENT_TYPE");
      expect(JSON.stringify(res.body)).not.toContain(PAYLOAD_CANARY);
    });

    it("rejects an oversized body with 413 before it is parsed", async () => {
      const oversized = JSON.stringify([
        { ...validEvent("too-big"), pad: `${PAYLOAD_CANARY}${"x".repeat(EVENT_INGEST_MAX_BODY_BYTES)}` },
      ]);

      const res = await supertest(app)
        .post("/api/v1/events")
        .set("x-api-key", INDEXER_API_KEY)
        .set("Content-Type", "application/json")
        .send(oversized);

      expect(res.status).toBe(413);
      expect(res.body.error.code).toBe("BODY_LIMIT_EXCEEDED");
      expect(JSON.stringify(res.body)).not.toContain(PAYLOAD_CANARY);
    });

    it("rejects a body between the route budget and the global 1MB budget", async () => {
      const body = JSON.stringify([{ pad: "x".repeat(400 * 1024) }]);
      expect(Buffer.byteLength(body)).toBeGreaterThan(EVENT_INGEST_MAX_BODY_BYTES);
      expect(Buffer.byteLength(body)).toBeLessThan(1024 * 1024);

      const res = await supertest(app)
        .post("/api/v1/events")
        .set("x-api-key", INDEXER_API_KEY)
        .set("Content-Type", "application/json")
        .send(body);

      expect(res.status).toBe(413);
      expect(res.body.error.code).toBe("BODY_LIMIT_EXCEEDED");
    });

    it("rejects malformed JSON without echoing payload bytes", async () => {
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("x-api-key", INDEXER_API_KEY)
        .set("Content-Type", "application/json")
        .send(`{"broken": ${PAYLOAD_CANARY}`);

      expect(res.status).toBe(400);
      expect(res.body.error.code).toBe("INVALID_JSON_BODY");
      expect(JSON.stringify(res.body)).not.toContain(PAYLOAD_CANARY);
    });

    it("lets a valid, correctly framed event batch reach the route handler", async () => {
      const res = await supertest(app)
        .post("/api/v1/events")
        .set("x-api-key", INDEXER_API_KEY)
        .set("Content-Type", "application/json")
        .send(JSON.stringify([validEvent(`ingest-ok-${Date.now()}`)]));

      // The handler envelope proves the guard chain forwarded the request; the
      // outcome of event processing itself is covered by the ingestion suites.
      expect([411, 413, 415]).not.toContain(res.status);
      expect(res.body).toHaveProperty("results");
      expect(res.body.results).toHaveLength(1);
    });
  });
});
