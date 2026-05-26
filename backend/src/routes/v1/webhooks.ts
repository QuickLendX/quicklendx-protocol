import { Router } from "express";
import express from "express";
import * as webhookController from "../../controllers/v1/webhooks";
import { webhookVerifyMiddleware } from "../../middleware/webhook-verify";

const router = Router();

// ---------------------------------------------------------------------------
// Subscriber management
// ---------------------------------------------------------------------------

/** Register a new subscriber and receive their initial secret. */
router.post("/subscribers", webhookController.registerSubscriber);

/** Get the public rotation state for a subscriber. */
router.get("/subscribers/:subscriberId", webhookController.getSubscriber);

// ---------------------------------------------------------------------------
// Secret rotation lifecycle
// ---------------------------------------------------------------------------

/** Step 1: Initiate rotation – generates a pending secret. */
router.post(
  "/subscribers/:subscriberId/rotate",
  webhookController.initiateRotation
);

/** Step 2a: Finalize rotation – promotes pending → primary. */
router.post(
  "/subscribers/:subscriberId/rotate/finalize",
  webhookController.finalizeRotation
);

/** Step 2b: Cancel rotation – discards pending secret. */
router.post(
  "/subscribers/:subscriberId/rotate/cancel",
  webhookController.cancelRotation
);

// ---------------------------------------------------------------------------
// Webhook ingest (signature-verified)
// ---------------------------------------------------------------------------

/**
 * Ingest endpoint for incoming webhook events.
 *
 * express.raw() captures the exact bytes sent by the caller so that the
 * HMAC-SHA256 signature can be verified against the unmodified body.
 * This must run BEFORE webhookVerifyMiddleware.
 */
router.post(
  "/ingest/:subscriberId",
  express.raw({ type: "*/*" }),
  webhookVerifyMiddleware,
  webhookController.ingestWebhook
);

export default router;
