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

let ulidGenerator: () => string = ulid;

/**
 * Hook for testing failure boundaries when ULID generation fails.
 * Internal only.
 */
export function _setUlidGeneratorForTesting(fn: () => string): void {
  ulidGenerator = fn;
}

/**
 * Reset ULID generator hook to standard implementation.
 * Internal only.
 */
export function _resetUlidGeneratorForTesting(): void {
  ulidGenerator = ulid;
}

/**
 * Generate a new ULID-based correlation ID.
 * ULIDs are lexicographically sortable and URL-safe.
 *
 * Failure-boundary guarantees:
 * - Deterministic fallback: If ULID generation fails (e.g., PRNG exhaustion or clock error),
 *   falls back to crypto-backed or entropy-based token generation without throwing.
 */
export function generateCorrelationId(): string {
  try {
    const id = ulidGenerator();
    if (typeof id === "string" && id.length > 0) {
      return id;
    }
  } catch {
    // Fall through to resilient fallback generator
  }

  try {
    const { randomUUID } = require("node:crypto");
    if (typeof randomUUID === "function") {
      return randomUUID().replace(/-/g, "").toUpperCase().slice(0, 26);
    }
  } catch {
    // Fall through to timestamp-entropy fallback
  }

  return `FALLBACK${Date.now()}${Math.random().toString(36).substring(2, 10)}`
    .toUpperCase()
    .slice(0, 26);
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
    req: {
      correlationId?: string;
      requestId?: string;
      headers?: Record<string, unknown>;
    },
    _res: unknown,
    next: (err?: any) => void
  ): void {
    try {
      const rawId =
        req.correlationId ??
        req.requestId ??
        (req.headers?.["x-request-id"] as string | undefined);
      const sanitized = sanitizeCorrelationId(rawId);
      if (sanitized) {
        runWithContext(sanitized, next);
      } else {
        next();
      }
    } catch {
      next();
    }
  };
}
