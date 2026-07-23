/**
 * Request Context - Async Local Storage for Correlation IDs
 *
 * This module provides a thread-safe way to propagate correlation IDs
 * across async operations using Node.js AsyncLocalStorage. This ensures
 * that correlation IDs are automatically available in all downstream
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
 */
export function runWithContext<T>(correlationId: string, fn: () => T): T {
  return storage.run({ correlationId }, fn);
}

/**
 * Get the correlation ID for the current async context.
 * Returns null if called outside a request context.
 */
export function getCorrelationId(): string | null {
  return storage.getStore()?.correlationId ?? null;
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
 * Generate a new ULID-based correlation ID.
 * ULIDs are lexicographically sortable and URL-safe.
 */
export function generateCorrelationId(): string {
  return ulid();
}

/**
 * Sanitize a client-supplied correlation ID to prevent log injection.
 *
 * Leading/trailing whitespace is trimmed, then the value must consist solely
 * of alphanumerics, hyphens, and underscores and be 1–128 characters long.
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
 */
export function createRequestContextMiddleware() {
  return function requestContextMiddleware(
    req: { correlationId?: string; requestId?: string },
    _res: unknown,
    next: () => void
  ): void {
    const id = req.correlationId ?? req.requestId;
    if (id) {
      runWithContext(id, next);
    } else {
      next();
    }
  };
}