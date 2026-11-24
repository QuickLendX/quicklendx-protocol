import { Router, Request, Response } from "express";
import { auditService } from "../../services/auditService";
import { apiKeyAuth, AuthenticatedRequest } from "../../middleware/api-key-auth";
import { auditMiddleware } from "../../middleware/auditMiddleware";
import {
  AuditOperationSchema,
  AuditEntrySchema,
} from "../../types/audit";
import { requireAdminRoles } from "../../middleware/rbac";
import { OPERATIONS_WRITE_ROLES, SUPPORT_READ_ROLES } from "../../types/rbac";
import { featureFlagService } from "../../services/featureFlagService";
import { auditLogService } from "../../services/auditLogService";
import { webhookDeliveryRepo } from "../../services/webhookDeliveryRepo";

const router = Router();

router.use(apiKeyAuth);
router.use(auditMiddleware);

router.get("/audit", (req: AuthenticatedRequest, res: Response) => {
  try {
    const result = auditService.query({
      actor: req.query["actor"] as string | undefined,
      operation: req.query["operation"] as string | undefined,
      from: req.query["from"] as string | undefined,
      to: req.query["to"] as string | undefined,
      limit: req.query["limit"] as string | undefined,
      offset: req.query["offset"] as string | undefined,
    });

    res.json(result);
  } catch (err) {
    const message = err instanceof Error ? err.message : "Invalid query parameters";
    res.status(400).json({
      error: {
        message,
        code: "INVALID_AUDIT_QUERY",
      },
    });
  }
});

router.get("/audit/operations", (req: Request, res: Response) => {
  const operations = AuditOperationSchema.options;
  res.json({ operations });
});

router.get(
  "/audit/export",
  (req: AuthenticatedRequest, res: Response) => {
    try {
      const from = req.query["from"] as string | undefined;
      const to = req.query["to"] as string | undefined;

      const result = auditService.query({ from, to, limit: 10000, offset: 0 });

      res.setHeader("Content-Type", "application/x-ndjson");
      res.setHeader(
        "Content-Disposition",
        `attachment; filename="audit-export-${new Date().toISOString().slice(0, 10)}.ndjson"`
      );

      for (const entry of result.entries) {
        res.write(JSON.stringify(entry) + "\n");
      }
      res.end();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Export failed";
      res.status(400).json({
        error: {
          message,
          code: "AUDIT_EXPORT_FAILED",
        },
      });
    }
  }
);

// ---------------------------------------------------------------------------
// Feature Flag Admin Endpoints
// ---------------------------------------------------------------------------

/**
 * GET /api/v1/admin/feature-flags
 *
 * List all feature flags across all tenants (admin overview).
 * Requires: operations_admin or super_admin.
 */
router.get(
  "/feature-flags",
  requireAdminRoles(OPERATIONS_WRITE_ROLES, "list_all_feature_flags"),
  (req: Request, res: Response) => {
    try {
      const flags = featureFlagService.listAllFlags();
      res.json({ flags });
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to list feature flags";
      res.status(500).json({ error: { message, code: "FEATURE_FLAG_ERROR" } });
    }
  }
);

/**
 * GET /api/v1/admin/feature-flags/:apiKeyId
 *
 * List all feature flags for a specific API key tenant.
 * Requires: support (read), operations_admin, or super_admin.
 */
router.get(
  "/feature-flags/:apiKeyId",
  requireAdminRoles(SUPPORT_READ_ROLES, "list_feature_flags_for_key"),
  (req: Request, res: Response) => {
    try {
      const { apiKeyId } = req.params as { apiKeyId: string };
      const flags = featureFlagService.listFlagsForKey(apiKeyId);
      res.json({ api_key_id: apiKeyId, flags });
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to list feature flags";
      res.status(500).json({ error: { message, code: "FEATURE_FLAG_ERROR" } });
    }
  }
);

/**
 * PUT /api/v1/admin/feature-flags/:apiKeyId/:flag
 *
 * Enable or disable a feature flag for a specific tenant.
 *
 * Body: { "enabled": true|false }
 *
 * Requires: operations_admin or super_admin.
 * The toggle is recorded via auditLogService for a full admin action history.
 */
router.put(
  "/feature-flags/:apiKeyId/:flag",
  requireAdminRoles(OPERATIONS_WRITE_ROLES, "toggle_feature_flag"),
  (req: Request, res: Response) => {
    try {
      const { apiKeyId, flag } = req.params as { apiKeyId: string; flag: string };
      const body = req.body as { enabled?: unknown };

      if (typeof body.enabled !== "boolean") {
        res.status(400).json({
          error: {
            message: '"enabled" must be a boolean',
            code: "VALIDATION_ERROR",
          },
        });
        return;
      }

      // Determine the actor from the admin context (set by requireAdminRoles)
      const adminContext = (req as any).adminContext as { role: string; envName: string } | undefined;
      const updatedBy = adminContext?.envName ?? "unknown";

      const result = featureFlagService.setFlag({
        api_key_id: apiKeyId,
        flag,
        enabled: body.enabled,
        updated_by: updatedBy,
      });

      // Record audit event for the toggle
      const ip =
        (req.headers["x-forwarded-for"] as string)?.split(",")[0]?.trim() ||
        req.ip ||
        "unknown";

      auditLogService.recordAdminAction({
        action: "FEATURE_FLAG_TOGGLE",
        role: adminContext?.role as any ?? "operations_admin",
        method: req.method,
        path: req.path,
        ip,
        metadata: {
          api_key_id: apiKeyId,
          flag,
          enabled: body.enabled,
          updated_by: updatedBy,
        },
      });

      res.json({ flag: result });
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to toggle feature flag";
      res.status(500).json({ error: { message, code: "FEATURE_FLAG_ERROR" } });
    }
  }
);

/**
 * DELETE /api/v1/admin/feature-flags/:apiKeyId/:flag
 *
 * Remove a feature flag row entirely (reverts to default-deny).
 * Requires: operations_admin or super_admin.
 */
router.delete(
  "/feature-flags/:apiKeyId/:flag",
  requireAdminRoles(OPERATIONS_WRITE_ROLES, "delete_feature_flag"),
  (req: Request, res: Response) => {
    try {
      const { apiKeyId, flag } = req.params as { apiKeyId: string; flag: string };

      const deleted = featureFlagService.deleteFlag(apiKeyId, flag);
      if (!deleted) {
        res.status(404).json({
          error: {
            message: `Feature flag "${flag}" not found for api_key_id "${apiKeyId}"`,
            code: "NOT_FOUND",
          },
        });
        return;
      }

      const adminContext = (req as any).adminContext as { role: string; envName: string } | undefined;
      const ip =
        (req.headers["x-forwarded-for"] as string)?.split(",")[0]?.trim() ||
        req.ip ||
        "unknown";

      auditLogService.recordAdminAction({
        action: "FEATURE_FLAG_DELETE",
        role: adminContext?.role as any ?? "operations_admin",
        method: req.method,
        path: req.path,
        ip,
        metadata: { api_key_id: apiKeyId, flag },
      });

      res.status(204).end();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to delete feature flag";
      res.status(500).json({ error: { message, code: "FEATURE_FLAG_ERROR" } });
    }
  }
);

// ---------------------------------------------------------------------------
// Webhook Dead-Letter Endpoints
// ---------------------------------------------------------------------------

/**
 * GET /api/v1/admin/webhooks/dead-letters
 *
 * List exhausted and permanently failed webhook deliveries with pagination.
 * Requires: support, operations_admin, or super_admin.
 */
router.get(
  "/webhooks/dead-letters",
  requireAdminRoles(SUPPORT_READ_ROLES, "list_webhook_dead_letters"),
  (req: Request, res: Response) => {
    try {
      const limitRaw = req.query["limit"];
      const offsetRaw = req.query["offset"];

      const limit = typeof limitRaw === "string" ? parseInt(limitRaw, 10) : undefined;
      const offset = typeof offsetRaw === "string" ? parseInt(offsetRaw, 10) : undefined;

      const result = webhookDeliveryRepo.listDeadLetters({
        limit: Number.isFinite(limit) ? limit : undefined,
        offset: Number.isFinite(offset) ? offset : undefined,
      });

      res.json({
        dead_letters: result.entries,
        deadLetters: result.entries,
        entries: result.entries,
        data: result.entries,
        pagination: {
          limit: result.limit,
          offset: result.offset,
          total: result.total,
          hasMore: result.hasMore,
          has_more: result.hasMore,
        },
        total: result.total,
      });
    } catch (err) {
      const message =
        err instanceof Error ? err.message : "Failed to list dead letters";
      res.status(500).json({ error: { message, code: "DEAD_LETTER_ERROR" } });
    }
  }
);

/**
 * POST /api/v1/admin/webhooks/dead-letters/:id/replay
 *
 * Re-enqueue a dead-letter delivery with a fresh attempt count.
 * Requires: operations_admin or super_admin.
 * Replay re-enqueues once; subsequent replays return 409 Conflict.
 */
router.post(
  "/webhooks/dead-letters/:id/replay",
  requireAdminRoles(OPERATIONS_WRITE_ROLES, "replay_webhook_dead_letter"),
  (req: Request, res: Response) => {
    try {
      const { id } = req.params as { id: string };

      const result = webhookDeliveryRepo.replayDeadLetter(id);
      if (!result) {
        res.status(404).json({
          error: {
            message: `Dead letter "${id}" not found`,
            code: "NOT_FOUND",
          },
        });
        return;
      }

      const adminContext = (req as any).adminContext as
        | { role: string; envName: string }
        | undefined;
      const ip =
        (req.headers["x-forwarded-for"] as string)?.split(",")[0]?.trim() ||
        req.ip ||
        "unknown";

      auditLogService.recordAdminAction({
        action: "WEBHOOK_DEAD_LETTER_REPLAY",
        role: (adminContext?.role as any) ?? "operations_admin",
        method: req.method,
        path: req.path,
        ip,
        metadata: {
          deadLetterId: id,
          reEnqueuedDeliveryId: result.newDelivery.id,
          eventType: result.deadLetter.eventType,
        },
      });

      res.status(200).json({
        success: true,
        message: "Dead letter replayed and re-enqueued successfully",
        dead_letter: result.deadLetter,
        delivery: result.newDelivery,
      });
    } catch (err: any) {
      if (
        err?.code === "ALREADY_REPLAYED" ||
        err?.message?.includes("already been replayed")
      ) {
        res.status(409).json({
          error: {
            message: err.message || "Dead letter has already been replayed",
            code: "ALREADY_REPLAYED",
          },
        });
        return;
      }

      const message =
        err instanceof Error ? err.message : "Failed to replay dead letter";
      res.status(500).json({ error: { message, code: "REPLAY_ERROR" } });
    }
  }
);

export default router;
