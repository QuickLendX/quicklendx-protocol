/**
 * Request ID propagation — end-to-end.
 *
 * Verifies the chain described in docs/logging.md:
 *
 *   inbound X-Request-Id header
 *     → request-logger opens an AsyncLocalStorage context
 *       → auditService.append() stamps AuditEntry.requestId
 *       → rpcClient.call() forwards X-Request-Id to upstream Soroban RPC
 *
 * Also asserts the context survives async boundaries, never cross-contaminates
 * between concurrent requests, and does not leak across event-loop ticks.
 */
import {
  describe,
  expect,
  it,
  jest,
  beforeAll,
  afterAll,
  beforeEach,
} from "@jest/globals";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

import { createRequestLogger, Logger } from "../src/middleware/request-logger";
import { getCorrelationId } from "../src/lib/requestContext";
import { auditService } from "../src/services/auditService";
import { rpcClient } from "../src/services/rpcClient";
import { AuditEntry } from "../src/types/audit";

const ULID_RE = /^[0-9A-HJKMNP-TV-Z]{26}$/;

// ── Test doubles ──────────────────────────────────────────────────────────────

/** Logger that swallows output so tests don't spam stdout. */
const silentLogger: Logger = { info: () => {}, error: () => {} };

interface FakeRes {
  statusCode: number;
  setHeader(name: string, value: string): void;
  getHeader(name: string): string | undefined;
  json(body: unknown): unknown;
  on(event: string, cb: () => void): FakeRes;
  emit(event: string): void;
}

function makeReq(headers: Record<string, unknown> = {}) {
  return {
    method: "POST",
    path: "/api/v1/test",
    query: {} as Record<string, unknown>,
    body: {} as Record<string, unknown>,
    headers,
  };
}

function makeRes(): FakeRes {
  const headers: Record<string, string> = {};
  const listeners: Record<string, Array<() => void>> = {};
  return {
    statusCode: 200,
    setHeader(name, value) {
      headers[name.toLowerCase()] = value;
    },
    getHeader(name) {
      return headers[name.toLowerCase()];
    },
    json(body) {
      return body;
    },
    on(event, cb) {
      (listeners[event] ??= []).push(cb);
      return this;
    },
    emit(event) {
      (listeners[event] ?? []).forEach((cb) => cb());
    },
  };
}

/**
 * Drive the request-logger middleware and run `work` inside the established
 * request context. Resolves once `work` completes. Returns the FakeRes so the
 * echoed X-Request-Id response header can be asserted.
 */
function runRequest(
  headers: Record<string, unknown>,
  work: () => void | Promise<void>
): Promise<FakeRes> {
  const middleware = createRequestLogger(silentLogger);
  const req = makeReq(headers);
  const res = makeRes();
  return new Promise<FakeRes>((resolve, reject) => {
    middleware(req as never, res as never, () => {
      Promise.resolve()
        .then(work)
        .then(() => {
          res.emit("finish"); // exercise the structured-log finish handler
          resolve(res);
        })
        .catch(reject);
    });
  });
}

/** Append a minimal audit entry (requestId is filled from context). */
function appendAudit(effect: string): AuditEntry {
  return auditService.append({
    actor: "test-actor",
    operation: "CONFIG_CHANGE",
    params: {},
    redactedParams: {},
    ip: "10.0.0.1",
    userAgent: "jest",
    effect,
    success: true,
  });
}

// ── fetch capture for rpcClient outbound headers ──────────────────────────────

let capturedHeaders: Array<Record<string, string>>;
let originalFetch: typeof globalThis.fetch;

function installFetchSpy() {
  originalFetch = globalThis.fetch;
  globalThis.fetch = jest.fn(async (_url: unknown, init?: unknown) => {
    const headers = ((init as { headers?: Record<string, string> })?.headers ??
      {}) as Record<string, string>;
    capturedHeaders.push(headers);
    return {
      ok: true,
      status: 200,
      statusText: "OK",
      json: async () => ({ jsonrpc: "2.0", id: 1, result: { healthy: true } }),
    } as Response;
  }) as unknown as typeof globalThis.fetch;
}

// ── Suite ─────────────────────────────────────────────────────────────────────

describe("request id propagation", () => {
  let auditDir: string;

  beforeAll(() => {
    auditDir = fs.mkdtempSync(path.join(os.tmpdir(), "qlx-audit-"));
    auditService.setAuditDir(auditDir);
    installFetchSpy();
  });

  afterAll(() => {
    globalThis.fetch = originalFetch;
    fs.rmSync(auditDir, { recursive: true, force: true });
  });

  beforeEach(() => {
    auditService.clearAll();
    capturedHeaders = [];
  });

  it("flows from inbound header to audit entry and outbound RPC header", async () => {
    const inbound = "client-trace-123";
    let entry: AuditEntry | undefined;

    const res = await runRequest({ "x-request-id": inbound }, async () => {
      expect(getCorrelationId()).toBe(inbound);
      entry = appendAudit("config changed");
      await rpcClient.call("getHealth");
    });

    // echoed back to the caller
    expect(res.getHeader("X-Request-Id")).toBe(inbound);
    // stamped on the audit entry
    expect(entry?.requestId).toBe(inbound);
    // forwarded to the upstream RPC
    expect(capturedHeaders).toHaveLength(1);
    expect(capturedHeaders[0]["X-Request-Id"]).toBe(inbound);
  });

  it("trims and accepts whitespace-padded inbound ids", async () => {
    let entry: AuditEntry | undefined;
    const res = await runRequest({ "x-request-id": "  padded-id-9  " }, () => {
      entry = appendAudit("padded");
    });
    expect(res.getHeader("X-Request-Id")).toBe("padded-id-9");
    expect(entry?.requestId).toBe("padded-id-9");
  });

  it("generates a server-side ULID when the inbound id is missing", async () => {
    let observed: string | null = null;
    let entry: AuditEntry | undefined;

    const res = await runRequest({}, () => {
      observed = getCorrelationId() ?? null;
      entry = appendAudit("generated");
    });

    expect(observed).toMatch(ULID_RE);
    expect(res.getHeader("X-Request-Id")).toBe(observed);
    expect(entry?.requestId).toBe(observed);
  });

  it("generates a fresh ULID when the inbound id is invalid (log injection)", async () => {
    const malicious = "bad\nid; rm -rf /";
    let observed: string | null = null;

    const res = await runRequest({ "x-request-id": malicious }, () => {
      observed = getCorrelationId() ?? null;
    });

    expect(observed).toMatch(ULID_RE);
    expect(res.getHeader("X-Request-Id")).not.toBe(malicious);
  });

  it("exposes the same id to async callbacks (await, setTimeout, setImmediate)", async () => {
    const inbound = "async-trace-7";
    const seen: Array<string | null> = [];

    await runRequest({ "x-request-id": inbound }, async () => {
      await Promise.resolve();
      seen.push(getCorrelationId() ?? null);

      await new Promise<void>((resolve) =>
        setTimeout(() => {
          seen.push(getCorrelationId() ?? null);
          resolve();
        }, 5)
      );

      await new Promise<void>((resolve) =>
        setImmediate(() => {
          // audit append inside a deferred callback still sees the id
          seen.push(appendAudit("deferred").requestId ?? null);
          resolve();
        })
      );
    });

    expect(seen).toEqual([inbound, inbound, inbound]);
  });

  it("does not cross-contaminate concurrent requests", async () => {
    const results: Record<string, { audit?: string; header?: string }> = {
      "req-aaa-1": {},
      "req-bbb-2": {},
      "req-ccc-3": {},
    };

    await Promise.all(
      Object.keys(results).map((id) =>
        runRequest({ "x-request-id": id }, async () => {
          // jitter so the requests genuinely interleave
          await new Promise((r) => setTimeout(r, Math.floor(Math.random() * 10)));
          results[id].audit = appendAudit(`work ${id}`).requestId;
          const before = capturedHeaders.length;
          await rpcClient.call("getHealth");
          results[id].header = capturedHeaders[before]["X-Request-Id"];
        })
      )
    );

    for (const id of Object.keys(results)) {
      expect(results[id].audit).toBe(id);
      expect(results[id].header).toBe(id);
    }

    // every audit entry on disk carries exactly one of the three ids
    const persisted = auditService.getAllEntries();
    expect(persisted).toHaveLength(3);
    expect(persisted.map((e) => e.requestId).sort()).toEqual(
      Object.keys(results).sort()
    );
  });

  it("does not leak context across event-loop ticks", async () => {
    expect(getCorrelationId()).toBeUndefined();

    await runRequest({ "x-request-id": "leak-check-1" }, () => {
      expect(getCorrelationId()).toBe("leak-check-1");
    });

    // immediately after the request completes the context is gone
    expect(getCorrelationId()).toBeUndefined();

    // and a tick scheduled entirely outside any request sees no context
    await new Promise<void>((resolve) =>
      setImmediate(() => {
        expect(getCorrelationId()).toBeUndefined();
        resolve();
      })
    );
  });

  it("omits requestId and the RPC header when there is no request context", async () => {
    const entry = appendAudit("no context");
    expect(entry.requestId).toBeUndefined();

    await rpcClient.call("getHealth");
    expect(capturedHeaders).toHaveLength(1);
    expect(capturedHeaders[0]["X-Request-Id"]).toBeUndefined();
  });
});
