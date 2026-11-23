/**
 * Deterministic failure-boundary coverage for `src/lib/requestContext.ts` and,
 * in particular, `createRequestContextMiddleware`.
 *
 * Strategy:
 * - The middleware is driven directly with a bare `req` literal. No Express app,
 *   no network, no database, no timers beyond the ones a case schedules itself,
 *   so every assertion is fixed to an explicit id.
 * - Observable state is `getCorrelationId()` as read by the handler from inside
 *   `next()`, plus whether `next()` ran and how many times. Nothing reaches into
 *   the private `AsyncLocalStorage` instance.
 * - Rejection cases assert two things together: the tainted value never reaches
 *   the context, *and* `next()` still runs exactly once. A request must never be
 *   stalled or dropped because its correlation id was unusable.
 * - A sentinel string is planted in every hostile id and asserted absent from the
 *   context, so a future refactor cannot leak untrusted input into the log stream
 *   that consumes this context.
 * - ULID ordering is deliberately never asserted. `ulid()` is only monotonic
 *   within a millisecond under `monotonicFactory`; asserting order on plain
 *   `ulid()` output would produce a test that fails intermittently for reasons
 *   unrelated to this module. Format and uniqueness are asserted instead.
 * - The defensive `catch` in `getCorrelationId()` is the one branch unreachable
 *   from the public surface, so it is covered by re-importing the module with
 *   `node:async_hooks` mocked to throw out of `getStore()`.
 */

import * as fc from "fast-check";
import {
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

const ULID_RE = /^[0-9A-HJKMNP-TV-Z]{26}$/;
// Must mirror the charset in sanitizeCorrelationId exactly. Alphanumeric plus
// "-", "_", "." and ":" — the dot and colon were widened upstream in #2788 and
// are deliberately accepted here rather than re-rejected.
const SAFE_ID_RE = /^[A-Za-z0-9_.:-]{1,128}$/;
const ULID_LENGTH = 26;
const MAX_ID_LENGTH = 128;
const SENTINEL = "sentinel-must-not-leak-7f3a";

type Handlers = (() => void) | undefined;
type Outcome = { calls: number; observed: string | null; returned: unknown };

/**
 * Drive one request through the middleware and report what the handler saw.
 * `observed` is the correlation id readable from inside `next()`.
 */
function invoke(req: unknown, onNext?: () => void): Outcome {
  const middleware = createRequestContextMiddleware();
  let calls = 0;
  let observed: string | null = null;
  let ran = false;
  const returned = middleware(req as never, {} as never, (() => {
    calls += 1;
    ran = true;
    observed = getCorrelationId();
    onNext?.();
  }) as never);
  // `ran` guards against an outcome that was never produced.
  if (!ran) observed = null;
  return { calls, observed, returned };
}

/** Drive a request whose handler defers its assertion, to test async boundaries. */
function invokeAsync(
  req: unknown,
  work: () => Promise<string | null>
): Promise<{ calls: number; id: string | null }> {
  const middleware = createRequestContextMiddleware();
  let calls = 0;
  return new Promise((resolve, reject) => {
    middleware(req as never, {} as never, (() => {
      calls += 1;
      work().then((id) => resolve({ calls, id }), reject);
    }) as never);
  });
}

/** Run `fn` and return whatever it threw, or null when it returned normally. */
function captureThrown(fn: () => unknown): unknown {
  try {
    fn();
  } catch (err) {
    return err;
  }
  return null;
}

// ── requestContextMiddleware: success paths ──────────────────────────────────

describe("requestContextMiddleware success paths", () => {
  test("establishes a context from req.correlationId", () => {
    const outcome = invoke({ correlationId: "req-corr-1" });
    expect(outcome.observed).toBe("req-corr-1");
    expect(outcome.calls).toBe(1);
  });

  test("falls back to req.requestId when correlationId is absent", () => {
    const outcome = invoke({ requestId: "req-fallback-2" });
    expect(outcome.observed).toBe("req-fallback-2");
    expect(outcome.calls).toBe(1);
  });

  test("prefers correlationId when both ids are present", () => {
    const outcome = invoke({ correlationId: "primary-3", requestId: "secondary-3" });
    expect(outcome.observed).toBe("primary-3");
  });

  test("trims surrounding whitespace the way request-logger does", () => {
    const outcome = invoke({ correlationId: "  padded-id-4  " });
    expect(outcome.observed).toBe("padded-id-4");
  });

  test("accepts an id at the 128 character limit", () => {
    const id = "a".repeat(MAX_ID_LENGTH);
    const outcome = invoke({ correlationId: id });
    expect(outcome.observed).toBe(id);
  });

  test("returns undefined and does not leak a next() return value", () => {
    const middleware = createRequestContextMiddleware();
    const result = middleware({ correlationId: "ret-5" } as never, {} as never, (() => {
      return "next-return-value";
    }) as never);
    expect(result).toBeUndefined();
  });

  test("is idempotent: the same request yields the same observable outcome", () => {
    const first = invoke({ correlationId: "stable-6", requestId: "ignored-6" });
    const second = invoke({ correlationId: "stable-6", requestId: "ignored-6" });
    expect(second).toEqual(first);
  });
});

// ── requestContextMiddleware: regression tests for the boundaries ─────────────

describe("requestContextMiddleware regression boundaries", () => {
  test("an empty correlationId must not mask a usable requestId", () => {
    // `??` treats "" as present, so the empty value used to win over the valid
    // requestId and the whole request ran with no correlation id at all.
    const outcome = invoke({ correlationId: "", requestId: "survivor-7" });
    expect(outcome.observed).toBe("survivor-7");
    expect(outcome.calls).toBe(1);
  });

  test("a whitespace-only correlationId must not mask a usable requestId", () => {
    const outcome = invoke({ correlationId: "   ", requestId: "survivor-8" });
    expect(outcome.observed).toBe("survivor-8");
  });

  test("an unparseable correlationId must not mask a usable requestId", () => {
    const outcome = invoke({
      correlationId: `bad id ${SENTINEL}`,
      requestId: "survivor-9",
    });
    expect(outcome.observed).toBe("survivor-9");
  });

  test("a log-injection attempt in correlationId never reaches the context", () => {
    // The module header promises log-injection prevention for any id placed into
    // the context; every downstream log line reads this value.
    const hostile = `trace-10\nINFO forged log line ${SENTINEL}`;
    const outcome = invoke({ correlationId: hostile });
    const leaked = outcome.observed ?? "";
    expect(leaked).not.toContain(SENTINEL);
    expect(leaked).not.toContain("\n");
    expect(outcome.observed).toBeNull();
    expect(outcome.calls).toBe(1);
  });

  test("a carriage return and terminal escape in the id never reach the context", () => {
    for (const hostile of [
      "trace-11\r\nforged",
      "trace-12\u001b[31mred",
      "trace-13\ttabbed",
      "trace-14\u0000null-byte",
      "trace-15\u007fdelete",
    ]) {
      const outcome = invoke({ correlationId: hostile });
      expect(outcome.observed).toBeNull();
      expect(outcome.calls).toBe(1);
    }
  });

  test("an id longer than 128 characters is rejected", () => {
    const outcome = invoke({ correlationId: "b".repeat(MAX_ID_LENGTH + 1) });
    expect(outcome.observed).toBeNull();
    expect(outcome.calls).toBe(1);
  });

  test("a missing req does not crash the chain", () => {
    // A malformed request must not take the request down before `next()` runs.
    for (const req of [null, undefined, {}]) {
      const outcome = invoke(req);
      expect(outcome.observed).toBeNull();
      expect(outcome.calls).toBe(1);
    }
  });

  test("a non-object req is tolerated the same way", () => {
    for (const req of ["correlationId=x", 42, true]) {
      const outcome = invoke(req);
      expect(outcome.observed).toBeNull();
      expect(outcome.calls).toBe(1);
    }
  });
});

// ── requestContextMiddleware: rejection without stalling the request ──────────

describe("requestContextMiddleware rejection without stalling the request", () => {
  const rejected: ReadonlyArray<readonly [string, unknown]> = [
    ["empty string", ""],
    ["whitespace only", "  \t "],
    ["number", 42],
    ["boolean", false],
    ["null", null],
    ["undefined", undefined],
    ["object", { nested: "id" }],
    ["array", ["id"]],
    ["string with internal space", "two words"],
    ["string with slash", "tenant/abc"],
    ["string with plus", "a+b"],
    ["over-long string", "c".repeat(MAX_ID_LENGTH + 1)],
  ];

  test.each(rejected)("rejects a %s id but still calls next exactly once", (_label, value) => {
    const outcome = invoke({ correlationId: value });
    expect(outcome.observed).toBeNull();
    expect(outcome.calls).toBe(1);
  });

  test("rejects an unusable requestId when correlationId is absent too", () => {
    const outcome = invoke({ requestId: "bad id" });
    expect(outcome.observed).toBeNull();
    expect(outcome.calls).toBe(1);
  });

  test("an id that reaches the context is always sanitized", () => {
    for (const value of ["ok-16", " ok-17 ", "a".repeat(MAX_ID_LENGTH), 7, "", "x/y"]) {
      const observed = invoke({ correlationId: value }).observed;
      if (observed !== null) {
        expect(observed).toMatch(SAFE_ID_RE);
      }
    }
  });

  test("a malformed next() fails loudly instead of being silently swallowed", () => {
    // Documented boundary: the middleware does not paper over a caller bug. The
    // failure surfaces at the call site rather than as a request that hangs.
    // The error is matched on `name`, not on the constructor, because the throw
    // originates inside node:internal/async_hooks and its prototype comes from a
    // different realm than this test's TypeError.
    const withId = createRequestContextMiddleware();
    const withoutId = createRequestContextMiddleware();
    const failures = [
      captureThrown(() => withId({ correlationId: "loud-18" } as never, {} as never, undefined as never)),
      captureThrown(() => withoutId({} as never, {} as never, "not-a-function" as never)),
    ];

    for (const failure of failures) {
      expect(failure).not.toBeNull();
      expect((failure as Error).name).toBe("TypeError");
    }
    expect(getCorrelationId()).toBeNull();
  });
});

// ── requestContextMiddleware: error propagation and teardown ──────────────────

describe("requestContextMiddleware error propagation and teardown", () => {
  test("an error thrown by next() propagates unchanged to the caller", () => {
    const middleware = createRequestContextMiddleware();
    const failure = new Error("downstream handler exploded");
    expect(() =>
      middleware({ correlationId: "boom-19" } as never, {} as never, (() => {
        throw failure;
      }) as never)
    ).toThrow(failure);
  });

  test("the context is torn down when next() throws", () => {
    let observedInside: string | null = "unset";
    let observedAfter: string | null = "unset";
    try {
      invoke({ correlationId: "boom-20" }, () => {
        observedInside = getCorrelationId();
        throw new Error("handler failure");
      });
    } catch {
      observedAfter = getCorrelationId();
    }
    expect(observedInside).toBe("boom-20");
    expect(observedAfter).toBeNull();
  });

  test("an outer context is intact again after an inner handler throws", () => {
    const outcome = runWithContext("outer-21", () => {
      try {
        invoke({ correlationId: "inner-21" }, () => {
          throw new Error("inner failure");
        });
      } catch {
        // swallowed on purpose: the assertion is about the restored context
      }
      return getCorrelationId();
    });
    expect(outcome).toBe("outer-21");
  });

  test("degrades to no context when a property access on req throws", () => {
    // The resolution is guarded, so a hostile getter on `req` cannot take the
    // chain down before next() — it only costs observability for this request.
    const hostile = {
      get correlationId(): string {
        throw new Error("getter exploded");
      },
    } as unknown as { correlationId?: string };
    const outcome = invoke(hostile);
    expect(outcome.observed).toBeNull();
    expect(outcome.calls).toBe(1);
  });

  test("no context is visible on a later tick after the request completed", async () => {
    expect(getCorrelationId()).toBeNull();
    invoke({ correlationId: "tick-22" });
    expect(getCorrelationId()).toBeNull();
    await new Promise<void>((resolve) => {
      setImmediate(() => {
        expect(getCorrelationId()).toBeNull();
        resolve();
      });
    });
  });
});

// ── requestContextMiddleware: async and concurrency boundaries ────────────────

describe("requestContextMiddleware async and concurrency boundaries", () => {
  test("the id stays readable after the middleware has already returned", async () => {
    let deferred: string | null = null;
    invoke({ correlationId: "async-23" }, () => {
      setImmediate(() => {
        deferred = getCorrelationId();
      });
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 5));
    expect(deferred).toBe("async-23");
  });

  test("the id survives await, setTimeout and setImmediate inside the handler", async () => {
    const seen: string[] = [];
    const { id } = await invokeAsync({ correlationId: "async-24" }, async () => {
      seen.push(String(getCorrelationId()));
      await Promise.resolve();
      seen.push(String(getCorrelationId()));
      await new Promise<void>((r) => setTimeout(r, 1));
      seen.push(String(getCorrelationId()));
      await new Promise<void>((r) => setImmediate(r));
      seen.push(String(getCorrelationId()));
      return getCorrelationId();
    });
    expect(seen).toEqual(["async-24", "async-24", "async-24", "async-24"]);
    expect(id).toBe("async-24");
  });

  test("interleaved concurrent requests never observe each other's id", async () => {
    const ids = Array.from({ length: 20 }, (_v, i) => `concurrent-${i}`);
    const results = await Promise.all(
      ids.map((id, index) =>
        invokeAsync({ correlationId: id }, async () => {
          // Deterministic stagger, not a random sleep: the requests interleave on
          // the event loop but the case stays reproducible.
          await new Promise<void>((r) => setTimeout(r, index % 4));
          const before = getCorrelationId();
          await new Promise<void>((r) => setImmediate(r));
          const after = getCorrelationId();
          if (before !== after) throw new Error("context changed mid-request");
          return before;
        })
      )
    );
    expect(results.map((r) => r.id)).toEqual(ids);
    expect(results.every((r) => r.calls === 1)).toBe(true);
  });

  test("a request without an id does not disturb a concurrent one", async () => {
    const [withId, withoutId] = await Promise.all([
      invokeAsync({ correlationId: "solo-25" }, async () => {
        await new Promise<void>((r) => setTimeout(r, 2));
        return getCorrelationId();
      }),
      invokeAsync({}, async () => {
        await new Promise<void>((r) => setTimeout(r, 1));
        return getCorrelationId();
      }),
    ]);
    expect(withId.id).toBe("solo-25");
    expect(withoutId.id).toBeNull();
  });

  test("mounting the middleware twice nests and restores the outer context", () => {
    const outer = createRequestContextMiddleware();
    const inner = createRequestContextMiddleware();
    const seen: Array<string | null> = [];

    outer({ correlationId: "outer-26" } as never, {} as never, (() => {
      seen.push(getCorrelationId());
      inner({ correlationId: "inner-26" } as never, {} as never, (() => {
        seen.push(getCorrelationId());
      }) as never);
      seen.push(getCorrelationId());
    }) as never);

    expect(seen).toEqual(["outer-26", "inner-26", "outer-26"]);
    expect(getCorrelationId()).toBeNull();
  });
});

// ── duplicate correlation ids ────────────────────────────────────────────────

describe("duplicate correlation ids", () => {
  test("two concurrent requests sharing one id do not cross-talk", async () => {
    // Two clients legitimately quoting the same trace id must not be able to
    // observe each other's context, and the collision must not be mistaken for
    // a missing id.
    const shared = "shared-41";
    const [first, second] = await Promise.all([
      invokeAsync({ correlationId: shared }, async () => {
        await new Promise<void>((r) => setTimeout(r, 1));
        return getCorrelationId();
      }),
      invokeAsync({ correlationId: shared }, async () => {
        await new Promise<void>((r) => setImmediate(r));
        return getCorrelationId();
      }),
    ]);

    expect(first.id).toBe(shared);
    expect(second.id).toBe(shared);
    expect(first.calls).toBe(1);
    expect(second.calls).toBe(1);
  });

  test("the same req driven twice yields the same outcome with no bleed", () => {
    const req = { correlationId: "replayed-42", requestId: "ignored-42" };
    const first = invoke(req);
    const second = invoke(req);

    expect(second).toEqual(first);
    expect(first.observed).toBe("replayed-42");
    expect(first.calls).toBe(1);
    expect(getCorrelationId()).toBeNull();
  });

  test("a duplicate id on both mounts reads the same value and still unwinds", () => {
    const outer = createRequestContextMiddleware();
    const inner = createRequestContextMiddleware();
    const seen: Array<string | null> = [];

    outer({ correlationId: "twin-43" } as never, {} as never, (() => {
      seen.push(getCorrelationId());
      inner({ correlationId: "twin-43" } as never, {} as never, (() => {
        seen.push(getCorrelationId());
      }) as never);
      seen.push(getCorrelationId());
    }) as never);

    expect(seen).toEqual(["twin-43", "twin-43", "twin-43"]);
    expect(getCorrelationId()).toBeNull();
  });

  test("the same unusable id in both fields still degrades to no context", () => {
    const hostile = "dup bad id\nforged";
    const outcome = invoke({ correlationId: hostile, requestId: hostile });
    expect(outcome.observed).toBeNull();
    expect(outcome.calls).toBe(1);
  });
});

// ── retried requests ─────────────────────────────────────────────────────────

describe("retried requests", () => {
  test("a handler retrying an operation re-reads the same id on every attempt", async () => {
    const attempts: Array<string | null> = [];

    const { id } = await invokeAsync({ correlationId: "retry-44" }, async () => {
      for (let attempt = 0; attempt < 3; attempt += 1) {
        await new Promise<void>((r) => setTimeout(r, 1));
        attempts.push(getCorrelationId());
      }
      return getCorrelationId();
    });

    expect(attempts).toEqual(["retry-44", "retry-44", "retry-44"]);
    expect(id).toBe("retry-44");
  });

  test("a retry that re-enters the context nests and restores the request id", () => {
    const outcome = invoke({ correlationId: "retry-45" }, () => {
      runWithContext("retry-attempt-45", () => getCorrelationId());
      return getCorrelationId();
    });

    expect(outcome.observed).toBe("retry-45");
    expect(getCorrelationId()).toBeNull();
  });

  test("a retry after a thrown error starts from a clean context", () => {
    const middleware = createRequestContextMiddleware();
    const observed: Array<string | null> = [];

    const attempt = (correlationId: string) => () => {
      try {
        middleware({ correlationId } as never, {} as never, (() => {
          observed.push(getCorrelationId());
          throw new Error("attempt failed");
        }) as never);
      } catch {
        // swallowed on purpose: the assertion is about the next attempt's context
      }
    };

    attempt("retry-46")();
    expect(getCorrelationId()).toBeNull();
    attempt("retry-46")();

    expect(observed).toEqual(["retry-46", "retry-46"]);
    expect(getCorrelationId()).toBeNull();
  });

  test("getOrGenerateCorrelationId inside a retry returns the live id, not a fresh one", async () => {
    const { id } = await invokeAsync({ correlationId: "retry-47" }, async () => {
      await new Promise<void>((r) => setTimeout(r, 1));
      const first = getOrGenerateCorrelationId();
      await new Promise<void>((r) => setTimeout(r, 1));
      const second = getOrGenerateCorrelationId();
      expect(first).toBe("retry-47");
      expect(second).toBe("retry-47");
      return getCorrelationId();
    });

    expect(id).toBe("retry-47");
  });

  test("a retry of a request whose id was rejected still runs next exactly once", () => {
    const req = { correlationId: "bad id retry-48" };
    const first = invoke(req);
    const second = invoke(req);

    expect(first.observed).toBeNull();
    expect(second.observed).toBeNull();
    expect(first.calls).toBe(1);
    expect(second.calls).toBe(1);
  });
});

// ── getCorrelationId ─────────────────────────────────────────────────────────

describe("getCorrelationId", () => {
  test("is null outside any request context", () => {
    expect(getCorrelationId()).toBeNull();
  });

  test("returns the id of the active context", () => {
    expect(runWithContext("read-27", () => getCorrelationId())).toBe("read-27");
  });

  test("falls back to a generated id rather than exposing a non-string one", () => {
    // runWithContext refuses to store an unusable id and mints one instead, so
    // the reader must never hand a non-string back to a caller.
    for (const bad of [undefined, 42, {}] as unknown[]) {
      const seen = runWithContext(bad as never, () => getCorrelationId());
      expect(typeof seen).toBe("string");
      expect(seen).toMatch(ULID_RE);
    }
  });

  test("falls back to a generated id rather than exposing an empty one", () => {
    const seen = runWithContext("", () => getCorrelationId());
    expect(seen).not.toBe("");
    expect(seen).toMatch(ULID_RE);
  });

  test("returns the innermost context", () => {
    const seen = runWithContext("a-28", () =>
      runWithContext("b-28", () => getCorrelationId())
    );
    expect(seen).toBe("b-28");
  });

  test("degrades to null when the async-hooks host itself fails", () => {
    // The only way to reach the defensive catch: a store provider that throws is
    // unrepresentable through the public API, so the host is stubbed instead.
    jest.resetModules();
    jest.doMock("node:async_hooks", () => ({
      AsyncLocalStorage: class {
        run<T>(_store: unknown, fn: () => T): T {
          return fn();
        }
        getStore(): unknown {
          throw new Error("async_hooks host failure");
        }
      },
    }));

    let isolated: typeof import("../lib/requestContext") | undefined;
    jest.isolateModules(() => {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      isolated = require("../lib/requestContext") as typeof import("../lib/requestContext");
    });

    expect(isolated?.getCorrelationId()).toBeNull();

    jest.dontMock("node:async_hooks");
    jest.resetModules();
  });

  test("refuses to read back a store entry that is not a non-empty string", () => {
    // runWithContext sanitizes before storing, so a tainted entry can only be
    // produced by a partially constructed context. The host is stubbed to plant
    // one, because the public API cannot.
    const tainted: unknown[] = [42, "", "bad\nid", null, undefined];
    for (const planted of tainted) {
      jest.resetModules();
      jest.doMock("node:async_hooks", () => ({
        AsyncLocalStorage: class {
          run<T>(_store: unknown, fn: () => T): T {
            return fn();
          }
          getStore(): unknown {
            return { correlationId: planted };
          }
        },
      }));

      let isolated: typeof import("../lib/requestContext") | undefined;
      jest.isolateModules(() => {
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        isolated = require("../lib/requestContext") as typeof import("../lib/requestContext");
      });

      expect(isolated?.getCorrelationId()).toBeNull();
      jest.dontMock("node:async_hooks");
    }
    jest.resetModules();
  });
});

// ── runWithContext / withCorrelationId ────────────────────────────────────────

describe("runWithContext", () => {
  test("returns the callback's return value", () => {
    expect(runWithContext("ret-29", () => 7 * 6)).toBe(42);
  });

  test("preserves an async callback and its result", async () => {
    const value = await runWithContext("ret-30", async () => {
      await Promise.resolve();
      return getCorrelationId();
    });
    expect(value).toBe("ret-30");
  });

  test("restores the outer context when a nested run completes", () => {
    const seen = runWithContext("outer-31", () => {
      runWithContext("inner-31", () => getCorrelationId());
      return getCorrelationId();
    });
    expect(seen).toBe("outer-31");
  });

  test("propagates a thrown error and restores the outer context", () => {
    const seen = runWithContext("outer-32", () => {
      expect(() =>
        runWithContext("inner-32", () => {
          throw new Error("inner failure");
        })
      ).toThrow("inner failure");
      return getCorrelationId();
    });
    expect(seen).toBe("outer-32");
  });

  test("leaves no context behind after the run completes", () => {
    runWithContext("leak-33", () => getCorrelationId());
    expect(getCorrelationId()).toBeNull();
  });
});

describe("withCorrelationId", () => {
  test("behaves identically to runWithContext", () => {
    expect(withCorrelationId("alias-34", () => getCorrelationId())).toBe(
      runWithContext("alias-34", () => getCorrelationId())
    );
  });

  test("returns the callback's return value", () => {
    expect(withCorrelationId("alias-35", () => "value")).toBe("value");
  });

  test("propagates the id through an async callback", async () => {
    const value = await withCorrelationId("alias-36", async () => {
      await new Promise<void>((r) => setTimeout(r, 1));
      return getCorrelationId();
    });
    expect(value).toBe("alias-36");
  });
});

// ── getOrGenerateCorrelationId / generateCorrelationId ───────────────────────

describe("getOrGenerateCorrelationId", () => {
  test("returns the active id when a context exists", () => {
    const seen = runWithContext("active-37", () => getOrGenerateCorrelationId());
    expect(seen).toBe("active-37");
  });

  test("generates a fresh ULID outside any context", () => {
    const generated = getOrGenerateCorrelationId();
    expect(generated).toMatch(ULID_RE);
    expect(generated).toHaveLength(ULID_LENGTH);
  });

  test("generates a distinct ULID on every call outside a context", () => {
    const generated = new Set(Array.from({ length: 100 }, () => getOrGenerateCorrelationId()));
    expect(generated.size).toBe(100);
    for (const id of generated) {
      expect(id).toMatch(ULID_RE);
    }
  });

  test("prefers the innermost context over generating", () => {
    const seen = runWithContext("outer-38", () =>
      runWithContext("inner-38", () => getOrGenerateCorrelationId())
    );
    expect(seen).toBe("inner-38");
  });
});

describe("generateCorrelationId", () => {
  test("returns a URL-safe, Crockford-base32 ULID", () => {
    const id = generateCorrelationId();
    expect(id).toMatch(ULID_RE);
    expect(id).toHaveLength(ULID_LENGTH);
    expect(sanitizeCorrelationId(id)).toBe(id);
  });

  test("returns a distinct id on every call", () => {
    const generated = new Set(Array.from({ length: 100 }, () => generateCorrelationId()));
    expect(generated.size).toBe(100);
  });

  test("asserts format and uniqueness, not ordering", () => {
    // Plain ulid() is not monotonic within a millisecond, so no ordering claim is
    // made here; this case exists to document that omission deliberately.
    const ids = [generateCorrelationId(), generateCorrelationId(), generateCorrelationId()];
    for (const id of ids) expect(id).toMatch(ULID_RE);
    expect(new Set(ids).size).toBe(3);
  });

  describe("fallback chain when the ULID generator fails", () => {
    afterEach(() => {
      _resetUlidGeneratorForTesting();
      jest.restoreAllMocks();
    });

    test("falls back to a crypto-backed id when the generator throws", () => {
      _setUlidGeneratorForTesting(() => {
        throw new Error("PRNG exhausted");
      });
      const id = generateCorrelationId();
      expect(id).toHaveLength(ULID_LENGTH);
      expect(id).toMatch(/^[0-9A-Z]+$/);
    });

    test("falls back when the generator returns a non-string or empty value", () => {
      for (const bogus of [undefined, null, "", 42]) {
        _setUlidGeneratorForTesting(() => bogus as never);
        expect(generateCorrelationId()).toHaveLength(ULID_LENGTH);
      }
    });

    test("falls through to the timestamp-entropy id when crypto is unavailable", () => {
      jest.resetModules();
      jest.doMock("node:crypto", () => ({}));
      jest.isolateModules(() => {
        // A fresh module instance has its own hook and its own generator state,
        // so both have to be stubbed inside this registry.
        const fresh = require("../lib/requestContext");
        fresh._setUlidGeneratorForTesting(() => {
          throw new Error("no ULID");
        });
        const id = fresh.generateCorrelationId();
        expect(id).toHaveLength(ULID_LENGTH);
        expect(id).toMatch(/^[0-9A-Z]+$/);
      });
      jest.dontMock("node:crypto");
    });
  });

  test("the testing hook round-trips to the standard implementation", () => {
    _setUlidGeneratorForTesting(() => "injected-id");
    expect(generateCorrelationId()).toBe("injected-id");
    _resetUlidGeneratorForTesting();
    expect(generateCorrelationId()).toMatch(ULID_RE);
  });
});

// ── sanitizeCorrelationId ─────────────────────────────────────────────────────

describe("sanitizeCorrelationId", () => {
  const accepted = [
    "a",
    "0",
    "A-b_c",
    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
    "req-abc-123",
    // Dot and colon are part of the charset widened upstream in #2788 and must
    // stay accepted: this suite adopts that decision rather than re-narrowing it.
    "trace.1",
    "trace:1",
    "svc.node:8080",
    "x".repeat(MAX_ID_LENGTH),
  ];
  const rejected: ReadonlyArray<readonly [string, unknown]> = [
    ["empty string", ""],
    ["single space", " "],
    ["tab only", "\t"],
    ["newline only", "\n"],
    ["internal space", "two words"],
    ["internal tab", "a\tb"],
    ["internal newline", "a\nb"],
    ["null byte", "abc\u0000"],
    ["ANSI escape", "\u001b[31mabc\u001b[0m"],
    ["slash", "a/b"],
    ["plus", "a+b"],
    ["at sign", "a@b"],
    ["brace", "{a}"],
    ["quote", 'a"b'],
    ["unicode letter", "abcé"],
    ["emoji", "ab\ud83d\ude00"],
    ["129 characters", "y".repeat(MAX_ID_LENGTH + 1)],
    ["undefined", undefined],
    ["null", null],
    ["number", 7],
    ["bigint", BigInt(7)],
    ["boolean", true],
    ["object", { id: "abc" }],
    ["array", ["abc"]],
    ["function", () => "abc"],
    ["symbol", Symbol("abc")],
  ];

  test.each(accepted)("accepts %j", (value) => {
    expect(sanitizeCorrelationId(value)).toBe(value);
  });

  test("trims surrounding whitespace before validating", () => {
    expect(sanitizeCorrelationId("  padded-39  ")).toBe("padded-39");
    expect(sanitizeCorrelationId("\tpadded-40\n")).toBe("padded-40");
    expect(sanitizeCorrelationId(" padded \n id ")).toBeNull();
  });

  test("trims surrounding control whitespace rather than rejecting it", () => {
    // Only *interior* whitespace is hostile: a header transport may hand over
    // "abc\r" from a bare CR line ending, and that must still resolve to "abc".
    expect(sanitizeCorrelationId("abc\r")).toBe("abc");
    expect(sanitizeCorrelationId("\tabc")).toBe("abc");
    expect(sanitizeCorrelationId("abc\n")).toBe("abc");
    expect(sanitizeCorrelationId("a b")).toBeNull();
    expect(sanitizeCorrelationId("a\tb")).toBeNull();
    expect(sanitizeCorrelationId("a\nb")).toBeNull();
  });

  test.each(rejected)("rejects %s", (_label, value) => {
    expect(sanitizeCorrelationId(value)).toBeNull();
  });

  test("accepts exactly 128 characters and rejects 129", () => {
    expect(sanitizeCorrelationId("z".repeat(MAX_ID_LENGTH))).toHaveLength(MAX_ID_LENGTH);
    expect(sanitizeCorrelationId("z".repeat(MAX_ID_LENGTH + 1))).toBeNull();
  });

  test("is idempotent for every accepted value", () => {
    for (const value of accepted) {
      const once = sanitizeCorrelationId(value);
      expect(sanitizeCorrelationId(once)).toBe(once);
    }
  });

  test("never returns a value carrying a control character", () => {
    const hostile = `safe${SENTINEL}`;
    const result = sanitizeCorrelationId(hostile);
    if (result !== null) {
      expect(/[\u0000-\u001f\u007f]/.test(result)).toBe(false);
    }
    expect(sanitizeCorrelationId(`bad\n${SENTINEL}`)).toBeNull();
  });
});

// ── properties ────────────────────────────────────────────────────────────────

describe("sanitizeCorrelationId properties", () => {
  test("returns null or a value matching the safe id shape", () => {
    fc.assert(
      fc.property(fc.string(), (raw) => {
        const result = sanitizeCorrelationId(raw);
        if (result === null) return;
        expect(result).toMatch(SAFE_ID_RE);
        expect(result).toBe(raw.trim());
      }),
      { numRuns: 400 }
    );
  });

  test("is idempotent over arbitrary strings", () => {
    fc.assert(
      fc.property(fc.string(), (raw) => {
        const once = sanitizeCorrelationId(raw);
        expect(sanitizeCorrelationId(once)).toBe(once);
      }),
      { numRuns: 300 }
    );
  });

  test("agrees with the documented pattern on generated candidates", () => {
    fc.assert(
      fc.property(
        fc.stringMatching(/^[A-Za-z0-9 _\-\t]{0,140}$/),
        (raw) => {
          const trimmed = raw.trim();
          const expected =
            trimmed.length > 0 && trimmed.length <= MAX_ID_LENGTH && /^[A-Za-z0-9_-]+$/.test(trimmed)
              ? trimmed
              : null;
          expect(sanitizeCorrelationId(raw)).toBe(expected);
        }
      ),
      { numRuns: 400 }
    );
  });
});

describe("requestContextMiddleware properties", () => {
  const requestShape = fc.oneof(
    fc.record({
      correlationId: fc.oneof(fc.string(), fc.integer(), fc.constant(null)),
      requestId: fc.oneof(fc.string(), fc.integer(), fc.constant(undefined)),
    }),
    fc.record({}),
    fc.constant(null),
    fc.constant(undefined)
  ) as fc.Arbitrary<unknown>;

  test("never leaves a tainted id in the context, and always calls next once", () => {
    fc.assert(
      fc.property(requestShape, (req) => {
        const outcome = invoke(req);
        expect(outcome.calls).toBe(1);
        if (outcome.observed !== null) {
          expect(outcome.observed).toMatch(SAFE_ID_RE);
        }
      }),
      { numRuns: 300 }
    );
  });

  test("an accepted id is always the sanitized form of one of the two fields", () => {
    fc.assert(
      fc.property(fc.string(), fc.string(), (correlationId, requestId) => {
        const outcome = invoke({ correlationId, requestId });
        const expected =
          sanitizeCorrelationId(correlationId) ?? sanitizeCorrelationId(requestId);
        expect(outcome.observed).toBe(expected);
      }),
      { numRuns: 300 }
    );
  });
});
