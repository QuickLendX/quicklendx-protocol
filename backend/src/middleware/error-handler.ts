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

export const errorHandler = (
  err: AppError,
  req: Request,
  res: Response,
  next: NextFunction
) => {
  const status = err.status || 500;
  const message = err.message || "Internal Server Error";
  const code = err.code || "INTERNAL_ERROR";

  // Redact sensitive fields before logging so secrets never reach log sinks.
  const safeDetails = redactSensitiveFields(err.details);
  console.error(`[Error] ${code}: ${message}`, safeDetails ?? "");

  res.status(status).json({
    error: {
      message,
      code,
      details: process.env.NODE_ENV === "development" ? err.details : undefined,
    },
  });
};
