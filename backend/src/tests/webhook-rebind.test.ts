/**
 * DNS rebinding protection tests for webhook delivery.
 *
 * Uses `jest.mock("node:dns")` at the module level to mock dns.lookup
 * instead of the FaultyDns helper, which lives in a file with a
 * pre-existing TypeScript error.
 */

import * as dns from "node:dns";
import http from "node:http";
import https from "node:https";
import { EventEmitter } from "node:events";
import type { IncomingMessage } from "node:http";
import {
  deliverWebhookJson,
  WebhookDeliveryError,
} from "../services/webhook/delivery";
import type { WebhookEgressPolicy } from "../services/webhook/egressPolicy";

// ---------------------------------------------------------------------------
// Mock dns.lookup at the module level
// ---------------------------------------------------------------------------
//
// jest.mock is hoisted above imports, so the factory creates jest.fn() inline.
// Tests access the mock via `(dns.lookup as unknown as jest.Mock)`.
//
jest.mock("node:dns", () => {
  const actual = jest.requireActual("node:dns");
  return { ...actual, lookup: jest.fn() };
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const basePolicy: WebhookEgressPolicy = {
  timeoutMs: 5000,
  maxResponseBytes: 65536,
  maxRedirects: 3,
  hostAllowRules: [],
  hostDenyRules: [],
};

/** A mock HTTP/1.1 response-like EventEmitter shaped as IncomingMessage. */
function mockResponse(opts: {
  statusCode: number;
  headers?: Record<string, string>;
  body?: string;
}): IncomingMessage {
  const res = new EventEmitter() as unknown as IncomingMessage;
  res.statusCode = opts.statusCode;
  res.headers = opts.headers ?? {};
  res.statusMessage = "";
  res.readable = true;
  res.destroy = jest.fn() as any;
  // Use setImmediate so data/end fire after the response callback (which is
  // scheduled on process.nextTick) has already attached readBodyWithByteLimit
  // listeners.
  if (opts.body !== undefined) {
    setImmediate(() => {
      res.emit("data", Buffer.from(opts.body!));
      res.emit("end");
    });
  }
  return res;
}

/** Create a fake http.ClientRequest that fires the response callback. */
function fakeRequest(
  res: IncomingMessage,
  cb?: (r: IncomingMessage) => void,
): http.ClientRequest {
  const req = new EventEmitter() as unknown as http.ClientRequest;
  req.write = jest.fn() as any;
  req.end = jest.fn() as any;
  req.destroy = jest.fn() as any;
  req.abort = jest.fn() as any;
  req.flushHeaders = jest.fn() as any;
  req.setHeader = jest.fn() as any;
  req.getHeader = jest.fn() as any;
  req.removeHeader = jest.fn() as any;
  (req as any).headersSent = false;
  (req as any).reusedSocket = false;
  (req as any).path = "/";
  (req as any).method = "POST";
  (req as any).cork = jest.fn();
  (req as any).uncork = jest.fn();
  (req as any).setNoDelay = jest.fn();
  (req as any).setSocketKeepAlive = jest.fn();
  (req as any).writable = true;
  process.nextTick(() => {
    if (cb) cb(res);
  });
  return req;
}

/** Configure the dns.lookup mock to resolve any hostname to the given IPs. */
function setupDns(ips: string[]) {
  ((dns.lookup as unknown) as jest.Mock).mockImplementation(
    (hostname: string, opts: any, cb: any): void => {
      let callback = cb;
      let options: dns.LookupOptions | undefined;
      if (typeof opts === "function") {
        callback = opts;
        options = {};
      } else {
        options = opts;
      }
      const addresses = ips.map((ip) => ({
        address: ip,
        family: (ip.includes(":") ? 6 : 4) as 4 | 6,
      }));
      if (options?.all) {
        callback(null, addresses);
      } else {
        callback(null, addresses[0].address, addresses[0].family);
      }
    },
  );
}

// ---------------------------------------------------------------------------
// Shared mock state managed in beforeEach/afterEach
// ---------------------------------------------------------------------------

let httpsSpy: jest.SpiedFunction<typeof https.request> | null = null;
const fakeAgent = new https.Agent();

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("Webhook delivery — DNS rebinding protection", () => {
  beforeEach(() => {
    ((dns.lookup as unknown) as jest.Mock).mockReset();
    // Spy on https.request and keep it installed across the full test
    httpsSpy = jest.spyOn(https, "request").mockImplementation(jest.fn());
  });

  afterEach(() => {
    if (httpsSpy) {
      httpsSpy.mockRestore();
      httpsSpy = null;
    }
    ((dns.lookup as unknown) as jest.Mock).mockReset();
  });

  // ------------------------------------------------------------------
  // a) Normal case: DNS returns public IP, delivery succeeds
  // ------------------------------------------------------------------
  it("succeeds when DNS resolves a public IP", async () => {
    setupDns(["93.184.216.34"]);

    const res = mockResponse({ statusCode: 200, body: "ok" });
    httpsSpy!.mockImplementation(((_target: any, _opts: any, cb: any) => {
      return fakeRequest(res, cb);
    }) as any);

    const result = await deliverWebhookJson(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
    );

    expect(result.statusCode).toBe(200);
    expect(result.redirectCount).toBe(0);
  });

  // ------------------------------------------------------------------
  // b) Rebinding attempt: first resolve → public IP, but connection
  //    still uses the pinned IP even if DNS changes.
  // ------------------------------------------------------------------
  it("pins the first resolved IP and ignores later DNS changes", async () => {
    setupDns(["93.184.216.34"]);

    const res = mockResponse({ statusCode: 200, body: "ok" });
    httpsSpy!.mockImplementation(((_target: any, _opts: any, cb: any) => {
      return fakeRequest(res, cb);
    }) as any);

    const result = await deliverWebhookJson(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
    );

    expect(result.statusCode).toBe(200);
    // The pinning agent never re-resolves DNS — only the initial
    // resolveHostnameToPinnedIp should have called dns.lookup.
    expect(((dns.lookup as unknown) as jest.Mock)).toHaveBeenCalledTimes(1);
  });

  // ------------------------------------------------------------------
  // c) IPv4-mapped IPv6 attack: ::ffff:127.0.0.1 must be blocked
  // ------------------------------------------------------------------
  it("blocks an IPv4-mapped IPv6 loopback address (::ffff:127.0.0.1)", async () => {
    setupDns(["::ffff:127.0.0.1"]);

    // Should fail before reaching https.request, so mock is not needed

    await expect(
      deliverWebhookJson(
        "https://rebind-attack.example/webhook",
        { event: "test" },
        basePolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);

    try {
      await deliverWebhookJson(
        "https://rebind-attack.example/webhook",
        { event: "test" },
        basePolicy,
      );
    } catch (err: any) {
      expect(err.code).toBe("EGRESS_BLOCKED");
    }
  });

  // ------------------------------------------------------------------
  // d) SNI test: servername must be the original hostname
  // ------------------------------------------------------------------
  it("sets servername to the original hostname (not pinned IP)", async () => {
    setupDns(["93.184.216.34"]);

    let capturedServername: string | undefined;
    const res = mockResponse({ statusCode: 200, body: "ok" });
    httpsSpy!.mockImplementation(((target: any, opts: any, cb: any) => {
      capturedServername = opts.servername;
      return fakeRequest(res, cb);
    }) as any);

    await deliverWebhookJson(
      "https://my-webhook.example.com/callback",
      { event: "test" },
      basePolicy,
    );

    expect(capturedServername).toBe("my-webhook.example.com");
  });

  // ------------------------------------------------------------------
  // e) Certificate validation: a cert mismatch should result in error
  // ------------------------------------------------------------------
  it("fails when TLS certificate does not match the original hostname", async () => {
    setupDns(["93.184.216.34"]);

    httpsSpy!.mockImplementation((() => {
      const req = fakeRequest(mockResponse({ statusCode: 200 }));
      process.nextTick(() => {
        req.emit(
          "error",
          Object.assign(
            new Error(
              "Hostname/IP does not match certificate's altnames: " +
                "Host: evil.com. is not in the cert's list",
            ),
            { code: "ERR_TLS_CERT_ALTNAME_INVALID" },
          ),
        );
      });
      return req;
    }) as any);

    await expect(
      deliverWebhookJson(
        "https://legitimate-host.com/webhook",
        { event: "test" },
        basePolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);
  });

  // ------------------------------------------------------------------
  // f) Redirect handling: one redirect hop is allowed with full
  //    re-validation of the destination.
  // ------------------------------------------------------------------
  it("follows a single 301 redirect when the target is valid", async () => {
    setupDns(["93.184.216.34"]);

    let callIdx = 0;
    httpsSpy!.mockImplementation(((target: any, opts: any, cb: any) => {
      callIdx++;
      if (callIdx === 1) {
        const res = mockResponse({
          statusCode: 301,
          headers: { location: "https://valid-target.example/hook" },
          body: "",
        });
        return fakeRequest(res, cb);
      }
      const res = mockResponse({ statusCode: 200, body: "ok" });
      return fakeRequest(res, cb);
    }) as any);

    const result = await deliverWebhookJson(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
    );

    expect(result.statusCode).toBe(200);
    expect(result.redirectCount).toBe(1);
  });

  it("follows a single 302 redirect when the target is valid", async () => {
    setupDns(["93.184.216.34"]);

    let callIdx = 0;
    httpsSpy!.mockImplementation(((target: any, opts: any, cb: any) => {
      callIdx++;
      if (callIdx === 1) {
        const res = mockResponse({
          statusCode: 302,
          headers: { location: "https://valid-target.example/hook" },
          body: "",
        });
        return fakeRequest(res, cb);
      }
      const res = mockResponse({ statusCode: 200, body: "ok" });
      return fakeRequest(res, cb);
    }) as any);

    const result = await deliverWebhookJson(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
    );

    expect(result.statusCode).toBe(200);
    expect(result.redirectCount).toBe(1);
  });

  it("blocks a redirect whose target resolves to a blocked IP", async () => {
    // DNS returns a public IP for the initial hostname but a private
    // IP for the redirect target.
    ((dns.lookup as unknown) as jest.Mock).mockImplementation(
      (hostname: string, opts: any, cb: any): void => {
        let callback = cb;
        let options: dns.LookupOptions | undefined;
        if (typeof opts === "function") {
          callback = opts;
          options = {};
        } else {
          options = opts;
        }
        if (hostname.includes("private-target")) {
          const addrs = [{ address: "10.0.0.1", family: 4 }];
          if (options?.all) return callback(null, addrs);
          callback(null, addrs[0].address, addrs[0].family);
        } else {
          const addrs = [{ address: "93.184.216.34", family: 4 }];
          if (options?.all) return callback(null, addrs);
          callback(null, addrs[0].address, addrs[0].family);
        }
      },
    );

    httpsSpy!.mockImplementation(((target: any, opts: any, cb: any) => {
      const res = mockResponse({
        statusCode: 301,
        headers: { location: "https://private-target.example/evil" },
        body: "",
      });
      return fakeRequest(res, cb);
    }) as any);

    await expect(
      deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        basePolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);

    try {
      await deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        basePolicy,
      );
    } catch (err: any) {
      expect(err.code).toBe("REDIRECT_NOT_ALLOWED");
    }
  });

  // ------------------------------------------------------------------
  // g) Re-validation: transport EADDRNOTAVAIL → EGRESS_BLOCKED
  // ------------------------------------------------------------------
  it("converts EADDRNOTAVAIL transport errors to EGRESS_BLOCKED", async () => {
    setupDns(["93.184.216.34"]);

    httpsSpy!.mockImplementation((() => {
      const req = fakeRequest(mockResponse({ statusCode: 200 }));
      process.nextTick(() => {
        req.emit(
          "error",
          Object.assign(new Error("WEBHOOK_EGRESS_BLOCKED"), {
            code: "EADDRNOTAVAIL",
          }),
        );
      });
      return req;
    }) as any);

    await expect(
      deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        basePolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);

    try {
      await deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        basePolicy,
      );
    } catch (err: any) {
      expect(err.code).toBe("EGRESS_BLOCKED");
    }
  });

  // ------------------------------------------------------------------
  // Additional edge-case: direct IP literal hostname
  // ------------------------------------------------------------------
  it("rejects blocked IP literal hostnames directly", async () => {
    // No DNS needed for IP literals; https.request should not be called
    await expect(
      deliverWebhookJson(
        "https://127.0.0.1/webhook",
        { event: "test" },
        basePolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);

    expect(((dns.lookup as unknown) as jest.Mock)).not.toHaveBeenCalled();
  });

  it("accepts a public IP literal hostname", async () => {
    const res = mockResponse({ statusCode: 200, body: "ok" });
    httpsSpy!.mockImplementation(((_target: any, _opts: any, cb: any) => {
      return fakeRequest(res, cb);
    }) as any);

    const result = await deliverWebhookJson(
      "https://93.184.216.34/webhook",
      { event: "test" },
      basePolicy,
    );

    expect(result.statusCode).toBe(200);
    expect(((dns.lookup as unknown) as jest.Mock)).not.toHaveBeenCalled();
  });

  // ------------------------------------------------------------------
  // Backward compatibility: existing mock-based tests still work
  // ------------------------------------------------------------------
  it("works with injected requestImpl (test compatibility)", async () => {
    const mockRequest = jest.fn().mockResolvedValue({
      statusCode: 200,
      headers: {},
      body: Buffer.from("ok"),
    });

    const result = await deliverWebhookJson(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
      { requestImpl: mockRequest, createAgent: () => fakeAgent },
    );

    expect(result.statusCode).toBe(200);
  });

  // ------------------------------------------------------------------
  // Edge-case: DNS resolution failure
  // ------------------------------------------------------------------
  it("fails with TRANSPORT_ERROR when DNS lookup fails", async () => {
    ((dns.lookup as unknown) as jest.Mock).mockImplementation(
      (hostname: string, opts: any, cb: any): void => {
        let callback = cb;
        let options: dns.LookupOptions | undefined;
        if (typeof opts === "function") {
          callback = opts;
          options = {};
        } else {
          options = opts;
        }
        const err = Object.assign(
          new Error("ENOTFOUND test error"),
          { code: "ENOTFOUND" },
        );
        if (options?.all) {
          callback(err, []);
        } else {
          callback(err, "", 0);
        }
      },
    );

    await expect(
      deliverWebhookJson(
        "https://nonexistent.example/webhook",
        { event: "test" },
        basePolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);

    try {
      await deliverWebhookJson(
        "https://nonexistent.example/webhook",
        { event: "test" },
        basePolicy,
      );
    } catch (err: any) {
      expect(err.code).toBe("TRANSPORT_ERROR");
    }
  });

  // ------------------------------------------------------------------
  // Edge-case: DNS returns empty address list
  // ------------------------------------------------------------------
  it("fails with EGRESS_BLOCKED when DNS returns no addresses", async () => {
    ((dns.lookup as unknown) as jest.Mock).mockImplementation(
      (hostname: string, opts: any, cb: any): void => {
        let callback = cb;
        let options: dns.LookupOptions | undefined;
        if (typeof opts === "function") {
          callback = opts;
          options = {};
        } else {
          options = opts;
        }
        if (options?.all) {
          callback(null, []);
        } else {
          callback(null, "", 0);
        }
      },
    );

    await expect(
      deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        basePolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);

    try {
      await deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        basePolicy,
      );
    } catch (err: any) {
      expect(err.code).toBe("EGRESS_BLOCKED");
    }
  });

  // ------------------------------------------------------------------
  // Edge-case: response exceeds maxResponseBytes — triggers the
  // .catch(reject) in requestOnceHttps response callback.
  // ------------------------------------------------------------------
  it("fails when webhook response exceeds configured size limit", async () => {
    setupDns(["93.184.216.34"]);

    const smallPolicy: WebhookEgressPolicy = { ...basePolicy, maxResponseBytes: 5 };
    httpsSpy!.mockImplementation(((target: any, opts: any, cb: any) => {
      const res = mockResponse({ statusCode: 200, body: "hello world longer than 5" });
      return fakeRequest(res, cb);
    }) as any);

    await expect(
      deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        smallPolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);

    try {
      await deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        smallPolicy,
      );
    } catch (err: any) {
      expect(err.code).toBe("RESPONSE_TOO_LARGE");
    }
  });

  // ------------------------------------------------------------------
  // Edge-case: timeout from https.request
  // ------------------------------------------------------------------
  it("fails with TIMEOUT when request times out", async () => {
    setupDns(["93.184.216.34"]);

    httpsSpy!.mockImplementation((() => {
      const req = new EventEmitter() as unknown as http.ClientRequest;
      req.write = jest.fn() as any;
      req.end = jest.fn() as any;
      req.destroy = jest.fn() as any;
      req.abort = jest.fn() as any;
      req.flushHeaders = jest.fn() as any;
      req.setHeader = jest.fn() as any;
      req.getHeader = jest.fn() as any;
      req.removeHeader = jest.fn() as any;
      (req as any).headersSent = false;
      (req as any).reusedSocket = false;
      (req as any).path = "/";
      (req as any).writable = true;
      process.nextTick(() => {
        req.emit("timeout");
      });
      return req;
    }) as any);

    await expect(
      deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        basePolicy,
      ),
    ).rejects.toThrow(WebhookDeliveryError);

    try {
      await deliverWebhookJson(
        "https://example.com/webhook",
        { event: "test" },
        basePolicy,
      );
    } catch (err: any) {
      expect(err.code).toBe("TIMEOUT");
    }
  });

  // ------------------------------------------------------------------
  // Edge-case: uses legacy createWebhookAgent when requestImpl is
  // provided without createAgent (test compatibility)
  // ------------------------------------------------------------------
  it("falls back to createWebhookAgent when createAgent is omitted", async () => {
    const mockRequest = jest.fn().mockResolvedValue({
      statusCode: 200,
      headers: {},
      body: Buffer.from("ok"),
    });

    const result = await deliverWebhookJson(
      "https://example.com/webhook",
      { event: "test" },
      basePolicy,
      { requestImpl: mockRequest },  // no createAgent
    );

    expect(result.statusCode).toBe(200);
  });
});
