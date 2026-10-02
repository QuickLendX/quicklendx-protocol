/**
 * `runWithContext` — deterministic failure-boundary coverage (issue #2696).
 *
 * `runWithContext` is the single point at which an inbound correlation id is
 * admitted to, or refused entry to, the `AsyncLocalStorage` context that every
 * downstream audit write, outbound RPC call and log line reads. It is the
 * function that decides whether a request is traceable and whether an
 * attacker-controlled string can forge a log line, so its boundaries have to be
 * explicit and total:
 *
 *   • Success — a usable id is stored verbatim and the callback's outcome
 *     (value, promise, throw, rejection) is passed through unchanged.
 *   • Rejection — an id that is not a string, or that fails
 *     `sanitizeCorrelationId`, is discarded in favour of a generated one. It is
 *     never stored, so it can never reach a log line.
 *   • Degradation — when the id source is *also* unavailable, the run still
 *     succeeds with a `fb-`-prefixed, alertable, log-safe id. This is the
 *     combined boundary: neither #2697 (`getCorrelationId`), #2699
 *     (`getOrGenerateCorrelationId`) nor #2702 (the middleware) exercises it.
 *   • Teardown — a throw or a rejection propagates with its original identity
 *     and leaves the caller's scope clean, while work scheduled inside the
 *     callback that outlives it keeps the id.
 *   • Isolation — concurrent, nested and retried runs never cross-talk.
 *
 * Every assertion is deterministic: no clock, network, database, Express app,
 * or randomness in the assertions themselves. Uniqueness is asserted as
 * "all distinct", never as a fixed value, and ULID ordering is never asserted
 * (plain `ulid()` is not monotonic within a millisecond).
 *
 * Self-contained by design: the only seam used to drive id-source failure is
 * the module's own `_setUlidGeneratorForTesting` hook. No `fast-check`, no
 * `supertest`, no module-namespace monkey-patching — the package is ESM, where
 * a test runner cannot spy on a module namespace object, so the exported hook
 * is the only portable way to reach this boundary.
 */

import { ulid } from "ulid";

import {
  MAX_CORRELATION_ID_LENGTH,
  _resetUlidGeneratorForTesting,
  _setUlidGeneratorForTesting,
  createRequestContextMiddleware,
  generateCorrelationId,
  getCorrelationId,
  getOrGenerateCorrelationId,
  runWithContext,
  sanitizeCorrelationId,
  withCorrelationId,
} from "../lib/requestContext";
import { createRequestLogger } from "../middleware/request-logger";

/** Canonical ULID shape: 26 Crockford base32 characters, no I/L/O/U. */
const ULID_RE = /^[0-9A-HJKMNP-TV-Z]{26}$/;
const ULID_LENGTH = 26;

/** Degraded-mode marker emitted when the id source is unavailable. */
const DEGRADED_PREFIX = "fb-";

/**
 * Every id this module puts into the context must survive the sanitiser
 * unchanged. This is the invariant that couples the generator to the security
 * boundary, so it is asserted for generated ids as well as stored ones: a
 * generated id that the sanitiser would reject could not be logged safely.
 */
function expectLogSafeId(id: unknown): string {
  expect(typeof id).toBe("string");
  const value = id as string;
  expect(value.length).toBeGreaterThan(0);
  expect(value).not.toMatch(/[\s\u0000-\u001f\u007f]/);
  expect(sanitizeCorrelationId(value)).toBe(value);
  return value;
}

/** Make the id source unusable for the duration of a test. */
function failIdSource(message = "id source unavailable"): void {
  _setUlidGeneratorForTesting(() => {
    throw new Error(message);
  });
}

/**
 * Install a counting id source. Returns a getter for the number of calls, so a
 * test can prove the source is consulted exactly once per fallback — or not at
 * all when a usable inbound id means no generation is needed.
 */
function countingIdSource(
  impl: (call: number) => string
): { calls: () => number } {
  let call = 0;
  _setUlidGeneratorForTesting(() => impl(call++));
  return { calls: () => call };
}

/**
 * Install a source that returns each supplied value in turn and then delegates
 * to the real `ulid`, so a run of bad values can be shown not to poison later
 * calls.
 *
 * It delegates to the `ulid` package directly rather than to
 * `generateCorrelationId`, because the override *is* the source
 * `generateCorrelationId` reads — calling back into it would recurse until the
 * stack overflowed, and the resulting `RangeError` would be swallowed by the
 * very catch this boundary is meant to test.
 */
function useFlakyIdSource(...badValues: string[]): void {
  let index = 0;
  _setUlidGeneratorForTesting(() => {
    const next = badValues[index++];
    return next === undefined ? ulid() : next;
  });
}

afterEach(() => {
  _resetUlidGeneratorForTesting();
});

// ── 1. Success path ───────────────────────────────────────────────────────────

describe("runWithContext — a usable id is stored verbatim", () => {
  it("returns the callback's synchronous value", () => {
    expect(runWithContext("ok-1", () => 6 * 7)).toBe(42);
  });

  it("passes the callback's promise back untouched, not a wrapper", async () => {
    const promise = Promise.resolve("value");
    const returned = runWithContext("ok-2", () => promise);
    expect(returned).toBe(promise);
    await expect(returned).resolves.toBe("value");
  });

  it("exposes the id to the callback and to its synchronous callees", () => {
    const deep = () => getCorrelationId();
    const middle = () => deep();
    expect(runWithContext("ok-3", middle)).toBe("ok-3");
  });

  it("returns the same id on every read within one run", () => {
    const reads = runWithContext("ok-4", () => [
      getCorrelationId(),
      getCorrelationId(),
      getCorrelationId(),
    ]);
    expect(reads).toEqual(["ok-4", "ok-4", "ok-4"]);
  });

  it("keeps the id visible across an await inside the callback", async () => {
    const seen = await runWithContext("ok-5", async () => {
      await Promise.resolve();
      await new Promise<void>((resolve) => setTimeout(resolve, 1));
      return getCorrelationId();
    });
    expect(seen).toBe("ok-5");
  });

  it("trims surrounding whitespace rather than storing it", () => {
    expect(runWithContext("  ok-6  ", () => getCorrelationId())).toBe("ok-6");
    expect(runWithContext("\t\nok-7\r\n", () => getCorrelationId())).toBe("ok-7");
  });

  it("accepts every character the sanitiser admits", () => {
    const accepted = [
      "a",
      "Z",
      "0",
      "with-hyphen",
      "with_underscore",
      "trace.1",
      "trace:1",
      "svc.node:8080",
      "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    ];
    for (const id of accepted) {
      expect(runWithContext(id, () => getCorrelationId())).toBe(id);
    }
  });

  it("does not consult the id source when the inbound id is usable", () => {
    const source = countingIdSource(() => "SHOULD-NOT-BE-USED");
    expect(runWithContext("ok-8", () => getCorrelationId())).toBe("ok-8");
    expect(source.calls()).toBe(0);
  });

  it("reuses the context id for getOrGenerateCorrelationId without generating", () => {
    const source = countingIdSource(() => "SHOULD-NOT-BE-USED");
    const seen = runWithContext("ok-9", () => getOrGenerateCorrelationId());
    expect(seen).toBe("ok-9");
    expect(source.calls()).toBe(0);
  });

  it("lets an inner run win and restores the outer id afterwards", () => {
    const seen = runWithContext("outer-10", () => {
      const inner = runWithContext("inner-10", () => getCorrelationId());
      expect(inner).toBe("inner-10");
      return getCorrelationId();
    });
    expect(seen).toBe("outer-10");
  });

  it("keeps withCorrelationId an exact alias", () => {
    expect(withCorrelationId("alias-11", () => getCorrelationId())).toBe(
      runWithContext("alias-11", () => getCorrelationId())
    );
    expect(withCorrelationId("alias-12", () => getCorrelationId())).toBe(
      "alias-12"
    );
  });
});

// ── 2. Rejection path — an unusable id is never stored ────────────────────────

describe("runWithContext — an unusable id is discarded, never stored", () => {
  const nonStrings: ReadonlyArray<readonly [string, unknown]> = [
    ["undefined", undefined],
    ["null", null],
    ["number", 42],
    ["zero", 0],
    ["NaN", NaN],
    ["boolean", true],
    ["object", {}],
    ["array", []],
    ["symbol", Symbol("id")],
    ["bigint", BigInt(7)],
    ["function", () => "injected"],
  ];

  it.each(nonStrings)(
    "replaces a non-string id (%s) with a generated one",
    (_label, raw) => {
      const seen = runWithContext(raw as never, () => getCorrelationId());
      expect(seen).toMatch(ULID_RE);
      expect(seen).toHaveLength(ULID_LENGTH);
      expectLogSafeId(seen);
    }
  );

  it("still invokes the callback exactly once for a non-string id", () => {
    for (const [, raw] of nonStrings) {
      let calls = 0;
      runWithContext(raw as never, () => {
        calls += 1;
      });
      expect(calls).toBe(1);
    }
  });

  it("replaces blank and whitespace-only ids", () => {
    const blanks = ["", " ", "\t", "\n", "\r\n", "   \t  ", "\u00a0"];
    for (const raw of blanks) {
      const seen = runWithContext(raw, () => getCorrelationId());
      expect(seen).toMatch(ULID_RE);
      expect(seen).not.toBe(raw);
    }
  });

  it("replaces ids longer than the sanitiser ceiling", () => {
    for (const length of [MAX_CORRELATION_ID_LENGTH + 1, 1024]) {
      const seen = runWithContext("a".repeat(length), () => getCorrelationId());
      expect(seen).toMatch(ULID_RE);
    }
  });

  it("refuses every log-injection payload, storing a generated id instead", () => {
    const payloads = [
      "abc\nINFO forged log line",
      "abc\r\nX-Injected: 1",
      "\u001b[31mabc\u001b[0m",
      "abc\u0000",
      "abc def",
      "abc\tdef",
      "abc/def",
      "abc+def",
      "abc@def",
      "{abc}",
      'abc"def',
      "abcé",
    ];
    for (const raw of payloads) {
      const seen = runWithContext(raw, () => getCorrelationId());
      expect(seen).toMatch(ULID_RE);
      // The refused value must not survive anywhere in what was stored.
      expect(seen).not.toContain("abc");
      expect(seen).not.toContain("forged");
      expect(seen).not.toContain("Injected");
      expectLogSafeId(seen);
    }
  });

  it("never leaks a planted sentinel from a refused id", () => {
    const sentinel = "QLX-SECRET-4f2a91";
    // Each of these is genuinely refused by the sanitiser, so the sentinel must
    // not survive in any form. `pre-QLX-…` is deliberately *not* in this list:
    // it is a legal id, and rewriting a caller's legitimate id would be a bug.
    const payloads = [
      `${sentinel}\nforged`,
      `${sentinel}\r\nX-Trace: 1`,
      `\u0000${sentinel}\u0000`,
      `${sentinel} ${sentinel}`,
      sentinel.repeat(30),
    ];
    for (const raw of payloads) {
      // Guard the premise: this payload really is one the sanitiser refuses.
      expect(sanitizeCorrelationId(raw)).toBeNull();
      const seen = runWithContext(raw, () => getCorrelationId());
      expect(seen).not.toContain(sentinel);
    }
  });

  it("preserves a legitimate id verbatim, even one that contains sentinel text", () => {
    // The other half of the contract: refusing unsafe ids must not become a
    // blanket "rewrite anything unfamiliar" policy. A legal id is stored as-is,
    // so a caller can still correlate its own request.
    const legal = `pre-QLX-SECRET-4f2a91`;
    expect(sanitizeCorrelationId(legal)).toBe(legal);
    expect(runWithContext(legal, () => getCorrelationId())).toBe(legal);
  });

  it("mints a distinct id for each refused run rather than reusing one", () => {
    const seen = new Set<string>();
    for (let i = 0; i < 200; i++) {
      seen.add(runWithContext(`refused\n${i}`, () => getCorrelationId() as string));
    }
    expect(seen.size).toBe(200);
  });
});

// ── 3. Length boundaries ──────────────────────────────────────────────────────

describe("runWithContext — length boundaries", () => {
  it("accepts ids of one character and of the exact ceiling", () => {
    expect(runWithContext("a", () => getCorrelationId())).toBe("a");
    const atCeiling = "z".repeat(MAX_CORRELATION_ID_LENGTH);
    expect(runWithContext(atCeiling, () => getCorrelationId())).toBe(
      atCeiling
    );
  });

  it("rejects one character past the ceiling", () => {
    const pastCeiling = "z".repeat(MAX_CORRELATION_ID_LENGTH + 1);
    expect(runWithContext(pastCeiling, () => getCorrelationId())).toMatch(
      ULID_RE
    );
  });

  it("measures length after trimming, so padding around a ceiling-length id is accepted", () => {
    // The sanitiser trims first and bounds the trimmed value, so a 130-character
    // raw string whose 128-character body is valid is usable, not refused.
    const body = "q".repeat(MAX_CORRELATION_ID_LENGTH);
    expect(runWithContext(` ${body} `, () => getCorrelationId())).toBe(body);
  });

  it("accepts an id made only of separator characters", () => {
    for (const id of [".", ":", ".-:", "::..", "_-.:"]) {
      expect(runWithContext(id, () => getCorrelationId())).toBe(id);
    }
  });
});

// ── 4. Id-source failure — the combined boundary ──────────────────────────────

describe("runWithContext — when the id source is also unavailable", () => {
  it("honours a usable inbound id and never calls the failed source", () => {
    failIdSource();
    let calls = 0;
    _setUlidGeneratorForTesting(() => {
      calls += 1;
      throw new Error("id source unavailable");
    });
    expect(runWithContext("survivor-12", () => getCorrelationId())).toBe(
      "survivor-12"
    );
    expect(calls).toBe(0);
  });

  it("still establishes a context when the inbound id is refused and the source fails", () => {
    failIdSource();
    const seen = runWithContext("", () => getCorrelationId());
    expect(seen).not.toBeNull();
    expect(seen!.startsWith(DEGRADED_PREFIX)).toBe(true);
    expectLogSafeId(seen);
  });

  it("invokes the callback exactly once on the combined failure path", () => {
    failIdSource();
    let calls = 0;
    const seen = runWithContext("refused\nid", () => {
      calls += 1;
      return getCorrelationId();
    });
    expect(calls).toBe(1);
    expect(seen).not.toBeNull();
  });

  it("does not throw on any combined-failure input", () => {
    const refused: unknown[] = [
      "",
      "   ",
      "a".repeat(MAX_CORRELATION_ID_LENGTH + 1),
      "a\nb",
      42,
      null,
      undefined,
      {},
    ];
    for (const raw of refused) {
      failIdSource();
      expect(() =>
        runWithContext(raw as never, () => getCorrelationId())
      ).not.toThrow();
    }
  });

  it("treats a source returning a non-string or empty value as a failure", () => {
    const bogus: unknown[] = [undefined, null, "", "   ", 42, {}, [], true];
    for (const value of bogus) {
      _setUlidGeneratorForTesting(() => value as string);
      const seen = runWithContext("", () => getCorrelationId());
      expect(seen!.startsWith(DEGRADED_PREFIX)).toBe(true);
      expectLogSafeId(seen);
    }
  });

  it("treats a malformed or tainted source value as a failure", () => {
    const malformed: unknown[] = [
      "A".repeat(25),
      "A".repeat(27),
      "01h9k4w2x8y9z0a1b2c3d4e5f6", // lowercase
      "01H9K4W2X8Y9Z0A1B2C3D4E5I6", // I is not Crockford base32
      "01H9K4W2X8Y9Z0A1B2C3D4E5F6\n[INFO] forged log line",
      "\u0000\u0001",
    ];
    for (const value of malformed) {
      _setUlidGeneratorForTesting(() => value as string);
      const seen = runWithContext("", () => getCorrelationId());
      expect(seen!.startsWith(DEGRADED_PREFIX)).toBe(true);
      // A compromised source must not be able to smuggle content into the id.
      expect(seen).not.toContain("forged");
      expect(seen).not.toContain("[INFO]");
      expectLogSafeId(seen);
    }
  });

  it("never embeds the source error in the generated id", () => {
    failIdSource("secret=do-not-log-this");
    const seen = runWithContext("", () => getCorrelationId());
    expect(seen).not.toContain("secret");
    expect(seen).not.toContain("do-not-log-this");
    expect(seen).not.toContain("Error");
  });

  it("keeps degraded ids inside the sanitiser ceiling", () => {
    failIdSource();
    for (let i = 0; i < 50; i++) {
      const seen = runWithContext("", () => getCorrelationId());
      expect(seen!.length).toBeLessThanOrEqual(MAX_CORRELATION_ID_LENGTH);
    }
  });

  it("mints a distinct degraded id for every combined-failure run", () => {
    failIdSource();
    const seen = new Set<string>();
    for (let i = 0; i < 200; i++) {
      seen.add(runWithContext("", () => getCorrelationId() as string));
    }
    expect(seen.size).toBe(200);
  });

  it("stays unique and log-safe when the clock itself is unavailable", () => {
    // The degraded id embeds `Date.now()`. If the clock is missing or throwing,
    // the monotonic sequence alone must still yield a unique, log-safe id —
    // otherwise every request in that window would share one id and audit
    // entries would become indistinguishable.
    const nowSpy = vi.spyOn(Date, "now").mockImplementation(() => {
      throw new Error("clock unavailable");
    });
    try {
      failIdSource();
      const seen = new Set<string>();
      for (let i = 0; i < 100; i++) {
        const id = runWithContext("", () => getCorrelationId());
        expect(id!.startsWith(DEGRADED_PREFIX)).toBe(true);
        expectLogSafeId(id);
        seen.add(id as string);
      }
      expect(seen.size).toBe(100);
    } finally {
      nowSpy.mockRestore();
    }
  });

  it("stays unique and log-safe when the clock is frozen mid-millisecond", () => {
    const nowSpy = vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    try {
      failIdSource();
      const seen = new Set<string>();
      for (let i = 0; i < 100; i++) {
        seen.add(runWithContext("", () => getCorrelationId() as string));
      }
      expect(seen.size).toBe(100);
    } finally {
      nowSpy.mockRestore();
    }
  });

  it("consults the id source exactly once per fallback", () => {
    const source = countingIdSource(() => {
      throw new Error("id source unavailable");
    });
    runWithContext("", () => getCorrelationId());
    expect(source.calls()).toBe(1);
  });

  it("returns to the real generator as soon as the source recovers", () => {
    failIdSource();
    const degraded = runWithContext("", () => getCorrelationId());
    expect(degraded!.startsWith(DEGRADED_PREFIX)).toBe(true);

    _resetUlidGeneratorForTesting();
    const recovered = runWithContext("", () => getCorrelationId());
    expect(recovered).toMatch(ULID_RE);
    expect(recovered!.startsWith(DEGRADED_PREFIX)).toBe(false);
  });

  it("is not poisoned by a single bad source value", () => {
    useFlakyIdSource("not-a-ulid");
    const degraded = runWithContext("", () => getCorrelationId());
    expect(degraded!.startsWith(DEGRADED_PREFIX)).toBe(true);
    // The very next run uses the real generator again.
    expect(runWithContext("", () => getCorrelationId())).toMatch(ULID_RE);
  });

  it("is not poisoned by a run of bad source values", () => {
    useFlakyIdSource("not-a-ulid", "", "A".repeat(3), "01H9K4W2X8Y9Z0A1B2C3D4E5I6");
    for (let i = 0; i < 4; i++) {
      const degraded = runWithContext("", () => getCorrelationId());
      expect(degraded!.startsWith(DEGRADED_PREFIX)).toBe(true);
      expectLogSafeId(degraded);
    }
    expect(runWithContext("", () => getCorrelationId())).toMatch(ULID_RE);
  });

  it("keeps distinct contexts isolated while the source is failing", async () => {
    failIdSource();
    const results = await Promise.all(
      ["iso-a", "iso-b", "iso-c", "iso-d"].map((id) =>
        runWithContext(id, async () => {
          const before = getCorrelationId();
          await new Promise<void>((resolve) => setTimeout(resolve, 1));
          return { id, before, after: getCorrelationId() };
        })
      )
    );
    expect(results.map((r) => r.before)).toEqual([
      "iso-a",
      "iso-b",
      "iso-c",
      "iso-d",
    ]);
    expect(results.map((r) => r.after)).toEqual([
      "iso-a",
      "iso-b",
      "iso-c",
      "iso-d",
    ]);
  });

  it("keeps distinct contexts isolated when the fallback mints the ids", async () => {
    failIdSource();
    const results = await Promise.all(
      Array.from({ length: 8 }, () =>
        runWithContext("", async () => {
          await new Promise<void>((resolve) => setTimeout(resolve, 1));
          return getCorrelationId();
        })
      )
    );
    // Each refused run mints its own id, so no two parallel runs collide and no
    // one run observes another's.
    expect(new Set(results).size).toBe(8);
    for (const id of results) {
      expect(id!.startsWith(DEGRADED_PREFIX)).toBe(true);
      expectLogSafeId(id);
    }
  });
});

// ── 5. Failure propagation and teardown ───────────────────────────────────────

describe("runWithContext — propagates the callback's failure unchanged", () => {
  it("rethrows a synchronous failure with its original identity and tears down", () => {
    const failure = new Error("sync failure");
    let caught: unknown;
    try {
      runWithContext("throw-13", () => {
        expect(getCorrelationId()).toBe("throw-13");
        throw failure;
      });
    } catch (err) {
      caught = err;
    }
    expect(caught).toBe(failure);
    expect(getCorrelationId()).toBeNull();
  });

  it("propagates a non-Error synchronous throw with its original identity", () => {
    const thrown: unknown[] = ["string failure", 42, null, undefined, { a: 1 }];
    for (const value of thrown) {
      let caught: unknown;
      try {
        runWithContext("throw-14", () => {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw value;
        });
      } catch (err) {
        caught = err;
      }
      expect(caught).toBe(value);
    }
    expect(getCorrelationId()).toBeNull();
  });

  it("invokes the callback exactly once even when it throws", () => {
    let calls = 0;
    expect(() =>
      runWithContext("throw-15", () => {
        calls += 1;
        throw new Error("boom");
      })
    ).toThrow("boom");
    expect(calls).toBe(1);
  });

  it("restores the outer context after an inner run throws", () => {
    const seen = runWithContext("outer-16", () => {
      expect(() =>
        runWithContext("inner-16", () => {
          throw new Error("inner failure");
        })
      ).toThrow("inner failure");
      return getCorrelationId();
    });
    expect(seen).toBe("outer-16");
  });

  it("propagates a rejected promise with its original identity", async () => {
    const failure = new Error("async failure");
    await expect(
      runWithContext("reject-17", async () => {
        await Promise.resolve();
        expect(getCorrelationId()).toBe("reject-17");
        throw failure;
      })
    ).rejects.toBe(failure);
    expect(getCorrelationId()).toBeNull();
  });

  it("propagates a rejection that settles after the run returns", async () => {
    const failure = new Error("late failure");
    const pending = runWithContext("reject-18", () =>
      new Promise<never>((_resolve, reject) => {
        setTimeout(() => reject(failure), 1);
      })
    );
    await expect(pending).rejects.toBe(failure);
    expect(getCorrelationId()).toBeNull();
  });

  it("does not surface an unhandled rejection for a caught failure", async () => {
    const unhandled: unknown[] = [];
    const onUnhandled = (reason: unknown) => unhandled.push(reason);
    process.on("unhandledRejection", onUnhandled);
    try {
      const failure = new Error("handled failure");
      await expect(
        runWithContext("reject-19", async () => {
          throw failure;
        })
      ).rejects.toBe(failure);
      // Give the microtask queue and one macrotask turn to report anything the
      // runtime considered unhandled.
      await new Promise<void>((resolve) => setTimeout(resolve, 20));
    } finally {
      process.off("unhandledRejection", onUnhandled);
    }
    expect(unhandled).toEqual([]);
  });

  it("leaves the caller's scope clean after a rejection inside a nested run", async () => {
    const failure = new Error("nested rejection");
    await expect(
      runWithContext("outer-20", async () => {
        await expect(
          runWithContext("inner-20", async () => {
            throw failure;
          })
        ).rejects.toBe(failure);
        return getCorrelationId();
      })
    ).resolves.toBe("outer-20");
    expect(getCorrelationId()).toBeNull();
  });

  it("establishes no context when the callback is not callable", () => {
    let caught: unknown;
    try {
      runWithContext("bad-fn-21", undefined as never);
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(TypeError);
    expect(getCorrelationId()).toBeNull();
  });

  it("never throws on the id path, whatever the callback does", () => {
    const refusing: unknown[] = [undefined, null, 42, {}, "", " ", "a\nb"];
    for (const raw of refusing) {
      let caught: unknown;
      try {
        runWithContext(raw as never, () => {
          throw new Error("callback failure");
        });
      } catch (err) {
        caught = err;
      }
      // The only failure that surfaces is the callback's own, never an id-path
      // failure that would mask it.
      expect((caught as Error).message).toBe("callback failure");
    }
  });
});

// ── 6. Escaped async work ─────────────────────────────────────────────────────

describe("runWithContext — work scheduled inside the run outlives it", () => {
  it("still sees the id from a timer that fires after the run returned", async () => {
    let observed: string | null = null;
    const settled = new Promise<void>((resolve) => {
      runWithContext("escape-22", () => {
        setTimeout(() => {
          observed = getCorrelationId();
          resolve();
        }, 1);
      });
    });
    // The caller's own scope is clean while the escaped work is pending.
    expect(getCorrelationId()).toBeNull();
    await settled;
    expect(observed).toBe("escape-22");
    expect(getCorrelationId()).toBeNull();
  });

  it("still sees the id from a promise continuation that settles after the run returned", async () => {
    let observed: string | null = null;
    const settled = new Promise<void>((resolve) => {
      runWithContext("escape-23", () => {
        void Promise.resolve().then(() => {
          observed = getCorrelationId();
          resolve();
        });
      });
    });
    await settled;
    expect(observed).toBe("escape-23");
  });

  it("keeps two escaped continuations from different runs isolated", async () => {
    const seen = new Map<string, string | null>();
    const settled = Promise.all(
      [
        ["escape-24", 1],
        ["escape-25", 5],
        ["escape-26", 9],
      ].map(
        ([id, delay]) =>
          new Promise<void>((resolve) => {
            runWithContext(id as string, () => {
              setTimeout(() => {
                seen.set(id as string, getCorrelationId());
                resolve();
              }, delay as number);
            });
          })
      )
    );
    await settled;
    expect(seen.get("escape-24")).toBe("escape-24");
    expect(seen.get("escape-25")).toBe("escape-25");
    expect(seen.get("escape-26")).toBe("escape-26");
  });

  it("keeps an escaped continuation reading a degraded id after a source failure", async () => {
    failIdSource();
    let observed: string | null = null;
    const settled = new Promise<void>((resolve) => {
      runWithContext("", () => {
        setTimeout(() => {
          observed = getCorrelationId();
          resolve();
        }, 1);
      });
    });
    await settled;
    expect(observed!.startsWith(DEGRADED_PREFIX)).toBe(true);
    expectLogSafeId(observed);
  });
});

// ── 7. Concurrency, nesting and retry ─────────────────────────────────────────

describe("runWithContext — isolation under concurrency and retry", () => {
  it("keeps 20 concurrent runs with distinct ids from cross-talking", async () => {
    const results = await Promise.all(
      Array.from({ length: 20 }, (_unused, index) => {
        const id = `concurrent-${index}`;
        return runWithContext(id, async () => {
          const before = getCorrelationId();
          // Index-based stagger, not a wall-clock sleep, so interleaving is
          // reproducible across machines.
          await new Promise<void>((resolve) => setImmediate(resolve));
          await Promise.resolve();
          return { id, before, after: getCorrelationId() };
        });
      })
    );
    for (let index = 0; index < 20; index++) {
      const id = `concurrent-${index}`;
      expect(results[index]).toEqual({ id, before: id, after: id });
    }
  });

  it("gives two concurrent runs the same id the same view of it", async () => {
    const [a, b] = await Promise.all([
      runWithContext("shared-27", async () => {
        await new Promise<void>((resolve) => setImmediate(resolve));
        return getCorrelationId();
      }),
      runWithContext("shared-27", async () => {
        await new Promise<void>((resolve) => setImmediate(resolve));
        return getCorrelationId();
      }),
    ]);
    expect(a).toBe("shared-27");
    expect(b).toBe("shared-27");
  });

  it("keeps nested and parallel runs independent", async () => {
    const results = await Promise.all(
      ["nest-a", "nest-b"].map((outer) =>
        runWithContext(outer, async () => {
          const outerBefore = getCorrelationId();
          const inner = await runWithContext(`${outer}-inner`, async () => {
            const innerBefore = getCorrelationId();
            await new Promise<void>((resolve) => setImmediate(resolve));
            return { innerBefore, innerAfter: getCorrelationId() };
          });
          return { outerBefore, inner, outerAfter: getCorrelationId() };
        })
      )
    );
    expect(results[0].outerBefore).toBe("nest-a");
    expect(results[0].inner).toEqual({
      innerBefore: "nest-a-inner",
      innerAfter: "nest-a-inner",
    });
    expect(results[0].outerAfter).toBe("nest-a");
    expect(results[1].outerBefore).toBe("nest-b");
    expect(results[1].inner).toEqual({
      innerBefore: "nest-b-inner",
      innerAfter: "nest-b-inner",
    });
    expect(results[1].outerAfter).toBe("nest-b");
  });

  it("leaves no residue when the same id is retried sequentially", () => {
    for (let attempt = 0; attempt < 5; attempt++) {
      expect(runWithContext("retry-28", () => getCorrelationId())).toBe(
        "retry-28"
      );
      expect(getCorrelationId()).toBeNull();
    }
  });

  it("starts a retry from a clean context after a throw", () => {
    expect(() =>
      runWithContext("retry-29", () => {
        throw new Error("first attempt failed");
      })
    ).toThrow("first attempt failed");
    expect(getCorrelationId()).toBeNull();
    expect(runWithContext("retry-29", () => getCorrelationId())).toBe("retry-29");
    expect(getCorrelationId()).toBeNull();
  });

  it("mints a fresh id for each retry when the retry's id is refused", () => {
    const seen = new Set<string>();
    for (let attempt = 0; attempt < 5; attempt++) {
      seen.add(runWithContext("bad\nid", () => getCorrelationId() as string));
    }
    expect(seen.size).toBe(5);
    expect(getCorrelationId()).toBeNull();
  });

  it("reads the live id on every attempt of a retrying handler", async () => {
    const observed = await runWithContext("retry-30", async () => {
      const reads: (string | null)[] = [];
      for (let attempt = 0; attempt < 4; attempt++) {
        reads.push(getCorrelationId());
        await new Promise<void>((resolve) => setImmediate(resolve));
      }
      return reads;
    });
    expect(observed).toEqual([
      "retry-30",
      "retry-30",
      "retry-30",
      "retry-30",
    ]);
  });
});

// ── 8. Property: no input can put an unsafe value in the context ──────────────

describe("runWithContext — property: the context only ever holds a safe id", () => {
  /**
   * Deterministic pseudo-random string generator (a 32-bit LCG with a fixed
   * seed). Property testing here must be reproducible, so `Math.random` is not
   * used and the corpus is identical on every run.
   */
  function makeHostileString(seed: number, length: number): string {
    // Deliberately weighted towards the characters that make an id unsafe:
    // control characters, whitespace, separators outside the allowed set, and
    // non-ASCII — plus a few legitimate id characters so the accepted branch is
    // exercised too.
    const alphabet = [
      "a", "Z", "0", "-", "_", ".", ":", " ", "\t", "\n", "\r", "\u0000",
      "\u001b", "\u007f", "/", "+", "@", "{", '"', "é", "中",
    ];
    let state = seed >>> 0;
    let out = "";
    for (let i = 0; i < length; i++) {
      // xorshift32, so the sequence is stable across Node versions.
      state ^= state << 13;
      state >>>= 0;
      state ^= state >>> 17;
      state ^= state << 5;
      state >>>= 0;
      out += alphabet[state % alphabet.length];
    }
    return out;
  }

  it("stores either the sanitised input or a fresh safe id, never the raw input", () => {
    for (let seed = 1; seed <= 500; seed++) {
      const raw = makeHostileString(seed, (seed % 200) + 1);
      const sanitised = sanitizeCorrelationId(raw);
      const observed = runWithContext(raw, () => getCorrelationId());

      // Whatever happened, what landed in the context is log-safe.
      expectLogSafeId(observed);

      if (sanitised === null) {
        // Refused input must be replaced, never stored. The replacement is a
        // generated id — canonical, or degraded when the source is unavailable.
        expect(observed).not.toBe(raw);
        expect(observed).toMatch(
          new RegExp(`^(${ULID_RE.source}|${DEGRADED_PREFIX})`)
        );
      } else {
        expect(observed).toBe(sanitised);
      }
    }
  });

  it("holds the same guarantee when the id source is failing", () => {
    failIdSource();
    for (let seed = 1; seed <= 200; seed++) {
      const raw = makeHostileString(seed * 7, (seed % 150) + 1);
      const sanitised = sanitizeCorrelationId(raw);
      const observed = runWithContext(raw, () => getCorrelationId());

      expectLogSafeId(observed);
      if (sanitised === null) {
        expect(observed).not.toBe(raw);
        expect(observed!.startsWith(DEGRADED_PREFIX)).toBe(true);
      } else {
        expect(observed).toBe(sanitised);
      }
    }
  });

  it("holds the same guarantee for ids of every length across the ceiling", () => {
    for (const length of [
      0, 1, 2, 126, 127, 128, 129, 130, 200,
    ]) {
      failIdSource();
      const raw = makeHostileString(length + 1, length);
      const observed = runWithContext(raw, () => getCorrelationId());
      expectLogSafeId(observed);
    }
  });
});

// ── 9. Caller regression ──────────────────────────────────────────────────────

describe("runWithContext — existing callers keep their contracts", () => {
  it("is what the request-context middleware routes a usable id through", () => {
    const middleware = createRequestContextMiddleware();
    let observed: string | null = null;
    middleware({ correlationId: "via-middleware-31" }, {}, () => {
      observed = getCorrelationId();
    });
    expect(observed).toBe("via-middleware-31");
    expect(getCorrelationId()).toBeNull();
  });

  it("leaves a refused middleware id with no context, rather than a minted one", () => {
    // The middleware's documented contract is to proceed without a context and
    // let downstream callers fall back; that is deliberately different from
    // runWithContext's own mint-a-replacement behaviour.
    const middleware = createRequestContextMiddleware();
    let observed: string | null = "sentinel";
    middleware({ correlationId: "refused\nid" }, {}, () => {
      observed = getCorrelationId();
    });
    expect(observed).toBeNull();
  });

  it("invokes the middleware's next() exactly once on every path", () => {
    const middleware = createRequestContextMiddleware();
    const requests: unknown[] = [
      { correlationId: "once-1" },
      { correlationId: "" },
      { correlationId: "a".repeat(MAX_CORRELATION_ID_LENGTH + 1) },
      { correlationId: "refused\nid" },
      {},
    ];
    for (const req of requests) {
      let calls = 0;
      middleware(req as never, {}, () => {
        calls += 1;
      });
      expect(calls).toBe(1);
    }
  });

  it("degrades to no context when a request property accessor throws", () => {
    // A getter that throws must not take the chain down before `next()` runs:
    // the middleware catches the resolution, calls `next()` once with no
    // context, and `runWithContext` is never reached.
    const middleware = createRequestContextMiddleware();
    const hostile = {
      get correlationId(): string {
        throw new Error("hostile getter");
      },
      requestId: "survivor-34",
    };

    let calls = 0;
    let observed: string | null = "sentinel";
    middleware(hostile, {}, () => {
      calls += 1;
      observed = getCorrelationId();
    });

    expect(calls).toBe(1);
    expect(observed).toBeNull();
    expect(getCorrelationId()).toBeNull();
  });

  it("lets the request logger establish the context a handler reads", () => {
    const entries: { requestId: string }[] = [];
    const middleware = createRequestLogger({
      info: (entry: { requestId: string }) => entries.push(entry),
      error: () => {
        /* the probe never triggers a redaction error */
      },
    });

    let observed: string | null = null;
    const req = fakeRequest({ "x-request-id": "logger-32" });
    const res = fakeResponse();

    middleware(req as never, res as never, () => {
      observed = getCorrelationId();
    });

    expect(observed).toBe("logger-32");
    expect((req as { correlationId: string }).correlationId).toBe("logger-32");
    expect(res.headers["X-Request-Id"]).toBe("logger-32");
    res.emit("finish");
    expect(entries).toHaveLength(1);
    expect(entries[0].requestId).toBe("logger-32");
    expect(getCorrelationId()).toBeNull();
  });

  it("refuses a tainted inbound id in the logger and still emits a safe one", () => {
    const entries: { requestId: string }[] = [];
    const middleware = createRequestLogger({
      info: (entry: { requestId: string }) => entries.push(entry),
      error: () => {
        /* the probe never triggers a redaction error */
      },
    });

    let observed: string | null = null;
    const res = fakeResponse();
    const req = fakeRequest({ "x-request-id": "tainted\nINFO forged" });

    middleware(req as never, res as never, () => {
      observed = getCorrelationId();
    });

    // The tainted value is replaced, not stored, and does not reach the log line.
    expect(observed).toMatch(ULID_RE);
    expect(observed).not.toContain("tainted");
    res.emit("finish");
    expect(entries).toHaveLength(1);
    expect(entries[0].requestId).toBe(observed);
    expect(entries[0].requestId).not.toContain("forged");
  });

  it("keeps the logger working while the id source is failing", () => {
    failIdSource();
    const entries: { requestId: string }[] = [];
    const middleware = createRequestLogger({
      info: (entry: { requestId: string }) => entries.push(entry),
      error: () => {
        /* the probe never triggers a redaction error */
      },
    });

    const res = fakeResponse();
    let observed: string | null = null;
    middleware(
      fakeRequest({ "x-request-id": "still-honoured-33" }) as never,
      res as never,
      () => {
        observed = getCorrelationId();
      }
    );

    expect(observed).toBe("still-honoured-33");
    res.emit("finish");
    expect(entries[0].requestId).toBe("still-honoured-33");
  });
});

// ── Fakes ─────────────────────────────────────────────────────────────────────

/**
 * Minimal Express-shaped request. `requestLogger` only reads `path`, `method`,
 * `headers`, `query` and `body`, and writes `requestId` / `correlationId`.
 */
function fakeRequest(headers: Record<string, string> = {}): unknown {
  return {
    path: "/probe",
    method: "GET",
    query: {},
    body: undefined,
    headers,
  };
}

/**
 * Minimal Express-shaped response. `requestLogger` binds over `res.json`,
 * registers a `finish` listener and sets a header, so all three must exist.
 */
function fakeResponse(): {
  headers: Record<string, unknown>;
  statusCode: number;
  json: (body: unknown) => unknown;
  setHeader: (name: string, value: unknown) => void;
  on: (event: string, listener: () => void) => void;
  emit: (event: string) => void;
} {
  const listeners = new Map<string, (() => void)[]>();
  return {
    headers: {},
    statusCode: 200,
    json: (body: unknown) => body,
    setHeader(name: string, value: unknown) {
      this.headers[name] = value;
    },
    on(event: string, listener: () => void) {
      const existing = listeners.get(event) ?? [];
      existing.push(listener);
      listeners.set(event, existing);
    },
    emit(event: string) {
      for (const listener of listeners.get(event) ?? []) listener();
    },
  };
}
