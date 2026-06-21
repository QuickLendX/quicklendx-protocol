/**
 * Query-parameter validation middleware.
 *
 * Rejects requests whose query values exceed a safe length or contain
 * characters that have no legitimate use in this API (null bytes, raw
 * newlines, angle brackets used in HTML injection).  This is a defence-in-
 * depth layer; it does not replace proper parameterised queries or ORM
 * escaping in the data layer.
 *
 * Limits are intentionally generous so they never block valid traffic while
 * still catching obviously malformed or oversized inputs.
 */

import { Request, Response, NextFunction } from "express";

/** Maximum allowed length for any single query-parameter value. */
export const MAX_QUERY_PARAM_LENGTH = 256;

/**
 * Characters that are never valid in the query params this API accepts.
 * - \x00  null byte
 * - \r\n  log-injection / CRLF injection
 * - <>    HTML/script injection
 */
const FORBIDDEN_PATTERN = /[\x00\r\n<>]/;

/**
 * Returns true when the value is safe to pass to downstream handlers.
 */
export function isSafeQueryValue(value: string): boolean {
  if (value.length > MAX_QUERY_PARAM_LENGTH) return false;
  if (FORBIDDEN_PATTERN.test(value)) return false;
  return true;
}

/**
 * Express middleware that validates every query-parameter value on the
 * incoming request.  Responds with 400 Bad Request if any value is unsafe.
 */
export const validateQueryParams = (
  req: Request,
  res: Response,
  next: NextFunction
): void => {
  for (const [key, value] of Object.entries(req.query)) {
    const raw = Array.isArray(value) ? value.join("") : String(value ?? "");
    if (!isSafeQueryValue(raw)) {
      res.status(400).json({
        error: {
          message: `Invalid query parameter: ${key}`,
          code: "INVALID_QUERY_PARAM",
        },
      });
      return;
    }
  }
  next();
};
