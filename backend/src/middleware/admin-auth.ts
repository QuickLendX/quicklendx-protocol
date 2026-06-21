/**
 * Admin-endpoint authentication middleware.
 *
 * Protects privileged routes (e.g. POST /api/admin/maintenance) with a
 * static API key supplied via the `X-Admin-Key` request header.  The
 * expected key is read from the `ADMIN_API_KEY` environment variable so it
 * is never hard-coded in source.
 *
 * Security properties:
 * - Constant-time comparison prevents timing-based key enumeration.
 * - Returns 401 (not 403) when the header is absent so as not to reveal
 *   that the route exists to unauthenticated callers.
 * - Returns 403 when the header is present but the key is wrong, allowing
 *   callers to distinguish "no credentials" from "wrong credentials".
 * - Logs every rejected attempt (without echoing the supplied key) so that
 *   brute-force attempts are visible in audit logs.
 *
 * In production this middleware should be combined with network-level
 * controls (VPN / VPC) so that admin routes are never reachable from the
 * public internet.
 */

import { Request, Response, NextFunction } from "express";
import { timingSafeEqual } from "crypto";

/** Header name callers must supply. */
export const ADMIN_KEY_HEADER = "x-admin-key";

/**
 * Compares two strings in constant time to prevent timing attacks.
 * Returns false if the lengths differ (which itself leaks length, but the
 * key length is not secret in this scheme).
 */
export function timingSafeStringEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  const bufA = Buffer.from(a);
  const bufB = Buffer.from(b);
  return timingSafeEqual(bufA, bufB);
}

/**
 * Express middleware that enforces admin API-key authentication.
 * Mount this before any route handler that should be admin-only.
 */
export const adminAuth = (
  req: Request,
  res: Response,
  next: NextFunction
): void => {
  const expectedKey = process.env.ADMIN_API_KEY;

  // If no key is configured the endpoint is effectively disabled.
  if (!expectedKey) {
    res.status(503).json({
      error: {
        message: "Admin endpoint not configured",
        code: "ADMIN_NOT_CONFIGURED",
      },
    });
    return;
  }

  const suppliedKey = req.headers[ADMIN_KEY_HEADER];

  if (!suppliedKey) {
    res.status(401).json({
      error: {
        message: "Authentication required",
        code: "UNAUTHORIZED",
      },
    });
    return;
  }

  const keyStr = Array.isArray(suppliedKey) ? suppliedKey[0] : suppliedKey;

  if (!timingSafeStringEqual(keyStr, expectedKey)) {
    // Log the rejection without echoing the supplied key.
    console.warn(
      `[adminAuth] Rejected request from ${req.ip ?? "unknown"} – invalid key`
    );
    res.status(403).json({
      error: {
        message: "Forbidden",
        code: "FORBIDDEN",
      },
    });
    return;
  }

  next();
};
