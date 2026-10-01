import { Request, Response, NextFunction } from "express";
import { webhookSecretService } from "../services/webhookSecretService";
import { WebhookSecretError } from "../services/webhookSecretService";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Header name carrying the HMAC-SHA256 signature. */
export const WEBHOOK_SIGNATURE_HEADER = "x-webhook-signature";

/** Header name carrying the subscriber identifier. */
export const WEBHOOK_SUBSCRIBER_HEADER = "x-webhook-subscriber-id";

/** Header name carrying the signature algorithm. */
export const WEBHOOK_ALGORITHM_HEADER = "x-webhook-signature-algorithm";

// ---------------------------------------------------------------------------
// Augment Express Request
// ---------------------------------------------------------------------------

declare global {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace Express {
    interface Request {
      /** Set by webhookVerifyMiddleware when signature is valid. */
      webhookSubscriberId?: string;
      /** Which secret slot matched: "primary" | "pending". */
      webhookMatchedSecret?: "primary" | "pending";
    }
  }
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/**
 * Express middleware that verifies an incoming webhook request's signature
 * (HMAC-SHA256 or Ed25519) against the subscriber's current secret(s)/key(s).
 *
 * During a rotation grace window both the primary and pending secrets are
 * accepted, enabling zero-downtime key rollover for integrators.
 *
 * Prerequisites:
 *   - `express.raw({ type: '*\/*' })` (or equivalent) must be applied
 *     **before** this middleware so that `req.body` is a raw Buffer.
 *   - The caller must set `X-Webhook-Subscriber-Id` and
 *     `X-Webhook-Signature` headers.
 *   - If negotiated, `X-Webhook-Signature-Algorithm` must be set to `"ed25519"`.
 *
 * On success the middleware calls `next()` and attaches:
 *   - `req.webhookSubscriberId`
 *   - `req.webhookMatchedSecret`
 *
 * On failure it responds with 401 (invalid signature) or 400 (missing
 * headers).  Subscriber-not-found is treated as 401 to avoid enumeration.
 *
 * SECURITY: Error messages deliberately omit secret values and subscriber
 * details to prevent information leakage.
 */
export function webhookVerifyMiddleware(
  req: Request,
  res: Response,
  next: NextFunction
): void {
  const subscriberId = req.headers[WEBHOOK_SUBSCRIBER_HEADER];
  const signature = req.headers[WEBHOOK_SIGNATURE_HEADER];
  const algoHeader = req.headers[WEBHOOK_ALGORITHM_HEADER];

  // Validate required headers.
  if (!subscriberId || typeof subscriberId !== "string") {
    res.status(400).json({
      error: {
        message: "Missing required header: X-Webhook-Subscriber-Id",
        code: "MISSING_SUBSCRIBER_HEADER",
      },
    });
    return;
  }

  if (!signature || typeof signature !== "string") {
    res.status(400).json({
      error: {
        message: "Missing required header: X-Webhook-Signature",
        code: "MISSING_SIGNATURE_HEADER",
      },
    });
    return;
  }

  // Obtain raw body.  express.raw() stores it as a Buffer on req.body.
  // Fall back to an empty buffer if the body is absent (e.g. GET requests).
  const rawBody: Buffer =
    Buffer.isBuffer(req.body)
      ? req.body
      : Buffer.from(
          typeof req.body === "string"
            ? req.body
            : JSON.stringify(req.body ?? ""),
          "utf8"
        );

  // Negotiate signature algorithm.
  let algo: "hmac-sha256" | "ed25519" = "hmac-sha256";
  if (algoHeader !== undefined) {
    if (typeof algoHeader === "string" && (algoHeader === "hmac-sha256" || algoHeader === "ed25519")) {
      algo = algoHeader;
    } else {
      // Unknown algorithm - run dummy check to preserve timing-safe error response
      webhookSecretService.dummyVerifyHmac(rawBody);
      res.status(401).json({
        error: {
          message: "Webhook signature verification failed",
          code: "INVALID_WEBHOOK_SIGNATURE",
        },
      });
      return;
    }
  }

  try {
    const result = webhookSecretService.verifySignature(
      subscriberId,
      rawBody,
      signature,
      algo
    );

    if (!result.valid) {
      res.status(401).json({
        error: {
          message: "Webhook signature verification failed",
          code: "INVALID_WEBHOOK_SIGNATURE",
        },
      });
      return;
    }

    // Attach verified context to the request for downstream handlers.
    req.webhookSubscriberId = subscriberId;
    req.webhookMatchedSecret = result.matched_secret ?? undefined;

    next();
  } catch (err) {
    if (err instanceof WebhookSecretError) {
      // Treat subscriber-not-found as 401 to avoid subscriber enumeration.
      const status = err.code === "SUBSCRIBER_NOT_FOUND" ? 401 : err.status;

      if (err.code === "SUBSCRIBER_NOT_FOUND") {
        // Run dummy signature verification to match execution time
        if (algo === "ed25519") {
          webhookSecretService.dummyVerifyEd25519(rawBody);
        } else {
          webhookSecretService.dummyVerifyHmac(rawBody);
        }
      }

      res.status(status).json({
        error: {
          // Generic message for 401 to avoid leaking subscriber existence.
          message:
            status === 401
              ? "Webhook signature verification failed"
              : err.message,
          code: err.code,
        },
      });
      return;
    }
    next(err);
  }
}
