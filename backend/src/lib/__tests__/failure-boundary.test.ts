import { describe, it, expect } from "vitest";
import { FrozenClock } from "../../services/clock";
import { FailureBoundary } from "../failure-boundary";
import { ApiKeyError, apiKeyRetryPredicate } from "../../errors/api-key-error";

const alwaysRetryable = () => true;

const deferred = <T>() => {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
};

describe("FailureBoundary.withRetry", () => {
  it("succeeds on the first attempt", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock });
    const calls: number[] = [];
    const result = await b.withRetry(() => {
      calls.push(1);
      return Promise.resolve(42);
    }, alwaysRetryable);
    expect(result).toBe(42);
    expect(calls).toHaveLength(1);
    expect(clock.scheduledDelays).toHaveLength(0);
  });

  it("retries transient failures with exponential backoff and eventually succeeds", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock, maxAttempts: 4, baseDelayMs: 100 });
    let attempts = 0;
    const result = await b.withRetry(() => {
      attempts++;
      if (attempts < 3) {
        throw new ApiKeyError("TRANSIENT", "flaky", { action: "x" });
      }
      return Promise.resolve("ok");
    }, alwaysRetryable);
    expect(result).toBe("ok");
    expect(attempts).toBe(3);
    // backoff: 100, 200 (2 retries before success)
    expect(clock.scheduledDelays).toEqual([100, 200]);
  });

  it("does not retry terminal (non-retryable) errors", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock, maxAttempts: 5 });
    const terminal = new ApiKeyError("VALIDATION_ERROR", "bad id", {
      action: "x",
    });
    // The realistic predicate (same one the controller uses): a
    // non-retryable AppError must short-circuit immediately.
    await expect(
      b.withRetry(() => Promise.reject(terminal), apiKeyRetryPredicate)
    ).rejects.toBe(terminal);
    expect(clock.scheduledDelays).toHaveLength(0);
  });

  it("respects a custom retry predicate", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock, maxAttempts: 3 });
    let attempts = 0;
    // ApiKeyError with retryable=false (e.g. FORBIDDEN) should not retry
    // even though the error type is "retryable" structurally.
    const predicate = (e: unknown) => e instanceof ApiKeyError && e.retryable;
    const err = new ApiKeyError("FORBIDDEN", "nope", { action: "x" });
    await expect(
      b.withRetry(() => {
        attempts++;
        return Promise.reject(err);
      }, predicate)
    ).rejects.toBe(err);
    expect(attempts).toBe(1);
  });

  it("exhausts attempts and rethrows the last error", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock, maxAttempts: 3, baseDelayMs: 50 });
    let attempts = 0;
    const sentinel = new ApiKeyError("TRANSIENT", "always down", {
      action: "x",
    });
    await expect(
      b.withRetry(() => {
        attempts++;
        return Promise.reject(sentinel);
      }, alwaysRetryable)
    ).rejects.toBe(sentinel);
    expect(attempts).toBe(3);
    // 2 retries -> backoff 50, 100
    expect(clock.scheduledDelays).toEqual([50, 100]);
  });

  it("does not retry when maxAttempts is 1", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock, maxAttempts: 1 });
    const sentinel = new ApiKeyError("TRANSIENT", "down", { action: "x" });
    let attempts = 0;
    await expect(
      b.withRetry(() => {
        attempts++;
        return Promise.reject(sentinel);
      }, alwaysRetryable)
    ).rejects.toBe(sentinel);
    expect(attempts).toBe(1);
    expect(clock.scheduledDelays).toHaveLength(0);
  });
});

describe("FailureBoundary.dedupe", () => {
  it("deduplicates concurrent calls sharing the same key", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock });
    let calls = 0;
    // A never-resolving promise so concurrent callers overlap.
    let resolveShared: (v: string) => void = () => {};
    const op = () =>
      new Promise<string>((resolve) => {
        calls++;
        resolveShared = resolve;
      });

    const p1 = b.dedupe("shared", op);
    const p2 = b.dedupe("shared", op);
    const p3 = b.dedupe("shared", op);

    expect(calls).toBe(1); // only one underlying execution
    expect(b.isPending("shared")).toBe(true);

    resolveShared("value");
    const [r1, r2, r3] = await Promise.all([p1, p2, p3]);
    expect(r1).toBe("value");
    expect(r2).toBe("value");
    expect(r3).toBe("value");
    expect(calls).toBe(1);
    expect(b.isPending("shared")).toBe(false);
  });

  it("treats different keys as independent executions", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock });
    let calls = 0;
    const op = async () => {
      calls++;
      return calls;
    };
    const [a, c] = await Promise.all([b.dedupe("a", op), b.dedupe("b", op)]);
    expect(a).toBe(1);
    expect(c).toBe(2);
    expect(calls).toBe(2);
  });

  it("clears a failed entry so a later caller can retry", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock });
    let calls = 0;
    const op = () => {
      calls++;
      return Promise.reject(new Error("boom"));
    };
    await expect(b.dedupe("k", op)).rejects.toThrow("boom");
    expect(b.isPending("k")).toBe(false);
    // After failure, a new caller re-executes rather than sharing a rejection.
    let secondCalls = 0;
    await b.dedupe("k", async () => {
      secondCalls++;
      return Promise.resolve("recovered");
    });
    expect(secondCalls).toBe(1);
    expect(calls).toBe(1);
  });

  it("cancel() drops the in-flight entry without affecting the running op", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock });
    const def = deferred<string>();
    const p = b.dedupe("key", () => def.promise);
    expect(b.isPending("key")).toBe(true);

    b.cancel("key");
    expect(b.isPending("key")).toBe(false);

    // The original op is unaffected; resolving it still satisfies the caller.
    def.resolve("done");
    await expect(p).resolves.toBe("done");
  });
});

describe("FailureBoundary — guard rails", () => {
  it("rejects maxAttempts < 1 in withRetry", async () => {
    const clock = new FrozenClock();
    const b = new FailureBoundary({ clock, maxAttempts: 0 });
    await expect(
      b.withRetry(() => Promise.resolve("x"), alwaysRetryable)
    ).rejects.toThrow(RangeError);
  });
});
