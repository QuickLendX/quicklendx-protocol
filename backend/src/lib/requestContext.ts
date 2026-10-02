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
 *
 * Invariants:
 * - Sanitizes the correlation ID before storing. If invalid or empty, falls back to generating a valid ULID.
 * - Context is strictly bound to the execution scope of `fn` and automatically cleaned up upon completion or failure.
 *
 * Failure-boundary guarantees:
 * - Never stores an unusable id. A value that is not a string, or that fails
 *   `sanitizeCorrelationId` (blank, oversized, or carrying a newline, terminal
 *   escape, null byte or internal space), is discarded in favour of a freshly
 *   generated one. A tainted id therefore cannot reach the context, and from
 *   the context cannot reach any downstream log line.
 * - Never throws on the id path. `generateCorrelationId` cannot throw, so a
 *   caller with no usable id still gets a context, and `fn` always runs.
 * - Total under id-source failure. If the id source is unavailable *and* the
 *   inbound id is unusable, the run still succeeds with a `fb-`-prefixed
 *   degraded id, which is alertable from logs without embedding the error.
 * - Propagates `fn`'s outcome unchanged. A synchronous throw and a returned
 *   promise's rejection both reach the caller with their original identity, and
 *   the context is torn down on either path.
 * - The caller's own scope is never polluted: after `runWithContext` returns or
 *   throws, `getCorrelationId()` is `null` outside. Work scheduled inside `fn`
 *   that outlives it keeps the id, which is what audit writes and outbound RPC
 *   calls that complete after the response depend on.
 */
export function runWithContext<T>(correlationId: string, fn: () => T): T {
  const sanitized =
    (typeof correlationId === "string"
      ? sanitizeCorrelationId(correlationId)
      : null) ?? generateCorrelationId();
  return storage.run({ correlationId: sanitized }, fn);
}

/**
 * Get the correlation ID for the current async context.
 * Returns null if called outside a request context.
 *
 * Failure-boundary guarantees:
 * - Deterministic: always returns a non-empty string or null. Never throws.
 * - If the underlying store is missing, empty, or corrupted (e.g. a non-string
 *   value injected by a bug, unvalidated input, or a partially constructed context),
 *   this returns null rather than propagating a tainted value downstream.
 * - Concurrency: AsyncLocalStorage isolates stores per async chain, so a failure
 *   in one request cannot leak into another.
 */
export function getCorrelationId(): string | null {
  try {
    const store = storage.getStore();
    if (!store) return null;
    const id = store.correlationId;
    if (typeof id !== "string" || id.length === 0) return null;
    return sanitizeCorrelationId(id);
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
 *
 * Failure-boundary guarantees:
 * - Deterministic: Always returns a valid, non-empty, sanitized correlation ID string. Never throws.
 * - Context Fallback: When no context is active or when getCorrelationId() returns null,
 *   generates and returns a fresh ULID.
 * - Corruption Recovery: If the active context is corrupt, empty, non-string, or
 *   contains log-injectable characters, falls back to generating a valid ULID.
 * - Host / PRNG Failure Safety: In the event of storage retrieval failure or ULID generator
 *   failure, safely falls back to a deterministic entropy-backed identifier.
 * - Idempotency: Repeated calls within the same valid context return the identical correlation ID.
 * - Concurrency & Isolation: Multiple concurrent operations maintain isolated contexts or
 *   generate distinct IDs without cross-talk or race conditions.
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
 * Test-only stand-in for the correlation-id source, consulted by
 * `generateCorrelationId` in place of the live `ulid` export.
 *
 * Invariants:
 * - `null` is the default and means "no override", so production behaviour and
 *   the healthy code path are untouched.
 * - The override is resolved *per call* rather than captured at module load, so
 *   it is actually consulted. (It previously was not: `generateCorrelationId`
 *   dereferenced `ulid` directly, leaving this hook write-only — every test
 *   that set it exercised the real generator and asserted nothing.)
 * - The override cannot weaken validation. A value it returns is still checked
 *   against `ULID_PATTERN` and still falls through to the degraded path, so a
 *   test double can drive the failure boundary but never launder a malformed
 *   or tainted id into the context.
 * - This is the only portable seam for the failure boundary: the package is
 *   ESM, where a test runner cannot spy on a module namespace object, so
 *   monkey-patching the `ulid` export is not available.
 */
let ulidOverride: (() => string) | null = null;

/**
 * Hook for testing failure boundaries when ULID generation fails.
 * Internal only.
 */
export function _setUlidGeneratorForTesting(fn: () => string): void {
  ulidOverride = fn;
}

/**
 * Reset ULID generator hook to standard implementation.
 * Internal only.
 */
export function _resetUlidGeneratorForTesting(): void {
  ulidOverride = null;
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
  // `ulid` is dereferenced per call rather than captured at module load, so
  // module-level mocking still reaches the healthy path. `ulidOverride` is the
  // supported seam for driving the failure boundary deterministically.
  const source = ulidOverride ?? ulid;
  try {
    const candidate = source();
    if (typeof candidate === "string" && ULID_PATTERN.test(candidate)) {
      return candidate;
    }
  } catch {
    // A failure in the id source must never surface on the request path.
  }
  return generateDegradedCorrelationId();
}

export const MAX_CORRELATION_ID_LENGTH = 128;

/**
 * Sanitize a client-supplied correlation ID to prevent log injection.
 *
 * Leading/trailing whitespace is trimmed, then the value must consist solely
 * of alphanumerics, hyphens, underscores, dots, and colons and be 1–128 characters long.
 * Any other character (newlines, carriage returns, tabs, ANSI escapes, null
 * bytes, internal spaces, …) causes the value to be rejected. Returns null
 * when validation fails.
 */
export function sanitizeCorrelationId(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  if (trimmed.length === 0 || trimmed.length > MAX_CORRELATION_ID_LENGTH) return null;
  if (!/^[A-Za-z0-9_.:-]+$/.test(trimmed)) return null;
  return trimmed;
}

/**
 * Resolve a candidate correlation id down to a value that is safe to place in
 * the context, or null when the candidate must not be used.
 *
 * Only a string can qualify, and it must additionally survive
 * `sanitizeCorrelationId`. Rejecting here — rather than at the point of use —
 * is what keeps an empty, oversized or unparseable value from masking the
 * fallback field, and keeps a value carrying newlines, terminal escapes or a
 * null byte out of every log line that reads the context.
 */
function resolveContextId(candidate: unknown): string | null {
  if (typeof candidate !== "string") return null;
  return sanitizeCorrelationId(candidate);
}

/**
 * Express middleware that establishes the async-local-storage request context
 * from an already-resolved correlation/request id on the request object.
 *
 * It takes the first *usable* of `req.correlationId`, `req.requestId` and the
 * `x-request-id` request header, in that order. "Usable" means a string that
 * passes `sanitizeCorrelationId`, so a blank, oversized or unparseable
 * correlationId no longer masks a valid requestId or header. When no source
 * yields a usable id the request proceeds without a context (downstream callers
 * fall back to generating their own id). All downstream async work — audit
 * writes, outbound RPC calls, event processing — can read the id via
 * getCorrelationId().
 *
 * Invariants:
 * - The context is only established for the duration of `next()`, so it cannot
 *   leak into subsequent requests on the same event-loop tick.
 * - A missing or empty id never creates a context with an undefined value, and a
 *   value that would be unsafe to log never creates a context at all.
 * - `next()` runs exactly once on every path, including the rejected ones: an
 *   unusable id degrades observability, it must never stall or drop a request.
 * - A missing or non-object `req` is treated as "no id" instead of throwing, so
 *   a malformed request cannot take the chain down before `next()` is reached.
 * - `next()` errors are propagated to the caller unchanged; the context is still
 *   torn down correctly by AsyncLocalStorage.
 */
export function createRequestContextMiddleware() {
  return function requestContextMiddleware(
    req: {
      correlationId?: unknown;
      requestId?: unknown;
      headers?: Record<string, unknown>;
    } | null | undefined,
    _res: unknown,
    next: (err?: any) => void
  ): void {
    // Every candidate source is resolved through resolveContextId rather than
    // `??`, because `??` short-circuits on a present-but-unusable value: a blank,
    // oversized or unparseable correlationId would otherwise mask a perfectly
    // usable requestId or x-request-id header. Resolving each source in turn
    // means the first *usable* id wins and an unusable one falls through.
    //
    // The try/catch wraps only the resolution, never the next() call, so a
    // throwing property access on a malformed `req` degrades to "no context"
    // while a genuine downstream failure still propagates to the caller instead
    // of being swallowed and the request silently stalling.
    let id: string | null = null;
    try {
      id =
        resolveContextId(req?.correlationId) ??
        resolveContextId(req?.requestId) ??
        resolveContextId(req?.headers?.["x-request-id"]);
    } catch {
      id = null;
    }
    if (id === null) {
      next();
      return;
    }
    runWithContext(id, next);
  };
}
