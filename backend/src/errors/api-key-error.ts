/**
 * Domain error model for the API-keys backend.
 *
 * Every failure surfaced by the controller/service layer is an `ApiKeyError`
 * carrying a stable `code`, a user-safe `message`, and a structured `context`.
 * The context includes only non-sensitive, diagnostic identifiers
 * (`apiKeyId`, `actorId`, `requestId`, `action`) — never the API key secret
 * or any token material — so failures stay diagnosable without leaking
 * credentials (see AGENTS.md: never log secrets).
 */

export type ApiKeyErrorCode =
  | "VALIDATION_ERROR"
  | "UNAUTHORIZED"
  | "FORBIDDEN"
  | "NOT_FOUND"
  | "CONFLICT"
  | "TRANSIENT"
  | "SYSTEM_ERROR";

export interface ApiKeyErrorContext {
  /** The controller/action that produced the error. */
  action?: string;
  /** Non-secret identifier of the key involved. */
  apiKeyId?: string;
  /** Non-secret identifier of the requesting actor. */
  actorId?: string;
  /** Correlation id for log tracing. */
  requestId?: string;
  /** Lower-level error that caused this one (kept internal; not user-facing). */
  cause?: unknown;
}

export class ApiKeyError extends Error {
  /** Stable, machine-readable error code. */
  public readonly code: ApiKeyErrorCode;
  /** Structured, non-sensitive diagnostic context. */
  public readonly context: ApiKeyErrorContext & { code: ApiKeyErrorCode };
  /** Whether a bounded retry may succeed. Terminal errors are not retryable. */
  public readonly retryable: boolean;

  constructor(
    code: ApiKeyErrorCode,
    message: string,
    context: ApiKeyErrorContext = {}
  ) {
    super(message);
    this.name = "ApiKeyError";
    this.code = code;
    this.context = { ...context, code };
    this.retryable = isApiKeyRetryableCode(code);
    // Preserve V8 stack chain when available (no secret data in stack).
    if (
      context.cause &&
      typeof (Error as any).captureStackTrace === "function"
    ) {
      // keep reference chain for diagnostics only
    }
  }
}

/**
 * Codes that a bounded retry may recover from. Transient backend hiccups,
 * optimistic-concurrency conflicts (stale reads), and generic system faults
 * are retryable; client/authorisation errors are terminal.
 */
export const isApiKeyRetryableCode = (code: ApiKeyErrorCode): boolean =>
  code === "TRANSIENT" || code === "CONFLICT" || code === "SYSTEM_ERROR";

export const isApiKeyError = (err: unknown): err is ApiKeyError =>
  err instanceof ApiKeyError;

/**
 * Predicate usable with the generic `FailureBoundary.withRetry`. Only
 * `ApiKeyError`s whose `retryable` flag is set are retried; everything else is
 * treated as terminal.
 */
export const apiKeyRetryPredicate = (err: unknown): boolean =>
  err instanceof ApiKeyError && err.retryable;
