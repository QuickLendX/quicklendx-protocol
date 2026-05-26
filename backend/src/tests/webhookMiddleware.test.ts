/**
 * Unit tests for webhookVerifyMiddleware.
 *
 * The middleware imports `webhookSecretService` as a named export from the
 * service module.  We use jest.mock() with a module factory so every test
 * controls exactly what the service returns, without touching the real
 * singleton or needing getter spies.
 */

import { Request, Response, NextFunction } from "express";

// ---------------------------------------------------------------------------
// Mock the service module BEFORE importing the middleware.
// jest.mock is hoisted to the top of the file by Babel/ts-jest.
// ---------------------------------------------------------------------------

import {
  WebhookSecretService,
  WebhookSecretStore,
} from "../services/webhookSecretService";

// We create a real service backed by a fresh store for each test so that
// the mock returns realistic behaviour without hitting the global singleton.
let _service: WebhookSecretService;

jest.mock("../services/webhookSecretService", () => {
  // Re-require the real module so we get the actual classes.
  const real = jest.requireActual<typeof import("../services/webhookSecretService")>(
    "../services/webhookSecretService"
  );

  return {
    ...real,
    // Replace the singleton export with a proxy that always delegates to
    // whatever `_service` is set to at call time.
    get webhookSecretService() {
      return _service;
    },
  };
});

// Now import the middleware (it will pick up the mocked module above).
import { webhookVerifyMiddleware } from "../middleware/webhook-verify";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeReqResNext(opts: {
  headers?: Record<string, string>;
  body?: Buffer | string | object | undefined;
}): { req: Request; res: Response; next: NextFunction } {
  const req = {
    headers: opts.headers ?? {},
    body: opts.body ?? Buffer.from(""),
  } as unknown as Request;

  const json = jest.fn().mockReturnThis();
  const status = jest.fn().mockReturnValue({ json });
  const res = { status, json } as unknown as Response;
  const next = jest.fn() as unknown as NextFunction;

  return { req, res, next };
}

// ---------------------------------------------------------------------------
// Setup: fresh service per test
// ---------------------------------------------------------------------------

beforeEach(() => {
  const store = new WebhookSecretStore();
  _service = new WebhookSecretService(store);
});

// ---------------------------------------------------------------------------
// Missing headers
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – missing headers", () => {
  it("returns 400 when X-Webhook-Subscriber-Id is missing", () => {
    const { req, res, next } = makeReqResNext({
      headers: { "x-webhook-signature": "sha256=abc" },
    });

    webhookVerifyMiddleware(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(400);
    const jsonArg = (res.status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(jsonArg.error.code).toBe("MISSING_SUBSCRIBER_HEADER");
    expect(next).not.toHaveBeenCalled();
  });

  it("returns 400 when X-Webhook-Signature is missing", () => {
    const { req, res, next } = makeReqResNext({
      headers: { "x-webhook-subscriber-id": "sub-1" },
    });

    webhookVerifyMiddleware(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(400);
    const jsonArg = (res.status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(jsonArg.error.code).toBe("MISSING_SIGNATURE_HEADER");
    expect(next).not.toHaveBeenCalled();
  });

  it("returns 400 when both headers are missing", () => {
    const { req, res, next } = makeReqResNext({ headers: {} });

    webhookVerifyMiddleware(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(400);
    expect(next).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Unknown subscriber
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – unknown subscriber", () => {
  it("returns 401 (not 404) to prevent subscriber enumeration", () => {
    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "ghost",
        "x-webhook-signature": "sha256=abc",
      },
      body: Buffer.from("payload"),
    });

    webhookVerifyMiddleware(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(401);
    const jsonArg = (res.status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(jsonArg.error.message).toBe("Webhook signature verification failed");
    expect(next).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Invalid signature
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – invalid signature", () => {
  it("returns 401 for wrong signature", () => {
    _service.registerSubscriber("bad-sig-sub");

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "bad-sig-sub",
        "x-webhook-signature": "sha256=deadbeef",
      },
      body: Buffer.from("payload"),
    });

    webhookVerifyMiddleware(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(401);
    expect(next).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Valid signature
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – valid signature", () => {
  it("calls next() and attaches subscriber context for valid primary signature", () => {
    const { initial_secret } = _service.registerSubscriber("valid-sub");
    const payload = Buffer.from("hello-world");
    const sig = _service.computeSignature(payload, initial_secret);

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "valid-sub",
        "x-webhook-signature": sig,
      },
      body: payload,
    });

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith();
    expect((req as Request & { webhookSubscriberId?: string }).webhookSubscriberId).toBe("valid-sub");
    expect((req as Request & { webhookMatchedSecret?: string }).webhookMatchedSecret).toBe("primary");
  });

  it("calls next() for valid pending signature during rotation window", () => {
    _service.registerSubscriber("rot-mid-sub");
    const { new_secret } = _service.initiateRotation("rot-mid-sub");
    const payload = Buffer.from("event-data");
    const sig = _service.computeSignature(payload, new_secret);

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "rot-mid-sub",
        "x-webhook-signature": sig,
      },
      body: payload,
    });

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith();
    expect((req as Request & { webhookMatchedSecret?: string }).webhookMatchedSecret).toBe("pending");
  });

  it("calls next() for valid old (primary) signature during rotation window", () => {
    const { initial_secret } = _service.registerSubscriber("rot-old-sub");
    _service.initiateRotation("rot-old-sub");
    const payload = Buffer.from("event-data");
    const sig = _service.computeSignature(payload, initial_secret);

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "rot-old-sub",
        "x-webhook-signature": sig,
      },
      body: payload,
    });

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith();
    expect((req as Request & { webhookMatchedSecret?: string }).webhookMatchedSecret).toBe("primary");
  });
});

// ---------------------------------------------------------------------------
// Body handling
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – body handling", () => {
  it("handles string body", () => {
    const { initial_secret } = _service.registerSubscriber("str-body-sub");
    const payload = "string-payload";
    const sig = _service.computeSignature(payload, initial_secret);

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "str-body-sub",
        "x-webhook-signature": sig,
      },
      body: payload,
    });

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith();
  });

  it("handles object body (JSON-serialised)", () => {
    const { initial_secret } = _service.registerSubscriber("obj-body-sub");
    const bodyObj = { event: "invoice.paid", id: "123" };
    const serialised = JSON.stringify(bodyObj);
    const sig = _service.computeSignature(serialised, initial_secret);

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "obj-body-sub",
        "x-webhook-signature": sig,
      },
      body: bodyObj,
    });

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith();
  });

  it("handles undefined body as empty string", () => {
    const { initial_secret } = _service.registerSubscriber("null-body-sub");
    const sig = _service.computeSignature("", initial_secret);

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "null-body-sub",
        "x-webhook-signature": sig,
      },
      body: undefined,
    });

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith();
  });
});

// ---------------------------------------------------------------------------
// Unexpected errors forwarded to next(err)
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – unexpected errors", () => {
  it("forwards unexpected errors to next(err)", () => {
    _service.registerSubscriber("err-sub");
    const boom = new Error("unexpected");
    jest.spyOn(_service, "verifySignature").mockImplementation(() => {
      throw boom;
    });

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "err-sub",
        "x-webhook-signature": "sha256=abc",
      },
      body: Buffer.from("data"),
    });

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith(boom);
  });
});

// ---------------------------------------------------------------------------
// Array header values (Express can produce string[] for multi-value headers)
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – array header values", () => {
  it("returns 400 when subscriber header is an array", () => {
    const req = {
      headers: {
        "x-webhook-subscriber-id": ["sub-a", "sub-b"],
        "x-webhook-signature": "sha256=abc",
      },
      body: Buffer.from(""),
    } as unknown as Request;

    const json = jest.fn().mockReturnThis();
    const status = jest.fn().mockReturnValue({ json });
    const res = { status, json } as unknown as Response;
    const next = jest.fn() as unknown as NextFunction;

    webhookVerifyMiddleware(req, res, next);

    expect((status as jest.Mock)).toHaveBeenCalledWith(400);
    const arg = (status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(arg.error.code).toBe("MISSING_SUBSCRIBER_HEADER");
  });

  it("returns 400 when signature header is an array", () => {
    _service.registerSubscriber("arr-sig-sub");

    const req = {
      headers: {
        "x-webhook-subscriber-id": "arr-sig-sub",
        "x-webhook-signature": ["sha256=a", "sha256=b"],
      },
      body: Buffer.from(""),
    } as unknown as Request;

    const json = jest.fn().mockReturnThis();
    const status = jest.fn().mockReturnValue({ json });
    const res = { status, json } as unknown as Response;
    const next = jest.fn() as unknown as NextFunction;

    webhookVerifyMiddleware(req, res, next);

    expect((status as jest.Mock)).toHaveBeenCalledWith(400);
    const arg = (status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(arg.error.code).toBe("MISSING_SIGNATURE_HEADER");
  });
});

// ---------------------------------------------------------------------------
// Non-404 WebhookSecretError forwarded with its own status code
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – non-404 WebhookSecretError", () => {
  it("returns the error's own status for non-SUBSCRIBER_NOT_FOUND errors", () => {
    _service.registerSubscriber("conflict-sub");
    const { WebhookSecretError: WSE } = jest.requireActual<
      typeof import("../services/webhookSecretService")
    >("../services/webhookSecretService");

    jest.spyOn(_service, "verifySignature").mockImplementation(() => {
      throw new WSE("Rotation conflict", "ROTATION_ALREADY_IN_PROGRESS", 409);
    });

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "conflict-sub",
        "x-webhook-signature": "sha256=abc",
      },
      body: Buffer.from("data"),
    });

    webhookVerifyMiddleware(req, res, next);

    expect((res.status as jest.Mock)).toHaveBeenCalledWith(409);
    const arg = (res.status as jest.Mock).mock.results[0].value.json.mock.calls[0][0];
    expect(arg.error.code).toBe("ROTATION_ALREADY_IN_PROGRESS");
    expect(arg.error.message).toBe("Rotation conflict");
  });
});

// ---------------------------------------------------------------------------
// Null body (distinct from undefined – covers req.body ?? "" null branch)
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – null body", () => {
  it("treats null body as JSON.stringify('') for signature computation", () => {
    const { initial_secret } = _service.registerSubscriber("null-body-exact");
    // null body: null ?? "" → "" → JSON.stringify("") → '""'
    const sig = _service.computeSignature(JSON.stringify(""), initial_secret);

    const req = {
      headers: {
        "x-webhook-subscriber-id": "null-body-exact",
        "x-webhook-signature": sig,
      },
      body: null,
    } as unknown as Request;

    const json = jest.fn().mockReturnThis();
    const status = jest.fn().mockReturnValue({ json });
    const res = { status, json } as unknown as Response;
    const next = jest.fn() as unknown as NextFunction;

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith();
  });
});

// ---------------------------------------------------------------------------
// matched_secret null path (result.matched_secret ?? undefined)
// ---------------------------------------------------------------------------

describe("webhookVerifyMiddleware – matched_secret null coalescing", () => {
  it("sets webhookMatchedSecret to undefined when matched_secret is null", () => {
    _service.registerSubscriber("null-match-sub");

    // Mock verifySignature to return valid=true but matched_secret=null
    jest.spyOn(_service, "verifySignature").mockReturnValueOnce({
      valid: true,
      matched_secret: null,
    });

    const { req, res, next } = makeReqResNext({
      headers: {
        "x-webhook-subscriber-id": "null-match-sub",
        "x-webhook-signature": "sha256=anything",
      },
      body: Buffer.from("data"),
    });

    webhookVerifyMiddleware(req, res, next);

    expect(next).toHaveBeenCalledWith();
    expect(
      (req as Request & { webhookMatchedSecret?: string }).webhookMatchedSecret
    ).toBeUndefined();
  });
});
