/**
 * feature-flag.ts
 *
 * Express middleware that gates a route behind a per-tenant feature flag.
 *
 * Usage:
 *   import { requireFlag } from "../middleware/feature-flag";
 *
 *   router.get(
 *     "/kyc/tiers",
 *     apiKeyAuthMiddleware,
 *     requireFlag("kyc_tiers"),
 *     kycTiersController,
 *   );
 *
 * Behaviour:
 * - Reads `req.apiKey.id` set by `apiKeyAuthMiddleware`.
 * - If the flag is ON  → calls `next()`.
 * - If the flag is OFF → responds 404 (endpoint appears non-existent to the caller).
 * - If no `req.apiKey` is present → responds 401.
 *
 * The 404 response (rather than 403) is intentional: it avoids leaking that a
 * feature exists but is disabled, which matches the product requirement of
 * staged rollout without revealing roadmap information.
 */

import { Request, Response, NextFunction } from "express";
import { featureFlagService } from "../services/featureFlagService";

/**
 * Returns Express middleware that 404s when the named flag is off for the
 * authenticated tenant.
 *
 * @param flag  The feature flag name (e.g. "kyc_tiers", "dispute_composer").
 */
export function requireFlag(flag: string) {
  return function featureFlagMiddleware(
    req: Request,
    res: Response,
    next: NextFunction
  ): void {
    // The API key must already be resolved by an upstream auth middleware.
    if (!req.apiKey) {
      res.status(401).json({
        error: {
          message: "Authentication required",
          code: "UNAUTHORIZED",
        },
      });
      return;
    }

    const apiKeyId = req.apiKey.id;
    const enabled = featureFlagService.isEnabled(apiKeyId, flag);

    if (!enabled) {
      res.status(404).json({
        error: {
          message: "Resource not found",
          code: "NOT_FOUND",
        },
      });
      return;
    }

    next();
  };
}
