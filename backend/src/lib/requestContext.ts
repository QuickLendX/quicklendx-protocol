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
 * Returns undefined if called outside a request context.
 */
export function getCorrelationId(): string | undefined {
  return storage.getStore()?.correlationId;
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
 * Accepts only alphanumeric characters and hyphens, max 128 chars.
 * Returns null if the value fails validation.
 */
export function sanitizeCorrelationId(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const trimmed = raw.trim();
  const sanitized = trimmed.replace(/[^a-zA-Z0-9\-_]/g, "");
  if (sanitized.length === 0 || sanitized.length > 128) return null;
  if (sanitized !== trimmed) return null;
  return sanitized;
}

/**
 * Get the existing correlation ID from the current context, or generate a new
 * one if no context is active. Useful when you need a correlation ID but don't
 * want to force the caller to provide one.
 */
export function getOrGenerateCorrelationId(): string {
  const existing = getCorrelationId();
  return existing ?? generateCorrelationId();
}

/**
 * Express middleware that wraps the request handler in a correlation ID context.
 * Uses an existing correlation ID from the request (header or generated), falling
 * back to the request ID if neither is available.
 */
export function createRequestContextMiddleware() {
  return function requestContextMiddleware(
    req: any,
    _res: any,
    next: () => void,
  ): void {
    const id = req.correlationId ?? req.requestId ?? generateCorrelationId();
    runWithContext(id, next);
  };
}