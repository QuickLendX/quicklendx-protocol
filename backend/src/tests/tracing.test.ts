import { withCorrelationId } from "../lib/requestContext";
import { endSpan, startSpan, withSpan } from "../lib/tracing";

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
      writeSpy.mock.calls as Array<[any, ...any[]]>,
    );
    const endEntries = entries.filter(
      (entry) =>
        entry.event === "end" && entry.name === "pipeline.idempotent-end",
    );

    expect(endEntries).toHaveLength(1);
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
});
