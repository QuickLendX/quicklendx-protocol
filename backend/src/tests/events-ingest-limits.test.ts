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

import crypto from "crypto";
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

// The ingest endpoint requires an HMAC-SHA256 signature over the raw body.
const INDEXER_INGEST_SECRET = "test-indexer-ingest-secret-0123456789abcdef";

const signIngest = (
  body: string,
  timestamp: number = Date.now(),
  secret: string = INDEXER_INGEST_SECRET
) => {
  const signature = crypto
    .createHmac("sha256", secret)
    .update(`${timestamp}.${body}`)
    .digest("hex");
  return {
    "X-QLX-Indexer-Timestamp": String(timestamp),
    "X-QLX-Indexer-Signature": signature,
  };
};

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

  const originalSecret = process.env.INDEXER_INGEST_SECRET;

  beforeEach(() => {
    delete process.env[CHUNKED_PROXY_ALLOWLIST_ENV];
    process.env.INDEXER_INGEST_SECRET = INDEXER_INGEST_SECRET;
    statusService.setMockCurrentLedger(100000);
    statusService.updateLastIndexedLedger(100000);
  });

  afterEach(() => {
    statusService.setMockCurrentLedger(null);
  });

  afterAll(() => {
    if (originalAllowlist === undefined) {
      delete process.env[CHUNKED_PROXY_ALLOWLIST_ENV];
    } else {
      process.env[CHUNKED_PROXY_ALLOWLIST_ENV] = originalAllowlist;
    }
    if (originalSecret === undefined) {
      delete process.env.INDEXER_INGEST_SECRET;
    } else {
      process.env.INDEXER_INGEST_SECRET = originalSecret;
    }
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
      expect(outcome.body?.error?.code).toBe("CHUNKED_ENCODING_NOT_ALLOWED_");
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
        next(new Error("unexpected"));
      });

      const res = await supertest(buildParserApp(failingParser))
        .post("/events")
        .set("Content-Type", "application/json")
        .send("{}");

      expect(res.status).toBe(500);
      expect(res.body.error.code).toBe("FORWARDED_TO_ERROR_HANDLER");
    });
  });
});
