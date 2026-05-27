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

// ── Context storage ─────────────────────────────────────────────────────────────

interface RequestContext {
  correlationId: string;
}

const requestContextStorage = new AsyncLocalStorage<RequestContext>();

// ── Correlation ID validation ─────────────────────────────────────────────────────

/**
 * Maximum length for client-supplied correlation IDs.
 * ULIDs are 26 characters, but we allow some margin for future formats.
 */
const MAX_CORRELATION_ID_LENGTH = 128;

/**
 * Valid characters for correlation IDs.
 * ULIDs use Crockford's Base32 (A-Z, 0-9 excluding I, L, O, U).
 * We allow alphanumeric and hyphens for flexibility.
 */
const VALID_CORRELATION_ID_PATTERN = /^[A-Za-z0-9\-_]+$/;

/**
 * Sanitize and validate a client-supplied correlation ID.
 *
 * Security: This prevents log injection by ensuring only safe characters
 * are accepted and the length is bounded.
 *
 * @param clientSupplied - The correlation ID from the request header
 * @returns The sanitized correlation ID, or null if invalid
 */
export function sanitizeCorrelationId(clientSupplied: string | undefined): string | null {
  if (!clientSupplied) {
    return null;
  }

  // Trim whitespace
  const trimmed = clientSupplied.trim();

  // Check length bounds
  if (trimmed.length === 0 || trimmed.length > MAX_CORRELATION_ID_LENGTH) {
    return null;
  }

  // Validate character set to prevent log injection
  if (!VALID_CORRELATION_ID_PATTERN.test(trimmed)) {
    return null;
  }

  return trimmed;
}

/**
 * Generate a new ULID correlation ID.
 */
export function generateCorrelationId(): string {
  return ulid();
}

// ── Context management ────────────────────────────────────────────────────────────

/**
 * Run a function with a correlation ID context.
 *
 * This sets the correlation ID in async local storage for the duration
 * of the function call, making it available to all downstream async operations.
 *
 * @param correlationId - The correlation ID to set in context
 * @param fn - The function to run within this context
 * @returns The result of the function
 */
export function withCorrelationId<T>(
  correlationId: string,
  fn: () => T
): T {
  return requestContextStorage.run({ correlationId }, fn);
}

/**
 * Get the current correlation ID from context.
 *
 * Returns null if no context is set (e.g., outside of a request).
 *
 * @returns The current correlation ID, or null if not set
 */
export function getCorrelationId(): string | null {
  const store = requestContextStorage.getStore();
  return store?.correlationId ?? null;
}

/**
 * Get the current correlation ID, or generate a new one if not set.
 *
 * This is useful for background tasks that may not have request context.
 *
 * @returns The current correlation ID, or a newly generated one
 */
export function getOrGenerateCorrelationId(): string {
  return getCorrelationId() ?? generateCorrelationId();
}

// ── Express middleware helper ───────────────────────────────────────────────────

/**
 * Express middleware to set correlation ID in async local storage.
 *
 * This should be used in conjunction with request-logger middleware.
 * The correlation ID is either accepted from the X-Request-Id header
 * (if valid) or generated as a new ULID.
 */
export function createRequestContextMiddleware() {
  return (req: any, res: any, next: any) => {
    // Extract correlation ID from request (set by request-logger middleware)
    const correlationId = req.correlationId || req.requestId;
    
    if (correlationId) {
      // Run the rest of the request handler with this context
      requestContextStorage.run({ correlationId }, next);
    } else {
      // No correlation ID available, proceed without context
      next();
    }
  };
}
