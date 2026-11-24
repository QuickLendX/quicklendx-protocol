import { Request, Response, NextFunction } from "express";

export interface AppError extends Error {
  status?: number;
  code?: string;
  details?: any;
}

/**
 * Field names whose values must never appear in logs.
 * All entries are stored lower-cased; comparison is case-insensitive.
 * Extend this list as new sensitive fields are introduced.
 */
const REDACTED_FIELDS = new Set([
  "password",
  "secret",
  "token",
  "apikey",
  "api_key",
  "authorization",
  "privatekey",
  "private_key",
  "mnemonic",
  "seed",
]);

/**
 * Recursively redacts sensitive keys from an object before it is logged.
 * Returns a new object; the original is not mutated.
 */
export function redactSensitiveFields(obj: unknown): unknown {
  if (obj === null || typeof obj !== "object") return obj;
  if (Array.isArray(obj)) return obj.map(redactSensitiveFields);

  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj as Record<string, unknown>)) {
    result[key] = REDACTED_FIELDS.has(key.toLowerCase())
      ? "[REDACTED]"
      : redactSensitiveFields(value);
  }
  return result;
}

export interface ErrorHandlerOptions {
  /**
   * When true, `err.details` is included in the response body.
   * Defaults to `process.env.NODE_ENV === "development"` so callers
   * never need to mutate `process.env` to control this behaviour.
   */
  exposeDetails?: boolean;
}

/**
 * Express error-handling middleware.
 *
 * @param options.exposeDetails - Override whether details are exposed.
 *   Inject `true` only in non-production environments; never read or
 *   mutate `process.env.NODE_ENV` from request handlers.
/**
 * Error handler middleware.
 * Logs sanitized error details and returns a JSON error response.
 * Only development mode exposes `details` to the client.
 */
export const errorHandler = (
  err: AppError,
  req: Request,
  res: Response,
  next: NextFunction,
  options: ErrorHandlerOptions = {}
) => {
  if (res.headersSent) {
    next(err);
    return;
  }

  const status = err.status || 500;
  const message = err.message || "Internal Server Error";
  const code = err.code || "INTERNAL_ERROR";

  // Redact sensitive fields before logging so secrets never reach log sinks.
  const safeDetails = redactSensitiveFields(err.details);
  console.error(`[Error] ${code}: ${message}`, safeDetails ?? "");

  const exposeDetails =
    options.exposeDetails !== undefined
      ? options.exposeDetails
      : process.env.NODE_ENV === "development";

  res.status(status).json({
    error: {
      message,
      code,
      details: exposeDetails ? err.details : undefined,
    },
  });
};

/**
 * Returns an Express error-handler bound to the given options.
 * Use this to wire the middleware into an Express app without
 * mutating process.env.
 *
 * Example (non-production only):
 *   app.use(createErrorHandler({ exposeDetails: true }));
 */
export function createErrorHandler(options: ErrorHandlerOptions = {}) {
  return (err: AppError, req: Request, res: Response, next: NextFunction) =>
    errorHandler(err, req, res, next, options);
}
