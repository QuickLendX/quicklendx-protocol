import { Request, Response, NextFunction } from "express";

/**
 * Maximum number of requests that may be in-flight simultaneously.
 * Requests arriving when this cap is reached receive 503 immediately.
 *
 * Security: keeps the cap low enough that a single burst cannot exhaust
 * server resources (file descriptors, memory, downstream connections).
 */
export const CONCURRENCY_CAP =
  process.env.NODE_ENV === "test" ? 5 : 100;

/**
 * Per-request timeout in milliseconds.
 * If a handler has not called `res.end()` within this window the middleware
 * aborts the request with 503 and a `Retry-After` header.
 *
 * Security: prevents slow-loris / slow-response amplification where an
 * attacker holds connections open indefinitely to exhaust the concurrency cap.
 */
export const REQUEST_TIMEOUT_MS =
  process.env.NODE_ENV === "test" ? 200 : 10_000;

/**
 * Seconds the client should wait before retrying after a 503.
 * Sent in the `Retry-After` header on both cap-exceeded and timeout responses.
 */
export const RETRY_AFTER_SECONDS = 5;

/** Live count of requests currently being processed. */
let activeRequests = 0;

/** Exposed for tests to inspect or reset state. */
export const getActiveRequests = () => activeRequests;
export const resetActiveRequests = () => {
  activeRequests = 0;
};

/**
 * Emit a 503 Service Unavailable response with a `Retry-After` header.
 *
 * @param res     - Express response object.
 * @param reason  - Human-readable reason code included in the JSON body.
 */
function shed(res: Response, reason: "CONCURRENCY_CAP" | "TIMEOUT"): void {
  if (res.headersSent) return;
  res
    .status(503)
    .set("Retry-After", String(RETRY_AFTER_SECONDS))
    .json({
      error: {
        message:
          reason === "CONCURRENCY_CAP"
            ? "Server is under heavy load. Please retry shortly."
            : "Request timed out. Please retry shortly.",
        code: reason,
        retryAfter: RETRY_AFTER_SECONDS,
      },
    });
}

/**
 * Load-shedding middleware.
 *
 * Enforces two independent protections:
 *
 * 1. **Concurrency cap** — if `activeRequests >= CONCURRENCY_CAP` the request
 *    is rejected immediately with 503 + `Retry-After`.  The counter is never
 *    incremented for rejected requests, so the cap is a hard ceiling.
 *
 * 2. **Request timeout** — once a request is admitted, a timer fires after
 *    `REQUEST_TIMEOUT_MS`.  If the response has not been sent by then the
 *    middleware sends 503 + `Retry-After` and decrements the counter.
 *    The `finish` / `close` events on the response always decrement the
 *    counter exactly once, preventing leaks.
 *
 * Security notes:
 * - The counter is decremented on `res.finish` *and* `res.close` (client
 *   disconnect) so aborted requests cannot hold a slot indefinitely.
 * - `res.headersSent` is checked before writing to avoid double-response
 *   errors when the handler and the timeout race.
 * - No request body is buffered here; the middleware is purely control-flow.
 */
export function loadSheddingMiddleware(
  req: Request,
  res: Response,
  next: NextFunction
): void {
  // ── 1. Concurrency cap check ──────────────────────────────────────────────
  if (activeRequests >= CONCURRENCY_CAP) {
    shed(res, "CONCURRENCY_CAP");
    return;
  }

  activeRequests++;

  // ── 2. Decrement on response completion (finish or client disconnect) ─────
  let decremented = false;
  const decrement = () => {
    if (!decremented) {
      decremented = true;
      activeRequests--;
    }
  };
  res.once("finish", decrement);
  res.once("close", decrement);

  // ── 3. Per-request timeout ────────────────────────────────────────────────
  const timer = setTimeout(() => {
    shed(res, "TIMEOUT");
    // Ensure the slot is released even if the handler never calls next().
    decrement();
  }, REQUEST_TIMEOUT_MS);

  // Clear the timer as soon as the response is sent so it does not fire late.
  res.once("finish", () => clearTimeout(timer));
  res.once("close", () => clearTimeout(timer));

  next();
}
