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
export const MAX_TRACE_ID_LENGTH = 128;

/**
 * Allowed characters for an inbound trace id: alphanumeric plus a conservative set of
 * separators commonly used by request id formats (e.g. UUIDs, W3 trace parents, request ids).
 * Whitespace, control characters, and log-injection sequences are rejected.
 */
const TRACE_ID_PATTERN = /^[A-Za-z0-9_.:-]{1,128}$/;

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

/**
 * Maximum length of a single emitted span log line. This bounds the
 * working memory used by the spank emitter and keeps the line atomic from
 * the perspective of line-oriented log consumers. It is deliberately
 * generous enough to preserve normal span payloads while preventing an
 * adversarial or accidentally large attribute bag from causing an unbounded
 * write.
 */
const MAX_SPAN_LOG_BYTES = 1024 * 1024;

/**
 * Maximum length of a string value within a span attribute. Values longer
 * than this are truncated with an explicit marker so consumers can tell
 * the data was clipped rather than corrupted.
 */
const MAX_ATTR_STRING_LENGTH = 2048;

/**
 * Maximum number of attribute keys preserved on a span. Excess keys are
 * dropped and reported via a deterministic counter attribute.
 */
const MAX_ATTR_KEYS = 64;

/**
 * Maximum depth of attribute values that are walked when sanitizing.
 * Deeper structures are replaced with a marker to avoid infinite recursion on
 * cyclic or pathologically deep objects.
 */
const MAX_ATTR_DEPTH = 6;

const TRUNCATION_MARKER = "…[truncated]";

const CIRCULAR_MARKER = "[Écircular]";

const DEPTH_MARKER = "[Édepth limit]";

/**
 * State of the spank log emitter. This is the only mutable state in this
 * module and it is used to enforce deterministic failure-boundary behavior:
 *
 * - `disabled` is latched when the underlying sink fails repeatedly. Once
 *   latched, further emits are skipped with out attempting to write, so a
 *   broken sink cannot cause a tight loop of failed writes.
 * - `consecutiveFailures` counts failures since the last successful write.
 * - `droppedEntries` counts entries that were not written because of a
 *   failure or because the emitter was latched off. It is exposed through
 *   `getSpanEmitterState` for observability and testing.
 */
interface SpanEmitterState {
  disabled: boolean;
  consecutiveFailures: number;
  droppedEntries: number;
}

const MAX_CONSECUTIVE_FAILURES = 5;

const emitterState: SpanEmitterState = {
  disabled: false,
  consecutiveFailures: 0,
  droppedEntries: 0,
};

/**
 * Returns a snapshot of the emitter state. This is useful for tests and
 * operational telemetry that needs to detect lost span logs.
 */
export function getSpanEmitterState(): Readonly<SpanEmitterState> {
  return { ...emitterState };
}

/**
 * Resets the emitter state. This is intended for test isolation and for
 * operational recovery hooks (e.g. after a rotation of the log sink).
 */
export function resetSpanEmitterState(): void {
  emitterState.disabled = false;
  emitterState.consecutiveFailures = 0;
  emitterState.droppedEntries = 0;
}

function isPromise<T>(value: T | Promise<T>): value is Promise<T> {
  return !!value && typeof (value as Promise<T>).then === "function";
}

/**
 * Deterministically sanitizes a value so that it can be JSON-serialized
 * without throwing and without exposing functions, symbols, or circular
 * references. This is the boundary that keeps the emitter from failing on
 * attribute values that are not JSON-safe.
 */
function sanitizeAttrValue(
  value: unknown,
  depth: number,
  seen: WeakSet<object>,
): unknown {
  if (value === null) {
    return null;
  }

  const type = typeof value;

  if (type === "string") {
    const str = value as string;
    return str.length > MAX_ATTR_STRING_LENGTH
      ? `${str.slice(0, MAX_ATTR_STRING_LENGTH)}${TRUNCATION_MARKER}`
      : str;
  }

  if (type === "number") {
    const num = value as number;
    return Number.isFinite(num) ? num : String(num);
  }

  if (type === "boolean") {
    return value;
  }

  if (type === "bigint") {
    return (value as bigint).toString();
  }

  if (type === "undefined") {
    return undefined;
  }

  if (type === "function" || type === "symbol") {
    return `[unsupported:${type}]`;
  }

  if (depth >= MAX_ATTR_DEPTH) {
    return DEPTH_MARKER;
  }

  if (type === "object") {
    const obj = value as object;
    if (seen.has(obj)) {
      return CIRCULAR_MARKER;
    }
    seen.add(obj);

    try {
      if (Array.isArray(obj)) {
        return obj.map((item) => sanitizeAttrValue(item, depth + 1, seen));
      }

      if (obj instanceof Date) {
        const time = obj.getTime();
        return Number.isFinite(time) ? obj.toISOString() : "[Invalid Date]";
      }

      if (obj instanceof Error) {
        return {
          name: obj.name,
          message: obj.message,
        };
      }

      const out: Record<string, unknown> = {};
      for (const [key, entryValue] of Object.entries(obj)) {
        out[key] = sanitizeAttrValue(entryValue, depth + 1, seen);
      }
      return out;
    } finally {
      seen.delete(obj);
    }
  }

  return String(value);
}

/**
 * Sanitizes the attribute bag of a span. The result is guaranteed to be
 * JSON-serializable and bounded in size. Keys are preserved in insertion order
 * up to `MAX_ATTR_KEYS`; any excess is reported via a counter attribute.
 */
function sanitizeAttrs(attrs: SpanAttributes): SpanAttributes {
  if (!attrs || typeof attrs !== "object") {
    return {};
  }

  const entries = Object.entries(attrs);
  if (entries.length === 0) {
    return {};
  }

  const seen = new WeakSet<object>();
  const out: SpanAttributes = {};
  const limit = Math.min(entries.length, MAX_ATTR_KEYS);

  for (let i = 0; i < limit; i++) {
    const [key, rawValue] = entries[i];
    const sanitized = sanitizeAttrValue(rawValue, 0, seen);
    if (sanitized !== undefined) {
      out[key] = sanitized;
    }
  }

  if (entries.length > MAX_ATTR_KEYS) {
    out["attrs_truncated"] = entries.length - MAX_ATTR_KEYS;
  }

  return out;
}

/**
 * Safely serializes a span log entry. The entry is always serializable
 * because attributes are sanitized before being handed to the emitter, but
 * this function still guards against unexpected serialization failures so
 * the emitter can never throw from a JSON error.
 */
function safeStringify(entry: SpanLogEntry): string {
  try {
    return JSON.stringify(entry);
  } catch {
    return JSON.stringify({
      level: entry.level,
      type: entry.type,
      event: entry.event,
      timestamp: entry.timestamp,
      name: entry.name,
      trace_id: entry.trace_id,
      span_id: entry.span_id,
      parent_span_id: entry.parent_span_id,
      attrs: {},
      serialization_error: true,
    });
  }
}

/**
 * Emits a span log entry to stdout. This function is the failure boundary
 * for tracing: it must never throw and must never allow a misbehaving
 * sink to corrupt the calling operation. It enforces the following
 * invariants:
 *
 * 1. A failure to write is counted and never propagated.
 * 2. After `MAX_CONSECUTIVE_FAILURES` consecutive failures, the emitter is
 *    latched off to avoid hot-looping on a broken sink.
 * 3. Every dropped entry is accounted for in `droppedEntries`.
 * 4. A log line is always a single JSON object followed by a newline, and
 *    it is bounded by `MAX_SPAN_LOG_BYTES`.
 */
function emitSpanLog(entry: SpanLogEntry): void {
  if (emitterState.disabled) {
    emitterState.droppedEntries += 1;
    return;
  }

  let line: string;
  try {
    line = `${safeStringify(entry)}\n`;
  } catch {
    emitterState.consecutiveFailures += 1;
    emitterState.droppedEntries += 1;
    if (emitterState.consecutiveFailures >= MAX_CONSECUTIVE_FAILURES) {
      emitterState.disabled = true;
    }
    return;
  }

  if (line.length > MAX_SPAN_LOG_BYTES) {
    // The entry is bounded by the sanitization steps above, but a defensive
    // truncation keeps the write atomic even if a future change adds a
    // large field. The truncated line is not valid JSON, so we drop it and
    // account for the loss instead of emitting a corrupt record.
    emitterState.droppedEntries += 1;
    return;
  }

  try {
    process.stdout.write(line);
    emitterState.consecutiveFailures = 0;
  } catch {
    emitterState.consecutiveFailures += 1;
    emitterState.droppedEntries += 1;
    if (emitterState.consecutiveFailures >= MAX_CONSECUTIVE_FAILURES) {
      emitterState.disabled = true;
    }
  }
}

export function startSpan(name: string, attrs: SpanAttributes = {}): Span {
  const parent = spanContextStorage.getStore();

  const sanitizedAttrs = sanitizeAttrs(attrs);

  const span: Span = {
    name,
    traceId: buildTraceId(parent),
    spanId: ulid(),
    parentSpanId: parent ? parent.spanId : null,
    attrs: sanitizedAttrs,
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