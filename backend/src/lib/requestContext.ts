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

export interface RequestContext {
  correlationId: string;
  actor?: string;
}

export interface RequestContextInput {
  correlationId?: string;
  requestId?: string;
  actor?: string | null;
}

const storage = new AsyncLocalStorage<RequestContext>();

function normalizeActor(actor: unknown): string | undefined {
  if (typeof actor !== "string") return undefined;
  const trimmed = actor.trim();
  if (trimmed.length === 0) return undefined;
  if (/[\r\n\t\0]/.test(trimmed)) return undefined;
  return trimmed.slice(0, 128);
}

function normalizeContext(context: string | RequestContextInput): RequestContext {
  if (typeof context === "string") {
    return { correlationId: context };
  }

  const correlationId = context.correlationId ?? context.requestId;
  if (!correlationId) {
    throw new Error("Request context requires a correlationId or requestId");
  }

  const actor = normalizeActor(context.actor);
  return actor ? { correlationId, actor } : { correlationId };
}

/**
 * Run a callback within a new request context.
 * The correlationId is available to all async code called within
 * the callback without needing to thread it through every function.
 */
export function runWithContext<T>(context: string | RequestContextInput, fn: () => T): T {
  return storage.run(normalizeContext(context), fn);
}

/**
 * Get the correlation ID for the current async context.
 * Returns null if called outside a request context.
 */
export function getCorrelationId(): string | null {
  return storage.getStore()?.correlationId ?? null;
}

/**
 * Request-id alias for call sites whose audit/event schema uses request_id
 * rather than correlation_id.
 */
export function getRequestId(): string | null {
  return getCorrelationId();
}

/**
 * Get the actor for the current async context.
 * Returns null outside a request context or before authentication is resolved.
 */
export function getRequestActor(): string | null {
  return storage.getStore()?.actor ?? null;
}

/**
 * Attach or replace the actor on the current async context after authentication.
 */
export function setRequestActor(actor: string): void {
  const store = storage.getStore();
  const normalized = normalizeActor(actor);
  if (!store || !normalized) return;
  store.actor = normalized;
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
 * Run a callback within a request context carrying request_id and actor metadata.
 */
export function withRequestContext<T>(context: RequestContextInput, fn: () => T): T {
  return runWithContext(context, fn);
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
    req: {
      correlationId?: string;
      requestId?: string;
      actor?: string;
      adminContext?: { envName?: string };
      user?: { id?: string; sub?: string };
    },
    _res: unknown,
    next: () => void
  ): void {
    const id = req.correlationId ?? req.requestId;
    const actor = req.actor ?? req.adminContext?.envName ?? req.user?.id ?? req.user?.sub;
    if (id) {
      runWithContext({ correlationId: id, actor }, next);
    } else {
      next();
    }
  };
}
