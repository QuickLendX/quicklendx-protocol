/**
 * Request Context — deterministic failure-boundary coverage for
 * `generateCorrelationId` (issue #2705).
 *
 * The generator sits on the hot path of every request that does not carry a
 * client-supplied id, so the failure boundary must be explicit and total:
 *
 *   • Success: canonical 26-character ULID, unique per call, sanitizer-safe.
 *   • Rejection: a ULID source that throws or returns anything that is not a
 *     well-formed ULID must never surface a tainted value or an exception.
 *   • Boundary: exact length/character-set edges, the 128 character sanitizer
 *     ceiling, a frozen clock and a missing crypto source.
 *   • Concurrency & retry: repeated and parallel failures stay unique, and a
 *     healthy source is used again as soon as it recovers.
 *   • Regression: existing callers (`getOrGenerateCorrelationId`,
 *     `request-logger`) keep working and keep their contracts.
 *
 * Every assertion is deterministic: no timing, no randomness and no network.
 */

import express from "express";
import supertest from "supertest";

import {
  generateCorrelationId,
  getCorrelationId,
  getOrGenerateCorrelationId,
  sanitizeCorrelationId,
  withCorrelationId,
} from "../lib/requestContext";
import {
  createRequestLogger,
  Logger,
  RequestLogEntry,
} from "../middleware/request-logger";

// Raw CommonJS handles so the ULID/crypto sources can be patched per test.
// Both properties are writable and configurable, so `jest.spyOn` observes the
// same module instance that `requestContext` reads at call time.
const ulidModule = require("ulid") as { ulid: () => string };
const cryptoModule = require("node:crypto") as { randomUUID: () => string };

/** The real ULID implementation, captured before any spy is installed. */
const realUlid = ulidModule.ulid;

/** Canonical ULID shape (Crockford base32, no I/L/O/U). */
const ULID_RE = /^[0-9A-HJKMNP-TV-Z]{26}$/;

/** Degraded-mode marker emitted when the ULID source fails. */
const DEGRADED_PREFIX = "fb-";

let ulidSpy: jest.SpyInstance;

beforeEach(() => {
  ulidSpy = jest.spyOn(ulidModule, "ulid");
});

afterEach(() => {
  jest.restoreAllMocks();
});

/**
 * Assert the invariant that couples the generator to the sanitiser: whatever
 * `generateCorrelationId` returns must be accepted verbatim by
 * `sanitizeCorrelationId`, so it can never be a log-injection vector.
 */
function expectLogSafeId(id: unknown): string {
  expect(typeof id).toBe("string");
  const value = id as string;
  expect(value.length).toBeGreaterThan(0);
  expect(value).not.toMatch(/[\s\u0000-\u001f\u007f]/);
  expect(sanitizeCorrelationId(value)).toBe(value);
  return value;
}

/** Make the ULID source unusable for the duration of a test. */
function failUlidSource(message = "ulid source unavailable"): void {
  ulidSpy.mockImplementation(() => {
    throw new Error(message);
  });
}

/** Make the ULID source return a single value (valid or corrupted), then recover. */
function ulidReturnsOnce(value: unknown): void {
  ulidSpy.mockImplementationOnce(() => value as string);
}

// ── 1. Success path ───────────────────────────────────────────────────────────

describe("generateCorrelationId — success path", () => {
  it("returns a canonical 26-character Crockford base32 ULID", () => {
    const id = generateCorrelationId();
    expect(id).toMatch(ULID_RE);
    expect(id).toHaveLength(26);
  });

  it("uses the ULID source exactly once per call", () => {
    ulidSpy.mockClear();
    generateCorrelationId();
    expect(ulidSpy).toHaveBeenCalledTimes(1);
  });

  it("returns a distinct id on every call", () => {
    const ids = new Set<string>();
    for (let i = 0; i < 1000; i++) {
      ids.add(generateCorrelationId());
    }
    expect(ids.size).toBe(1000);
  });

  it("never flags the healthy path as degraded", () => {
    for (let i = 0; i < 50; i++) {
      expect(generateCorrelationId().startsWith(DEGRADED_PREFIX)).toBe(false);
    }
  });

  it("always produces output accepted by sanitizeCorrelationId", () => {
    for (let i = 0; i < 250; i++) {
      const id = expectLogSafeId(generateCorrelationId());
      expect(id).toMatch(ULID_RE);
    }
  });

  it("never emits whitespace or control characters", () => {
    const ids = Array.from({ length: 250 }, () => generateCorrelationId());
    for (const id of ids) {
      expect(id).toMatch(/^[A-Za-z0-9_-]+$/);
    }
  });
});

// ── 2. Unusable / invalid ULID source ─────────────────────────────────────────

describe("generateCorrelationId — unusable ULID source", () => {
  it("does not throw when the ULID source throws an Error", () => {
    failUlidSource();
    expect(() => generateCorrelationId()).not.toThrow();
  });

  it("does not throw when the ULID source throws a non-Error value", () => {
    const thrownValues: unknown[] = [
      "string failure",
      42,
      null,
      undefined,
      { message: "object failure" },
    ];
    for (const value of thrownValues) {
      ulidSpy.mockImplementationOnce(() => {
        throw value;
      });
      expect(() => generateCorrelationId()).not.toThrow();
    }
  });

  it("returns a marked, log-safe degraded id when the source throws", () => {
    failUlidSource();
    const id = expectLogSafeId(generateCorrelationId());
    expect(id.startsWith(DEGRADED_PREFIX)).toBe(true);
  });

  it("never leaks the source error into the generated id", () => {
    failUlidSource("secret=do-not-log-this");
    const id = generateCorrelationId();
    expect(id).not.toContain("secret");
    expect(id).not.toContain("do-not-log-this");
    expect(id).not.toContain("Error");
    expect(id).not.toContain(" ");
  });

  it("rejects every non-string value the source may return", () => {
    const values: unknown[] = [
      undefined,
      null,
      42,
      NaN,
      true,
      {},
      [],
      () => realUlid(),
      Symbol("ulid"),
    ];
    for (const value of values) {
      ulidReturnsOnce(value);
      const id = expectLogSafeId(generateCorrelationId());
      expect(id.startsWith(DEGRADED_PREFIX)).toBe(true);
    }
  });

  it("rejects empty and whitespace-only values", () => {
    for (const value of ["", " ", "\t", "\n", "\r\n", "\u00a0", "   \t  "]) {
      ulidReturnsOnce(value);
      const id = expectLogSafeId(generateCorrelationId());
      expect(id.startsWith(DEGRADED_PREFIX)).toBe(true);
    }
  });

  it("rejects well-formed values with the wrong length", () => {
    for (const length of [1, 23, 24, 25, 27, 28, 32, 64]) {
      ulidReturnsOnce("A".repeat(length));
      const id = expectLogSafeId(generateCorrelationId());
      expect(id.startsWith(DEGRADED_PREFIX)).toBe(true);
    }
  });

  it("accepts a well-formed value at the exact 26 character boundary", () => {
    const boundary = "A".repeat(26);
    ulidReturnsOnce(boundary);
    expect(generateCorrelationId()).toBe(boundary);
  });

  it("rejects lowercase and out-of-alphabet Crockford characters", () => {
    const values = [
      "01h9k4w2x8y9z0a1b2c3d4e5f6", // lowercase
      "01H9K4W2X8Y9Z0A1B2C3D4E5I6", // contains I
      "01H9K4W2X8Y9Z0A1B2C3D4E5L6", // contains L
      "01H9K4W2X8Y9Z0A1B2C3D4E5O6", // contains O
      "01H9K4W2X8Y9Z0A1B2C3D4E5U6", // contains U
      "01H9K4W2X8Y9Z0A1B2C3D4E5F!", // punctuation
      " 1H9K4W2X8Y9Z0A1B2C3D4E5F6", // leading space (26 chars)
    ];
    for (const value of values) {
      expect(value).toHaveLength(26);
      ulidReturnsOnce(value);
      const id = expectLogSafeId(generateCorrelationId());
      expect(id.startsWith(DEGRADED_PREFIX)).toBe(true);
    }
  });

  it("rejects log-injection payloads from a compromised source", () => {
    const payloads = [
      "01H9K4W2X8Y9Z0A1B2C3D4E5F6\n[INFO] forged log line",
      "01H9K4W2X8Y9Z0A1B2C3D4E5F6\r\nX-Injected: 1",
      "\u001b[31m01H9K4W2X8Y9Z0A1B2C3D4E5F6\u001b[0m",
      "01H9K4W2X8Y9Z0A1B2C3D4E5F6\u0000",
      "01H9K4W2X8Y9Z0A1B2C3D4E5F6 01H9K4W2X8Y9Z0A1B2C3D4E5F6",
    ];
    for (const payload of payloads) {
      ulidReturnsOnce(payload);
      const id = expectLogSafeId(generateCorrelationId());
      expect(id.startsWith(DEGRADED_PREFIX)).toBe(true);
      expect(id).not.toContain("forged");
      expect(id).not.toContain("Injected");
    }
  });

  it("calls the source exactly once per call, even when it fails", () => {
    failUlidSource();
    ulidSpy.mockClear();
    generateCorrelationId();
    expect(ulidSpy).toHaveBeenCalledTimes(1);
  });
});


// ── 3. Boundary conditions ────────────────────────────────────────────────────

describe("generateCorrelationId — boundary conditions", () => {
  it("keeps degraded ids inside the 128 character sanitizer ceiling", () => {
    failUlidSource();
    for (let i = 0; i < 50; i++) {
      const id = expectLogSafeId(generateCorrelationId());
      expect(id.length).toBeLessThanOrEqual(128);
    }
  });

  it("stays unique when the clock is frozen mid-millisecond", () => {
    const nowSpy = jest.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    try {
      failUlidSource();
      const ids = new Set<string>();
      for (let i = 0; i < 500; i++) {
        ids.add(expectLogSafeId(generateCorrelationId()));
      }
      expect(ids.size).toBe(500);
    } finally {
      nowSpy.mockRestore();
    }
  });

  it("stays unique and log-safe when Date.now is unavailable", () => {
    jest.spyOn(Date, "now").mockImplementation(() => {
      throw new Error("clock unavailable");
    });
    failUlidSource();
    const ids = new Set<string>();
    for (let i = 0; i < 100; i++) {
      ids.add(expectLogSafeId(generateCorrelationId()));
    }
    expect(ids.size).toBe(100);
  });

  it("stays unique and log-safe when the crypto source is unavailable", () => {
    jest.spyOn(cryptoModule, "randomUUID").mockImplementation(() => {
      throw new Error("crypto unavailable");
    });
    failUlidSource();
    const ids = new Set<string>();
    for (let i = 0; i < 100; i++) {
      ids.add(expectLogSafeId(generateCorrelationId()));
    }
    expect(ids.size).toBe(100);
  });

  it("stays unique and log-safe when every entropy source is unavailable", () => {
    jest.spyOn(cryptoModule, "randomUUID").mockImplementation(() => {
      throw new Error("crypto unavailable");
    });
    jest.spyOn(Math, "random").mockImplementation(() => {
      throw new Error("entropy unavailable");
    });
    failUlidSource();
    const ids = new Set<string>();
    for (let i = 0; i < 50; i++) {
      ids.add(expectLogSafeId(generateCorrelationId()));
    }
    expect(ids.size).toBe(50);
  });

  it("keeps the healthy path unaffected by a previously frozen clock", () => {
    const nowSpy = jest.spyOn(Date, "now").mockReturnValue(0);
    try {
      expect(generateCorrelationId()).toMatch(ULID_RE);
    } finally {
      nowSpy.mockRestore();
    }
  });
});

// ── 4. Duplicates, retries and concurrency ────────────────────────────────────

describe("generateCorrelationId — duplicates, retries and concurrency", () => {
  it("returns unique ids across 1000 consecutive failures", () => {
    failUlidSource();
    const ids = new Set<string>();
    for (let i = 0; i < 1000; i++) {
      ids.add(generateCorrelationId());
    }
    expect(ids.size).toBe(1000);
  });

  it("returns unique ids across interleaved failures and successes", () => {
    let call = 0;
    ulidSpy.mockImplementation(() => {
      call += 1;
      if (call % 3 === 0) {
        throw new Error("flaky source");
      }
      return realUlid();
    });

    const ids = new Set<string>();
    let healthy = 0;
    let degraded = 0;
    for (let i = 0; i < 300; i++) {
      const id = expectLogSafeId(generateCorrelationId());
      ids.add(id);
      if (id.startsWith(DEGRADED_PREFIX)) {
        degraded += 1;
      } else {
        healthy += 1;
      }
    }

    expect(ids.size).toBe(300);
    expect(degraded).toBe(100);
    expect(healthy).toBe(200);
  });

  it("returns unique ids across 500 parallel failures", async () => {
    failUlidSource();
    const ids = await Promise.all(
      Array.from({ length: 500 }, () =>
        Promise.resolve().then(() => generateCorrelationId())
      )
    );
    expect(new Set(ids).size).toBe(500);
    ids.forEach((id) => expectLogSafeId(id));
  });

  it("recovers automatically once the source is healthy again", () => {
    failUlidSource();
    expect(generateCorrelationId().startsWith(DEGRADED_PREFIX)).toBe(true);

    ulidSpy.mockRestore();

    const recovered = generateCorrelationId();
    expect(recovered).toMatch(ULID_RE);
    expect(recovered.startsWith(DEGRADED_PREFIX)).toBe(false);
  });

  it("does not poison later calls after a single corrupted value", () => {
    ulidReturnsOnce("not-a-ulid");
    expect(generateCorrelationId().startsWith(DEGRADED_PREFIX)).toBe(true);
    expect(generateCorrelationId()).toMatch(ULID_RE);
  });

  it("does not mutate the correlation context it is called from", () => {
    failUlidSource();
    withCorrelationId("ctx-stable-1", () => {
      const first = expectLogSafeId(generateCorrelationId());
      const second = expectLogSafeId(generateCorrelationId());
      expect(first).not.toBe(second);
      expect(getCorrelationId()).toBe("ctx-stable-1");
    });
  });

  it("keeps parallel request contexts isolated while the source fails", async () => {
    failUlidSource();
    const results = await Promise.all(
      ["ctx-a", "ctx-b", "ctx-c"].map((ctx) =>
        withCorrelationId(ctx, async () => {
          const generated = expectLogSafeId(generateCorrelationId());
          await new Promise((resolve) => setTimeout(resolve, 1));
          return { ctx, observed: getCorrelationId(), generated };
        })
      )
    );

    expect(results.map((r) => r.observed)).toEqual(["ctx-a", "ctx-b", "ctx-c"]);
    expect(new Set(results.map((r) => r.generated)).size).toBe(3);
  });
});


// ── 5. Caller contracts (regression) ──────────────────────────────────────────

describe("generateCorrelationId — caller contracts", () => {
  it("getOrGenerateCorrelationId reuses the context id and skips the source", () => {
    withCorrelationId("ctx-reuse-1", () => {
      ulidSpy.mockClear();
      expect(getOrGenerateCorrelationId()).toBe("ctx-reuse-1");
      expect(ulidSpy).not.toHaveBeenCalled();
    });
  });

  it("getOrGenerateCorrelationId returns a log-safe id outside a context", () => {
    const id = expectLogSafeId(getOrGenerateCorrelationId());
    expect(id).toMatch(ULID_RE);
  });

  it("getOrGenerateCorrelationId survives a failing source", () => {
    failUlidSource();
    const id = expectLogSafeId(getOrGenerateCorrelationId());
    expect(id.startsWith(DEGRADED_PREFIX)).toBe(true);
  });
});

describe("request-logger integration (regression)", () => {
  function buildApp(entries: RequestLogEntry[]) {
    const captureLogger: Logger = {
      info: (entry) => {
        entries.push(entry);
      },
      error: () => {
        /* the probe route never triggers a redaction error */
      },
    };
    const app = express();
    app.use(createRequestLogger(captureLogger));
    app.get("/probe", (_req, res) => {
      res.status(200).json({ ok: true });
    });
    return app;
  }

  it("issues a valid X-Request-Id header when the source fails", async () => {
    failUlidSource();
    const entries: RequestLogEntry[] = [];

    const response = await supertest(buildApp(entries)).get("/probe").expect(200);

    const requestId = expectLogSafeId(response.headers["x-request-id"]);
    expect(requestId.startsWith(DEGRADED_PREFIX)).toBe(true);
    expect(entries).toHaveLength(1);
    expect(entries[entries.length - 1].requestId).toBe(requestId);
  });

  it("echoes a valid inbound X-Request-Id without calling the source", async () => {
    const entries: RequestLogEntry[] = [];
    const app = buildApp(entries);
    ulidSpy.mockClear();

    const response = await supertest(app)
      .get("/probe")
      .set("X-Request-Id", "inbound-request-1")
      .expect(200);

    expect(response.headers["x-request-id"]).toBe("inbound-request-1");
    expect(entries[entries.length - 1].requestId).toBe("inbound-request-1");
    expect(ulidSpy).not.toHaveBeenCalled();
  });

  it("replaces an over-long inbound id with a generated one", async () => {
    const entries: RequestLogEntry[] = [];
    const app = buildApp(entries);

    const response = await supertest(app)
      .get("/probe")
      .set("X-Request-Id", "a".repeat(129))
      .expect(200);

    const requestId = expectLogSafeId(response.headers["x-request-id"]);
    expect(requestId).toMatch(ULID_RE);
    expect(entries[entries.length - 1].requestId).toBe(requestId);
  });
});

