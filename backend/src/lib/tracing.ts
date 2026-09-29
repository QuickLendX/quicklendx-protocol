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

/**
 * Maximum length of a trace identifier accepted from an inbound correlation id.
 * This bounds the amount of untrusted data that can be propagated into every span log.
 */
export const MAX_TRACE_ID_LENGTH = 256;

/**
 * Allowed characters for an inbound trace id: alphanumeric plus a conservative set of
 * separators commonly used by request id formats (e.g. UUIDs, W3 trace parents, request ids).
 * Whitespace, control characters, and log-injection sequences are rejected.
 */
const TRACE_ID_PATTERN = /^[A-Za-z0-9_.:-]{1,256}$/;

/**
 * Deterministically derive a trace id from the current span context or the inbound
 * correlation id, falling back to a fresh ULID.
 *
 * Invariants:
 * - A non-empty parent trace id is always reused verbatim so child spans share the
 *   trace of their parent.
 * - An inbound correlation id is only adopted when it is a non-empty string that
 *   matches the allowed trace-id pattern. Otherwise a fresh ULID is generated.
 * - The function never throws and never returns an empty string.
 */
export function buildTraceId(parent?: SpanContext): string {
  if (parent && typeof parent.traceId === "string" && parent.traceId.length > 0) {
    return parent.traceId;
  }

  const inboundRequestId = safeGetCorrelationId();
  if (isValidTraceId(inboundRequestId)) {
    return inboundRequestId as string;
  }

  return ulid();
}

function safeGetCorrelationId(): unknown {
  try {
    return getCorrelationId();
  } catch {
    return undefined;
  }
}

function isValidTraceId(value: unknown): value is string {
  if (typeof value !== "string") {
    return false;
  }
  if (value.length === 0 || value.length > MAX_TRACE_ID_LENGTH) {
    return false;
  }
  return TRACE_ID_PATTERN.test(value);
}

const spanContextStorage = new AsyncLocalStorage<SpanContext>();

function isPromise<T>(value: T | Promise<T>): value is Promise<T> {
  return !!value && typeof (value as Promise<T>).then === "function";
}

function emitSpanLog(entry: SpanLogEntry): void {
  try {
    process.stdout.write(`${JSON.stringify(entry)}\n`);
  } catch {
    // Tracing must never break the calling operation. If the log sink fails,
    // swallow the error so the business path continues deterministically.
  }
}

export function startSpan(name: string, attrs: SpanAttributes = {}): Span {
  const parent = spanContextStorage.getStore();

  const span: Span = {
    name,
    traceId: buildTraceId(parent),
    spanId: ulid(),
    parentSpanId: parent ? parent.spanId : null,
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
  try {
    if (!span || typeof span !== "object" || span.ended) {
      return;
    }

    span.ended = true;
    
    let durationMs: number | undefined;
    try {
      if (typeof span.startedAtNs === "bigint") {
        const endedAtNs = process.hrtime.bigint();
        durationMs = Number(endedAtNs - span.startedAtNs) / 1_000_000;
      }
    } catch {
      // Ignore duration calculation errors
    }

    emitSpanLog({
      level: "INFO",
      type: "TRACE_SPAN",
      event: "end",
      timestamp: new Date().toISOString(),
      name: span.name || "unknown",
      trace_id: span.traceId || "unknown",
      span_id: span.spanId || "unknown",
      parent_span_id: span.parentSpanId ?? null,
      duration_ms: durationMs,
      error: err !== undefined,
      error_message:
        err instanceof Error ? err.message : err ? String(err) : undefined,
      attrs: span.attrs || {},
    });
  } catch {
    // Tracing must never break the calling operation. If the span processing fails,
    // swallow the error so the business path continues deterministically.
  }
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
