import { Request, Response, NextFunction } from "express";
import { webhookSecretService } from "../../services/webhookSecretService";
import { WebhookSecretError } from "../../services/webhookSecretService";
import {
  RegisterSubscriberRequestSchema,
  InitiateRotationRequestSchema,
} from "../../types/webhook";
import { webhookQueueService } from "../../services/webhookQueueService";
import { auditService } from "../../services/auditService";
import { getAdminContext } from "../../middleware/rbac";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Translates a WebhookSecretError into a structured HTTP response.
 * All other errors are forwarded to the global error handler.
 */
function handleError(
  err: unknown,
  res: Response,
  next: NextFunction
): void {
  if (err instanceof WebhookSecretError) {
    res.status(err.status).json({
      error: {
        message: err.message,
        code: err.code,
      },
    });
    return;
  }
  next(err);
}

// ---------------------------------------------------------------------------
// Controllers
// ---------------------------------------------------------------------------

/**
 * POST /api/v1/webhooks/subscribers
 *
 * Registers a new subscriber and returns their initial secret.
 * The secret is returned **once** – the caller must store it securely.
 */
export const registerSubscriber = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const parsed = RegisterSubscriberRequestSchema.safeParse(req.body);
    if (!parsed.success) {
      res.status(400).json({
        error: {
          message: "Invalid request body",
          code: "VALIDATION_ERROR",
          details: parsed.error.flatten(),
        },
      });
      return;
    }

    const { subscriber_id, grace_period_seconds, algorithm } = parsed.data;
    const result = webhookSecretService.registerSubscriber(
      subscriber_id,
      grace_period_seconds,
      algorithm
    );

    res.status(201).json({
      ...result.view,
      /**
       * The initial secret is returned once at registration.
       * Treat this like a password – store it in a secrets manager.
       */
      initial_secret: result.initial_secret,
    });
  } catch (err) {
    handleError(err, res, next);
  }
};

/**
 * GET /api/v1/webhooks/subscribers/:subscriberId
 *
 * Returns the public (non-secret) view of a subscriber's rotation state.
 */
export const getSubscriber = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const subscriberId = req.params["subscriberId"] as string;
    const view = webhookSecretService.getSubscriberView(subscriberId);
    res.json(view);
  } catch (err) {
    handleError(err, res, next);
  }
};

/**
 * POST /api/v1/webhooks/subscribers/:subscriberId/rotate
 *
 * Initiates a secret rotation.  Returns the new pending secret **once**.
 * Both the old and new secrets are accepted during the grace window.
 */
export const initiateRotation = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const subscriberId = req.params["subscriberId"] as string;

    const parsed = InitiateRotationRequestSchema.safeParse(req.body ?? {});
    if (!parsed.success) {
      res.status(400).json({
        error: {
          message: "Invalid request body",
          code: "VALIDATION_ERROR",
          details: parsed.error.flatten(),
        },
      });
      return;
    }

    const result = webhookSecretService.initiateRotation(
      subscriberId,
      parsed.data.grace_period_seconds
    );

    res.status(202).json(result);
  } catch (err) {
    handleError(err, res, next);
  }
};

/**
 * POST /api/v1/webhooks/subscribers/:subscriberId/rotate/finalize
 *
 * Finalizes the rotation: promotes the pending secret to primary and
 * discards the old secret.  After this call only the new secret is valid.
 */
export const finalizeRotation = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const subscriberId = req.params["subscriberId"] as string;
    const result = webhookSecretService.finalizeRotation(subscriberId);
    res.json(result);
  } catch (err) {
    handleError(err, res, next);
  }
};

/**
 * POST /api/v1/webhooks/subscribers/:subscriberId/rotate/cancel
 *
 * Cancels an in-progress rotation, reverting to the primary secret only.
 */
export const cancelRotation = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const subscriberId = req.params["subscriberId"] as string;
    const view = webhookSecretService.cancelRotation(subscriberId);
    res.json(view);
  } catch (err) {
    handleError(err, res, next);
  }
};

/**
 * POST /api/v1/webhooks/ingest/:subscriberId
 *
 * Example ingest endpoint protected by webhookVerifyMiddleware.
 * In a real system this would process the event payload.
 */
export const ingestWebhook = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    // req.webhookSubscriberId and req.webhookMatchedSecret are set by
    // webhookVerifyMiddleware before this handler is reached.
    res.json({
      received: true,
      subscriber_id: req.webhookSubscriberId,
      matched_secret: req.webhookMatchedSecret,
    });
  } catch (err) {
    next(err);
  }
};

/**
 * POST /api/v1/admin/webhooks/:subscriberId/drain
 *
 * Pauses webhook delivery for a subscriber by marking all pending, processing, and
 * failed deliveries as dead_letter. Surfaces progress.
 */
export const drainWebhook = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const subscriberId = req.params["subscriberId"] as string;

    // Check if the subscriber exists (throws 404 WebhookSecretError if not found)
    webhookSecretService.getSubscriberView(subscriberId);

    // Call drain operation
    const progress = webhookQueueService.drain(subscriberId);

    // Resolve actor from admin context or request
    let actor = "unknown";
    try {
      const adminCtx = getAdminContext(req);
      actor = adminCtx.envName || "unknown";
    } catch {
      actor = (req as any).actor || "unknown";
    }

    const clientIp =
      (req.headers["x-forwarded-for"] as string)?.split(",")[0]?.trim() ||
      (req.headers["x-real-ip"] as string) ||
      req.socket.remoteAddress ||
      "unknown";

    // Append to audit log
    const auditEntry = auditService.append({
      actor,
      operation: "WEBHOOK_DRAIN",
      params: { subscriberId },
      redactedParams: { subscriberId },
      ip: clientIp,
      userAgent: req.headers["user-agent"] || "unknown",
      effect: `Drained ${progress.drained} pending webhooks for subscriber ${subscriberId}`,
      success: true,
    });

    res.json({
      pending: progress.pending,
      drained: progress.drained,
      audit_entry_id: auditEntry.id,
    });
  } catch (err) {
    handleError(err, res, next);
  }
};
