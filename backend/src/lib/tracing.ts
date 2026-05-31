import { AsyncLocalStorage } from "node:async_hooks";
import { ulid } from "ulid";
import { getCorrelationId } from "./requestContext";

export type SpanAttributes = Record<string, unknown>;

export interface Span {
  name: string;
  traceId: string;
  spanId: string;
  parentSpanId: string | null;
  attrs: SpanAttributes;
  startedAtMs: number;
  startedAtNs: bigint;
  ended: boolean;
}

interface SpanContext {
  traceId: string;
  spanId: string;
}

interface SpanLogEntry {
  level: "INFO";
  type: "TRACE_SPAN";
  event: "start" | "end";
  timestamp: string;
  name: string;
  trace_id: string;
  span_id: string;
  parent_span_id: string | null;
  duration_ms?: number;
  error?: boolean;
  error_message?: string;
  attrs: SpanAttributes;
}

const spanContextStorage = new AsyncLocalStorage<SpanContext>();

function isPromise<T>(value: T | Promise<T>): value is Promise<T> {
  return !!value && typeof (value as Promise<T>).then === "function";
}

function emitSpanLog(entry: SpanLogEntry): void {
  process.stdout.write(`${JSON.stringify(entry)}\n`);
}

function buildTraceId(parent?: SpanContext): string {
  if (parent?.traceId) {
    return parent.traceId;
  }

  const inboundRequestId = getCorrelationId();
  return inboundRequestId && inboundRequestId.length > 0
    ? inboundRequestId
    : ulid();
}

export function startSpan(name: string, attrs: SpanAttributes = {}): Span {
  const parent = spanContextStorage.getStore();

  const span: Span = {
    name,
    traceId: buildTraceId(parent),
    spanId: ulid(),
    parentSpanId: parent?.spanId ?? null,
    attrs,
    startedAtMs: Date.now(),
    startedAtNs: process.hrtime.bigint(),
    ended: false,
  };

  emitSpanLog({
    level: "INFO",
    type: "TRACE_SPAN",
    event: "start",
    timestamp: new Date(span.startedAtMs).toISOString(),
    name: span.name,
    trace_id: span.traceId,
    span_id: span.spanId,
    parent_span_id: span.parentSpanId,
    attrs: span.attrs,
  });

  return span;
}

export function endSpan(span: Span, err?: unknown): void {
  if (span.ended) {
    return;
  }

  span.ended = true;
  const endedAtNs = process.hrtime.bigint();
  const durationMs = Number(endedAtNs - span.startedAtNs) / 1_000_000;

  emitSpanLog({
    level: "INFO",
    type: "TRACE_SPAN",
    event: "end",
    timestamp: new Date().toISOString(),
    name: span.name,
    trace_id: span.traceId,
    span_id: span.spanId,
    parent_span_id: span.parentSpanId,
    duration_ms: durationMs,
    error: err !== undefined,
    error_message:
      err instanceof Error ? err.message : err ? String(err) : undefined,
    attrs: span.attrs,
  });
}

export function withSpan<T>(
  name: string,
  attrs: SpanAttributes,
  fn: () => Promise<T>,
): Promise<T>;
export function withSpan<T>(
  name: string,
  attrs: SpanAttributes,
  fn: () => T,
): T;
export function withSpan<T>(
  name: string,
  attrs: SpanAttributes,
  fn: () => T | Promise<T>,
): T | Promise<T> {
  const span = startSpan(name, attrs);

  return spanContextStorage.run(
    { traceId: span.traceId, spanId: span.spanId },
    () => {
      try {
        const result = fn();

        if (isPromise(result)) {
          return result
            .then((value) => {
              endSpan(span);
              return value;
            })
            .catch((err) => {
              endSpan(span, err);
              throw err;
            });
        }

        endSpan(span);
        return result;
      } catch (err) {
        endSpan(span, err);
        throw err;
      }
    },
  );
}
