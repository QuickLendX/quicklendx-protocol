/**
 * Request Context - Async Local Storage for Correlation IDs
 *
 * This module provides a thread-safe way to propagate correlation IDs
 * across async operations using Node.js AsyncLocalStorage. This ensures that
 * correlation IDs are automatically available in all downstream
 * logging without manual threading.
 *
 * Security guarantees:
 * - Client-supplied correlation IDs are sanitized before use
 * - Log injection is prevented by strict validation
 * - Context isolation prevents bleeding between concurrent requests
 */
import { AsyncLocalStorage } from "node:async_hooks";
import { randomUUID } from "node:crypto";
import { ulid } from "ulid";

interface RequestContext {
  correlationId: string;
}

const storage = new AsyncLocalStorage<RequestContext>();

/**
 * Run a callback within a new request context.
 * The correlationId is available to all async code called within
 * the callback without needing to thread it through every function.
 */
export function runWithContext<T>(correlationId: string, fn: () => T): T {
  return storage.run({ correlationId }, fn);
}

/**
 * Get the correlation ID for the current async context.
 * Returns null if called outside a request context.
 *
 * Failure-boundary guarantees:
 * - Deterministic: always returns a non-empty string or null. Never throws.
 * - If the underlying store is missing, empty, or corrupted (e.g. a non-string
 *   value injected by a bug or a partially constructed context), this returns
 *   null rather than propagating a tainted value downstream.
 * - Concurrency: AsyncLocalStorage isolates stores per async chain, so a failure
 *   in one request cannot leak into another.
 */
export function getCorrelationId(): string | null {
  try {
    const store = storage.getStore();
    if (!store) return null;
    const id = store.correlationId;
    if (typeof id !== "string" || id.length === 0) return null;
    return id;
  } catch {
    // AsyncLocalStorage.getStore() is synchronous and non-throwing in normal
    // operation, but defensively guard against host environment failures so a
    // corrupted context never takes down a request path.
    return null;
  }
}

/**
 * Return the correlation ID for the current async context, or generate a new
 * ULID when no context is active. Useful for code paths (background workers,
 * scheduled jobs) that may run with or without an inbound request.
 */
export function getOrGenerateCorrelationId(): string {
  return getCorrelationId() ?? generateCorrelationId();
}

/**
 * Alias for runWithContext — kept for backwards compatibility
 * with any code that imported withCorrelationId.
 */
export function withCorrelationId<T>(correlationId: string, fn: () => T): T {
  return runWithContext(correlationId, fn);
}

/**
 * Exact shape of a ULID: 26 Crockford base32 characters, excluding I, L, O and
 * U. A healthy `ulid` source always produces a value matching this pattern, so
 * anything else is treated as a corrupted source.
 *
 * Note that this contract is a strict subset of what `sanitizeCorrelationId`
 * accepts (alphanumerics, hyphens and underscores, 1–128 characters), which is
 * what guarantees the "every generated id is log-safe" invariant below.
 */
const ULID_PATTERN = /^[0-9A-HJKMNP-TV-Z]{26}$/;

/**
 * Marker prefix for degraded-mode ids, i.e. ids produced when the ULID source
 * is unavailable or returned an invalid value.
 *
 * This is the observability hook for the failure boundary: operators can alert
 * on `fb-` prefixed request ids in logs to detect a failing id source. The
 * underlying error is deliberately *not* embedded in the id, so no untrusted or
 * sensitive data can leak into logs through the correlation id.
 */
const DEGRADED_ID_PREFIX = "fb";

/**
 * Monotonic per-process counter that keeps degraded-mode ids unique even when
 * two ids are generated within the same millisecond, when the clock is frozen,
 * or when `Date.now()` is mocked/removed by a test or a host patch.
 */
let degradedSequence = 0;

/**
 * Best-effort entropy suffix for degraded-mode ids. Never throws.
 *
 * This value only exists to avoid collisions between separate processes; it is
 * never used for an authorization or security decision, so the `Math.random`
 * fallback (used when the crypto source itself is unavailable) is acceptable.
 */
function degradedEntropy(): string {
  try {
    return randomUUID().replace(/-/g, "");
  } catch {
    return Math.random().toString(36).slice(2).padEnd(8, "0");
  }
}

/** Advance and return the degraded-mode sequence, never throwing. */
function nextDegradedSequence(): number {
  degradedSequence =
    degradedSequence >= Number.MAX_SAFE_INTEGER ? 1 : degradedSequence + 1;
  return degradedSequence;
}

/**
 * Build the fallback correlation id used when the ULID source fails.
 *
 * Invariants:
 * - Never throws.
 * - Always accepted by `sanitizeCorrelationId` (lowercase alphanumerics and
 *   hyphens, well under the 128 character ceiling), so it can be used anywhere
 *   a ULID can — response headers, outbound RPC headers, audit entries.
 * - Unique per process: the monotonic sequence guarantees uniqueness even if
 *   `Date.now()` is frozen, and the entropy suffix separates processes.
 */
function generateDegradedCorrelationId(): string {
  const sequence = nextDegradedSequence().toString(36);
  try {
    return `${DEGRADED_ID_PREFIX}-${Date.now().toString(36)}-${sequence}-${degradedEntropy()}`;
  } catch {
    // `Date.now` is missing or throwing; the monotonic sequence on its own is
    // still a unique, log-safe identifier.
    return `${DEGRADED_ID_PREFIX}-${sequence}`;
  }
}

/**
 * Generate a new ULID-based correlation ID.
 * ULIDs are lexicographically sortable and URL-safe.
 *
 * Failure-boundary guarantees:
 * - Never throws: callers on the request path always receive an id, so a broken
 *   id source can never turn into a 5xx or an aborted request.
 * - Deterministic: the result is always a non-empty string that
 *   `sanitizeCorrelationId` accepts — a canonical ULID when the source is
 *   healthy, an `fb-…` degraded id otherwise. It never returns a partial,
 *   empty, or unvalidated value.
 * - Taint resistance: the source output is validated against the ULID shape
 *   before use, so a corrupted or patched source cannot smuggle newlines,
 *   control characters, or other log-forging content into logs.
 * - Duplicates: every call returns a distinct id, including repeated failures
 *   and concurrent calls.
 * - Diagnosability: degraded ids carry the `fb-` prefix, so a failing id source
 *   is visible and alertable from logs without exposing the underlying error.
 */
export function generateCorrelationId(): string {
  try {
    const candidate = ulid();
    if (typeof candidate === "string" && ULID_PATTERN.test(candidate)) {
      return candidate;
    }
  } catch {
    // A failure in the id source must never surface on the request path.
  }
  return generateDegradedCorrelationId();
}

/**
 * Sanitize a client-supplied correlation ID to prevent log injection.
 *
 * Leading/trailing whitespace is trimmed, then the value must consist solely
 * of alphanumerics, hyphens, and underscrores and be 1–128 characters long.
 * Any other character (newlines, carriage returns, tabs, ANSI escapes, null
 * bytes, internal spaces, …) causes the value to be rejected. Returns null
 * when validation fails.
 */
export function sanitizeCorrelationId(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  if (trimmed.length === 0 || trimmed.length > 128) return null;
  if (!/^[A-Za-z0-9_-]+$/.test(trimmed)) return null;
  return trimmed;
}

/**
 * Express middleware that establishes the async-local-storage request context
 * from an already-resolved correlation/request id on the request object.
 *
 * It prefers `req.correlationId`, falling back to `req.requestId`. When neither
 * is present the request proceeds without a context (downstream callers fall
 * back to generating their own id). All downstream async work — audit writes,
 * outbound RPC calls, event processing — can read the id via getCorrelationId().
 *
 * Invariants:
 * - The context is only established for the duration of `next()`, so it cannot
 *   leak into subsequent requests on the same event-loop tick.
 * - A missing or empty id never creates a context with an undefined value.
 * - `next()` errors are propagated to the caller unchanged; the context is still
 *   torn down correctly by AsyncLocalStorage.
 */
export function createRequestContextMiddleware() {
  return function requestContextMiddleware(
    req: { correlationId?: string; requestId?: string },
    _res: unknown,
    next: () => void
  ): void {
    const id = req.correlationId ?? req.requestId;
    if (typeof id === "string" && id.length > 0) {
      runWithContext(id, next);
    } else {
      next();
    }
  };
}
