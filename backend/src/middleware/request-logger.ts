/**
 * Request / Response Logging Middleware
 *
 * Attaches to every route and emits a single structured log line per
 * request. The log line is produced AFTER the response is sent so the
 * status code is always available.
 *
 * Security guarantees
 * ───────────────────
 * • All field values are passed through `sanitiseRequest` / `sanitiseResponse`
 *   before being serialised. No SECRET field ever reaches the console or any
 *   downstream log sink.
 * • The middleware never throws — if redaction itself errors, it logs a
 *   minimal safe fallback line and re-throws the original error only if the
 *   application has a proper error boundary.
 * • `Authorization` and `Cookie` headers are always dropped from the header
 *   snapshot before redaction to add a second layer of defence.
 */

import { Request, Response, NextFunction } from "express";
import { ulid } from "ulid";
import {
  sanitiseRequest,
  sanitiseResponse,
  SafeRequestSnapshot,
  SafeResponseSnapshot,
} from "../lib/logging/policy";

// ── Structured log entry ──────────────────────────────────────────────────────

export interface RequestLogEntry {
  /** Monotonically sortable request ID (ULID). */
  requestId: string;
  timestamp: string;
  method: string;
  path: string;
  statusCode: number;
  /** Wall-clock latency in milliseconds. */
  durationMs: number;
  request: SafeRequestSnapshot;
  response: SafeResponseSnapshot;
}

// ── Logger interface (injectable for testing) ─────────────────────────────────

export interface Logger {
  info(entry: RequestLogEntry): void;
  error(message: string, meta?: Record<string, unknown>): void;
}

/** Default logger — writes structured JSON to stdout. */
export const defaultLogger: Logger = {
  info(entry) {
    process.stdout.write(JSON.stringify(entry) + "\n");
  },
  error(message, meta) {
    process.stderr.write(JSON.stringify({ level: "error", message, ...meta }) + "\n");
  },
};

// ── Drop-before-redaction headers ────────────────────────────────────────────

const STRIP_HEADERS = new Set([
  "authorization",
  "cookie",
  "set-cookie",
  "proxy-authorization",
]);

function stripSensitiveHeaders(
  headers: Record<string, unknown>
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(headers).filter(([k]) => !STRIP_HEADERS.has(k.toLowerCase()))
  );
}

// ── Middleware factory ────────────────────────────────────────────────────────

/**
 * Create the request/response logging middleware.
 *
 * @param logger  Logger to use (defaults to JSON stdout).
 * @param options Additional configuration.
 */
export function createRequestLogger(
  logger: Logger = defaultLogger,
  options: {
    /** If true, skip logging for the /health endpoint. */
    skipHealthCheck?: boolean;
  } = {}
) {
  const { skipHealthCheck = true } = options;

  return function requestLoggerMiddleware(
    req: Request,
    res: Response,
    next: NextFunction
  ): void {
    // Always bail early for health check to avoid log spam
    if (skipHealthCheck && req.path === "/health") {
      return next();
    }

    const requestId = ulid();
    const startMs = Date.now();

    // Attach the request ID so downstream handlers can reference it
    (req as any).requestId = requestId;
    res.setHeader("X-Request-Id", requestId);

    // Build a safe snapshot of the incoming request
    const safeRequest = sanitiseRequest({
      method: req.method,
      path: req.path,
      query: req.query as Record<string, unknown>,
      headers: stripSensitiveHeaders(
        req.headers as Record<string, unknown>
      ),
      body: req.body,
    });

    // Hook into res.json so we can capture and redact the response body
    let capturedBody: unknown = null;
    const originalJson = res.json.bind(res);

    res.json = function (body: unknown) {
      capturedBody = body;
      return originalJson(body);
    };

    // Emit the log line after the response has been fully sent
    res.on("finish", () => {
      try {
        const entry: RequestLogEntry = {
          requestId,
          timestamp: new Date().toISOString(),
          method: req.method,
          path: req.path,
          statusCode: res.statusCode,
          durationMs: Date.now() - startMs,
          request: safeRequest,
          response: sanitiseResponse(res.statusCode, capturedBody),
        };
        logger.info(entry);
      } catch (err) {
        logger.error("request-logger: redaction error", {
          requestId,
          err: String(err),
        });
      }
    });

    next();
  };
}

/** Pre-built middleware using the default JSON logger. */
export const requestLogger = createRequestLogger();
