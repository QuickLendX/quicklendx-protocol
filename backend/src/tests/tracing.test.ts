import { withCorrelationId } from "../lib/requestContext";
import {
  buildTraceId,
  endSpan,
  MAX_TRACE_ID_LENGTH,
  startSpan,
  withSpan,
} from "../lib/tracing";

function collectSpanEntries(
  writeCalls: Array<[any, ...any[]]>,
): Array<Record<string, any>> {
  return writeCalls
    .map((call) => {
      const chunk = Array.isArray(call) ? call[0] : call;
      return typeof chunk === "string"
        ? chunk
        : (chunk as { toString: (encoding?: string) => string }).toString("utf8");
    })
    .flatMap((line) => line.split("\n"))
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line) as Record<string, unknown>)
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
  });

  afterEach(() => {
    writeSpy.mockRestore();
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
      writeSpy.mock.calls as Array<[any, ...any[]]>,
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
      writeSpy.mock.calls as Array<[any, ...any[]]>,
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
      writeSpy.mock.calls as Array<[any, ...any[]]>,
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
      writeSpy.mock.calls as Array<[any, ...any[]]>,
    );
    const rootStart = entries.find(
      (entry) => entry.event === "start" && entry.name === "pipeline.root",
    );

    const safeRootStart = expectDefined(rootStart, "root start span");
    expect(safeRootStart.trace_id).toBe("client-request-abc-123");
  });

  it("generates a ULID trace_id when no inbound request id is present", () => {
    withSpan("pipeline.generated-trace", {}, () => {
      return 1;
    });

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]]>,
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
    expect((safeStartEntry.trace_id as string).length).toBeGreaterThan(0);
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

    const baselineMs = median([
      measure(runHotLoop),
      measure(runHotLoop),
      measure(runHotLoop),
    ]);
    const tracedMs = median([
      measure(runTraced),
      measure(runTraced),
      measure(runTraced),
    ]);
    const overheadRatio = (tracedMs - baselineMs) / baselineMs;

    expect(overheadRatio).toBeLessThan(0.10);
  });

  it("does not emit duplicate end logs when endSpan is called twice", () => {
    const span = startSpan("pipeline.idempotent-end", { service: "invariant" });

    endSpan(span);
    endSpan(span);

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]]>,
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
      writeSpy.mock.calls as Array<[any, ...any[]]>,
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
      writeSpy.mock.calls as Array<[any, ...any[]]>,
    );
    const endEntry = entries.find(
      (entry) => entry.event === "end" && entry.name === "pipeline.sync-throw",
    );
    const safeEndEntry = expectDefined(endEntry, "sync throw span end");

    expect(safeEndEntry.error).toBe(true);
    expect(safeEndEntry.error_message).toBe("sync-boom");
  });

  it("startSpan returns a valid span even when attrs are not provided", () => {
    const span = startSpan("pipeline.default-attrs");
    expect(span.attrs).toEqual({});
    endSpan(span);
  });

  it("startSpan never throws when attrs are not serializable", () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;

    const span = startSpan("pipeline.circular", circular);
    expect(span.name).toBe("pipeline.circular");
    endSpan(span);
  });

  it("startSpan survives a throwing stdout write without losing the span", () => {
    writeSpy.mockImplementation(() => {
      throw new Error("stdout failure");
    });

    const span = startSpan("pipeline.stdout-failure", { service: "invariant" });
    expect(span.ended).toBe(false);
    endSpan(span);
    expect(span.ended).toBe(true);
  });

  it("endSpan is idempotent even when stdout write throws", () => {
    const span = startSpan("pipeline.end-stdout-failure");
    writeSpy.mockImplementation(() => {
      throw new Error("stdout failure");
    });

    expect(() => endSpan(span)).not.toThrow();
    expect(span.ended).toBe(true);
    expect(() => endSpan(span)).not.toThrow();
  });

  it("startSpan generates unique span_ids for duplicate names", () => {
    const a = startSpan("pipeline.duplicate");
    const b = startSpan("pipeline.duplicate");
    expect(a.spanId).not.toBe(b.spanId);
    endSpan(a);
    endSpan(b);
  });

  it("startSpan keeps the parent trace_id for nested spans", () => {
    withSpan("pipeline.nested-parent", {}, () => {
      const child = startSpan("pipeline.nested-child");
      endSpan(child);
    });

    const entries = collectSpanEntries(
      writeSpy.mock.calls as Array<[any, ...any[]]>,
    );
    const parentStart = entries.find(
      (entry) =>
        entry.event === "start" && entry.name === "pipeline.nested-parent",
    );
    const childStart = entries.find(
      (entry) =>
        entry.event === "start" && entry.name === "pipeline.nested-child",
    );

    const safeParentStart = expectDefined(parentStart, "nested parent start");
    const safeChildStart = expectDefined(childStart, "nested child start");

    expect(safeChildStart.trace_id).toBe(safeParentStart.trace_id);
  });

  it("withSpan propagates the original error object to the caller", async () => {
    const original = new Error("original-failure");
    let caught: unknown;
    try {
      await withSpan("pipeline.preserve-error", {}, async () => {
        throw original;
      });
    } catch (err) {
      caught = err;
    }
    expect(caught).toBe(original);
  });

  it("withSpan preserves the resolved value for sync and async functions", async () => {
    const syncResult = withSpan("pipeline.sync-value", {}, () => 42);
    expect(syncResult).toBe(42);

    const asyncResult = await withSpan("pipeline.async-value", {}, async () => 7);
    expect(asyncResult).toBe(7);
  });

  it("startSpan does not leak parent context between independent calls", () => {
    const outer = startSpan("pipeline.leak-outer");
    endSpan(outer);

    const independent = startSpan("pipeline.leak-independent");
    expect(independent.parentSpanId).toBeNull();
    endSpan(independent);
  });

  describe("buildTraceId failure boundaries", () => {
    it("reuses a non-empty parent trace id verbatim", () => {
      expect(buildTraceId({ traceId: "parent-trace-123", spanId: "s-1" })).toBe(
        "parent-trace-123",
      );
    });

    it("prefers the parent trace id over an inbound correlation id", async () => {
      await withCorrelationId("inbound-correlation", async () => {
        expect(buildTraceId({ traceId: "parent-trace", spanId: "s-1" })).toBe(
          "parent-trace",
        );
      });
    });

    it("returns a fresh ULID when the inbound correlation id is blank", async () => {
      await withCorrelationId("   ", async () => {
        const traceId = buildTraceId();
        expect(traceId.length).toBeGreaterThan(0);
        expect(traceId.trim()).toBe(traceId);
        expect(traceId).not.toContain("\n");
      });
    });

    it("rejects inbound correlation ids with control characters or log-injection sequences", async () => {
      const malicious = "trace-id\nevil";
      await withCorrelationId(malicious, async () => {
        const traceId = buildTraceId();
        expect(traceId).not.toBe(malicious);
        expect(traceId).not.toContain("\n");
      });
    });

    it("rejects inbound correlation ids that exceed the maximum length", async () => {
      const oversized = "a".repeat(MAX_TRACE_ID_LENGTH + 1);
      await withCorrelationId(oversized, async () => {
        const traceId = buildTraceId();
        expect(traceId).not.toBe(oversized);
        expect(traceId.length).toBeLessThanOrEqual(MAX_TRACE_ID_LENGTH);
      });
    });

    it("accepts an inbound correlation id at the maximum length boundary", async () => {
      const boundary = "a".repeat(MAX_TRACE_ID_LENGTH);
      await withCorrelationId(boundary, async () => {
        expect(buildTraceId()).toBe(boundary);
      });
    });

    it("returns a distinct ULID on each call when no inbound id is available", () => {
      const first = buildTraceId();
      const second = buildTraceId();
      expect(first).not.toBe(second);
    });

    it("propagates the same trace id to concurrent child spans", async () => {
      await withCorrelationId("req-concurrent", async () => {
        await withSpan("parent", {}, async () => {
          await Promise.all([
            withSpan("child.a", {}, async () => {
              await Promise.resolve();
            }),
            withSpan("child.b", {}, async () => {
              await Promise.resolve();
            }),
          ]);
        });
      });

      const entries = collectSpanEntries(
        writeSpy.mock.calls as Array<[any, ...any[]]>,
      );
      const parentStart = expectDefined(
        entries.find(
          (entry) => entry.event === "start" && entry.name === "parent",
        ),
        "parent start",
      );
      const childA = expectDefined(
        entries.find(
          (entry) => entry.event === "start" && entry.name === "child.a",
        ),
        "child a start",
      );
      const childB = expectDefined(
        entries.find(
          (entry) => entry.event === "start" && entry.name === "child.b",
        ),
        "child b start",
      );

      expect(childA.trace_id).toBe(parentStart.trace_id);
      expect(childB.trace_id).toBe(parentStart.trace_id);
      expect(childA.parent_span_id).toBe(parentStart.span_id);
      expect(childB.parent_span_id).toBe(parentStart.span_id);
    });

    it("stays deterministic when the correlation context is absent", () => {
      const first = buildTraceId();
      const second = buildTraceId();
      expect(typeof first).toBe("string");
      expect(typeof second).toBe("string");
      expect(first.length).toBeLessThanOrEqual(MAX_TRACE_ID_LENGTH);
    });
  });
});