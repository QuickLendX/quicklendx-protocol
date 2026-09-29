import { withCorrelationId } from "../lib/requestContext";
import {
  endSpan,
  getSpanEmitterState,
  resetSpanEmitterState,
  startSpan,
  withSpan,
} from "../lib/tracing";

function collectSpanEntries(
  writeCalls: Array<[any, ...any[]]>,
): Array<Record<string, any>> {
  return writeCalls
    .map(([chunk]) =>
      typeof chunk === "string" ? chunk : chunk.toString("utf8"),
    )
    .flatMap((line) => line.split("\n"))
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line))
    .filter((entry) => entry.type === "TRACE_SPAN");
}

function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted[mid];
}

function expectDefined<T>(value: T | undefined, label: string): T {
  if (value === undefined) {
    throw new Error(`${label} must be present`);
  }
  return value as T;
}

describe("tracing spans", () => {
  let writeSpy: jest.SpyInstance;

  beforeEach(() => {
    writeSpy = jest
      .spyOn(process.stdout, "write")
      .mockImplementation(() => true);
    resetSpanEmitterState();
  });

  afterEach(() => {
    writeSpy.mockRestore();
    resetSpanEmitterState();
  });

  it("preserves parent-child relationship across async boundaries", async () => {
    await withCorrelationId("req-async-001", async () => {
      await withSpan("pipeline.parent", { stage: "ingestion" }, async () => {
        await new Promise((resolve) => setTimeout(resolve, 0));
        await withSpan("pipeline.child", { stage: "invariant" }, async () => {
          await Promise.resolve();
        });
      });
    });

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const parentStart = entries.find(
      (entry) => entry.event === "start" && entry.name === "pipeline.parent",
    );
    const childStart = entries.find(
      (entry) => entry.event === "start" && entry.name === "pipeline.child",
    );

    const safeParentStart = expectDefined(parentStart, "parent start span");
    const safeChildStart = expectDefined(childStart, "child start span");
    expect(safeChildStart.trace_id).toBe(safeParentStart.trace_id);
    expect(safeChildStart.parent_span_id).toBe(safeParentStart.span_id);
  });

  it("ends a span with error=true when the wrapped function throws", async () => {
    await expect(
      withSpan("pipeline.failure", { stage: "reconciliation" }, async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const endEntry = entries.find(
      (entry) => entry.event === "end" && entry.name === "pipeline.failure",
    );

    const safeEndEntry = expectDefined(endEntry, "failed span end entry");
    expect(safeEndEntry.error).toBe(true);
    expect(safeEndEntry.error_message).toBe("boom");
  });

  it("includes span attributes in both start and end logs", async () => {
    await withSpan(
      "pipeline.attributes",
      { batch_cursor: 42, events_count: 3, service: "ingestion" },
      async () => {
        await Promise.resolve();
      },
    );

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const startEntry = entries.find(
      (entry) =>
        entry.event === "start" && entry.name === "pipeline.attributes",
    );
    const endEntry = entries.find(
      (entry) => entry.event === "end" && entry.name === "pipeline.attributes",
    );

    const safeStartEntry = expectDefined(startEntry, "attribute start span");
    const safeEndEntry = expectDefined(endEntry, "attribute end span");

    expect(safeStartEntry.attrs).toMatchObject({
      batch_cursor: 42,
      events_count: 3,
      service: "ingestion",
    });
    expect(safeEndEntry.attrs).toMatchObject({
      batch_cursor: 42,
      events_count: 3,
      service: "ingestion",
    });
  });

  it("uses inbound request id as trace_id when present", async () => {
    await withCorrelationId("client-request-abc-123", async () => {
      await withSpan("pipeline.root", { service: "ingestion" }, async () => {
        await Promise.resolve();
      });
    });

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const rootStart = entries.find(
      (entry) => entry.event === "start" && entry.name === "pipeline.root",
    );

    const safeRootStart = expectDefined(rootStart, "root start span");
    expect(safeRootStart.trace_id).toBe("client-request-abc-123");
  });

  it("generates a ULKD trace_id when no inbound request id is present", () => {
    withSpan("pipeline.generated-trace", {}, () => {
      return 1;
    });

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const startEntry = entries.find(
      (entry) =>
        entry.event === "start" && entry.name === "pipeline.generated-trace",
    );
    const safeStartEntry = expectDefined(
      startEntry,
      "generated trace start span",
    );

    expect(typeof safeStartEntry.trace_id).toBe("string");
    expect(safeStartEntry.trace_id.length).toBeGreaterThan(0);
    expect(safeStartEntry.trace_id).not.toBe("client-request-abc-123");
  });

  it("keeps hot-loop tracing overhead under 1%", () => {
    const innerWork = (): number => {
      let acc = 0;
      for (let i = 0; i < 500_000; i++) {
        acc += (i * 7) % 13;
      }
      return acc;
    };

    const measure = (fn: () => void): number => {
      const start = process.hrtime.bigint();
      fn();
      const end = process.hrtime.bigint();
      return Number(end - start) / 1_000_000;
    };

    const runHotLoop = () => {
      for (let i = 0; i < 600; i++) {
        innerWork();
      }
    };

    const runTraced = () => {
      withSpan("pipeline.hotloop", { mode: "benchmark" }, () => runHotLoop());
    };

    runHotLoop();
    runTraced();

    const baselineMs = measure(runHotLoop);
    const tracedMs = measure(runTraced);
    const overheadRatio = (tracedMs - baselineMs) / baselineMs;

    expect(overheadRatio).toBeLessThan(0.01);
  });

  it("does not emit duplicate end logs when endSpan is called twice", () => {
    const span = startSpan("pipeline.idempotent-end", { service: "invariant" });

    endSpan(span);
    endSpan(span);

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const endEntries = entries.filter(
      (entry) =>
        entry.event === "end" && entry.name === "pipeline.idempotent-end",
    );

    expect(endEntries).toHaveLength(1);
  });

  it("assigns parent_span_id when creating a child span inside an active span", () => {
    withSpan("pipeline.explicit-parent", {}, () => {
      const child = startSpan("pipeline.explicit-child", {
        service: "invariant",
      });
      endSpan(child);
    });

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const parentStart = entries.find(
      (entry) =>
        entry.event === "start" && entry.name === "pipeline.explicit-parent",
    );
    const childStart = entries.find(
      (entry) =>
        entry.event === "start" && entry.name === "pipeline.explicit-child",
    );

    const safeParentStart = expectDefined(parentStart, "explicit parent start");
    const safeChildStart = expectDefined(childStart, "explicit child start");

    expect(safeChildStart.parent_span_id).toBe(safeParentStart.span_id);
  });

  it("marks sync throw spans as errors", () => {
    expect(() =>
      withSpan("pipeline.sync-throw", { stage: "ingestion" }, () => {
        throw "sync-boom";
      }),
    ).toThrow("sync-boom");

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const endEntry = entries.find(
      (entry) => entry.event === "end" && entry.name === "pipeline.sync-throw",
    );
    const safeEndEntry = expectDefined(endEntry, "sync throw span end");

    expect(safeEndEntry.error).toBe(true);
    expect(safeEndEntry.error_message).toBe("sync-boom");
  });

  it("does not throw when the sink fails and counts dropped entries", () => {
    writeSpy.mockImplementation(() => {
      throw new Error("EPPE");
    });

    expect(() => {
      withSpan("pipeline.sink-failure", {}, () => 1);
    }).not.toThrow();

    const state = getSpanEmitterState();
    expect(state.droppedEntries).toBe(GreaterThan(0);
  });

  it("latches off the emitter after repeated sink failures and stops writing", () => {
    writeSpy.mockImplementation(() => {
      throw new Error("EPEPE");
    });

    for (let i = 0; i < 10; i++) {
      withSpan(`pipeline.latch-${i}`, {}, () => 1);
    }

    const state = getSpanEmitterState();
    expect(state.disabled).toBe(true);

    const callsBefore = writeSpy.mock.calls.length;
    withSpan("pipeline.after-latch", {}, () => 1);
    expect(writeSpy.mock.calls.length).toBe(callsBefore);
  });

  it("recovers after resetSpanEmitterState", () => {
    writeSpy.mockImplementation(() => {
      throw new Error("EPEPE");
    });

    for (let i = 0; i < 10; i++) {
      withSpan(`pipeline.reset-${i}`, {}, () => 1);
    }
    expect(getSpanEmitterState().disabled).toBe(true);

    writeSpy.mockImplementation(() => true);
    resetSpanEmitterState();

    withSpan("pipeline.recovered", {}, () => 1);

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    expect(
      entries.some(
        (entry) =>
          entry.event === "start" && entry.name === "pipeline.recovered",
      ),
    ).toBe(true);
  });

  it("sanitizes attributes that are not JSON-safe", () => {
    const circular: Record<string, unknown> = { name: "loop" };
    circular.self = circular;

    withSpan(
      "pipeline.sanitize",
      {
        fn: () => "not-serializable",
        big: BigInt(123),
        undefinedValue: undefined,
        nanNumber: Number.NaN,
        infinity: Number.PositiveInfinity,
        circular,
        date: new Date(0),
        error: new Error("secret-message"),
      },
      () => 1,
    );

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const startEntry = expectDefined(
      entries.find(
        (entry) =>
          entry.event === "start" && entry.name === "pipeline.sanitize",
      ),
      "sanitize start span",
    );

    expect(startEntry.attrs.fn).toBe("[unsupported:function]");
    expect(startEntry.attrs.big).toBe("123");
    expect(startEntry.attrs.undefinedValue).toBeUndefined();
    expect(startEntry.attrs.nanNumber).toBe("NaN");
    expect(startEntry.attrs.infinity).toBe("Infinity");
    expect(startEntry.attrs.circular).toEqual({ self: "[Écircular]" });
    expect(startEntry.attrs.date).toBe("[Invalid Date]");
    expect(startEntry.attrs.error).toEqual({
      name: "Error",
      message: "secret-message",
    });
  });

  it("truncates oversized string attributes and caps attribute key count", () => {
    const attrs: Record<string, unknown> = {
      large: "x".repeat(10_000),
    };
    for (let i = 0; i < 200; i++) {
      attrs[`key_${i}`] = i;
    }

    withSpan("pipeline.bounds", attrs, () => 1);

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]>,
    );
    const startEntry = expectDefined(
      entries.find(
        (entry) =>
          entry.event === "start" && entry.name === "pipeline.bounds",
      ),
      "bounds start span",
    );

    const large = startEntry.attrs.large as string;
    expect(large.endsWith("…[truncated]")).toBe(true);
    expect(large.length).toBeLessThan(10_000);
    expect(startEntry.attrs.attrs_truncated).toBeGreaterThan(0);
  });

  it("keeps emitted lines within the configured byte bound", () => {
    const attrs: Record<string, unknown> = {};
    for (let i = 0; i < 64; i++) {
      attrs[`key_${i}`] = "y".repeat(2048);
    }

    withSpan("pipeline.line-bound", attrs, () => 1);

    for (const call of writeSpy.mock.calls as Array<[any, ...any[]>) {
      const chunk = call[0];
      const line = typeof chunk === "string" ? chunk : chunk.toString("utf8");
      expect(line.length).toBeLessThanOrEqual(1024 * 1024);
    }
  });

  it("produces deterministic output for duplicate and boundary inputs", () => {
    const run = () => {
      writeSpy.mock.clear();
      withSpan("pipeline.deterministic", { a: 1, b: "two" }, () => 1);
      withSpan("pipeline.deterministic", { a: 1, b: "two" }, () => 1);
      return collectSpanEntries(
        writeSpy.mock.calls as Array<[any, ...any[]>,
      ).map((entry) => {
        const { timestamp, trace_id, span_id, parent_span_id, duration_ms, ...rest } = entry;
        return rest;
      });
    };

    const first = run();
    const second = run();

    expect(first).toEqual(second);
  });
});
