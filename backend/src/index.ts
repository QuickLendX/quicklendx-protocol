import express from "express";
import cors from "cors";
import dotenv from "dotenv";
import { getAdminContext, requireAdminRoles } from "./middleware/rbac";
import { adminControlService } from "./services/adminControlService";
import { auditLogService } from "./services/auditLogService";
import { statusService } from "./services/statusService";
import {
  OPERATIONS_WRITE_ROLES,
  SUPPORT_READ_ROLES,
  SUPER_ADMIN_ONLY_ROLES,
} from "./types/rbac";

dotenv.config();

const app = express();
const port = process.env.PORT || 3001;

app.use(cors());
app.use(express.json());

/**
 * @openapi
 * /api/status:
 *   get:
 *     summary: Get system status
 *     description: Reports maintenance, degraded mode, and index lag.
 *     responses:
 *       200:
 *         description: OK
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/Status'
 */
app.get("/api/status", async (req, res) => {
  try {
    const status = await statusService.getStatus();

    // Cache safely: 30 seconds max age
    res.setHeader("Cache-Control", "public, max-age=30");
    res.json(status);
  } catch (error) {
    console.error("Status check failed:", error);
    res.status(500).json({ error: "Internal server error" });
  }
});

app.get(
  "/api/admin/status",
  requireAdminRoles(SUPPORT_READ_ROLES, "admin.status.read"),
  async (req, res) => {
    try {
      const adminContext = getAdminContext(req);
      const status = await statusService.getStatus();

      res.json({
        requested_by: adminContext.role,
        status,
        dangerous_config: adminControlService.getDangerousConfig(),
        queued_backfills: adminControlService.listBackfillJobs().length,
      });
    } catch (error) {
      console.error("Admin status check failed:", error);
      res.status(500).json({ error: "Internal server error" });
    }
  },
);

app.get(
  "/api/admin/audit-logs",
  requireAdminRoles(SUPPORT_READ_ROLES, "admin.audit_logs.read"),
  (req, res) => {
    const rawLimit = req.query.limit;
    const limit = rawLimit === undefined ? 50 : Number(rawLimit);
    if (!Number.isInteger(limit) || limit < 1) {
      return res.status(400).json({ error: "Invalid audit log limit" });
    }

    res.json({
      entries: auditLogService.listEntries(limit),
    });
  },
);

app.post(
  "/api/admin/maintenance",
  requireAdminRoles(OPERATIONS_WRITE_ROLES, "admin.maintenance.write"),
  (req, res) => {
    const adminContext = getAdminContext(req);
    const { enabled } = req.body;
    if (typeof enabled !== "boolean") {
      return res.status(400).json({ error: "Invalid enabled flag" });
    }

    statusService.setMaintenanceMode(enabled);
    auditLogService.recordAdminAction({
      action: "maintenance.mode.updated",
      role: adminContext.role,
      method: req.method,
      path: req.path,
      ip: req.ip || "unknown",
      metadata: { enabled },
    });
    res.json({
      success: true,
      maintenance: enabled,
      updated_by: adminContext.role,
    });
  },
);

app.post(
  "/api/admin/backfill",
  requireAdminRoles(OPERATIONS_WRITE_ROLES, "admin.backfill.write"),
  (req, res) => {
    const adminContext = getAdminContext(req);
    const scope =
      typeof req.body?.scope === "string" ? req.body.scope.trim() : "all";

    if (!scope) {
      return res.status(400).json({ error: "Invalid backfill scope" });
    }

    const job = adminControlService.queueBackfill(adminContext.role, scope);
    auditLogService.recordAdminAction({
      action: "backfill.job.queued",
      role: adminContext.role,
      method: req.method,
      path: req.path,
      ip: req.ip || "unknown",
      metadata: {
        jobId: job.id,
        scope: job.scope,
      },
    });

    res.status(202).json({
      success: true,
      job,
    });
  },
);

app.post(
  "/api/admin/config/dangerous",
  requireAdminRoles(SUPER_ADMIN_ONLY_ROLES, "admin.config.dangerous.write"),
  (req, res) => {
    const adminContext = getAdminContext(req);
    const { allowEmergencyConfigChanges, maintenanceWindowMinutes } =
      req.body ?? {};

    if (
      typeof allowEmergencyConfigChanges !== "boolean" ||
      !Number.isInteger(maintenanceWindowMinutes) ||
      maintenanceWindowMinutes < 1 ||
      maintenanceWindowMinutes > 1440
    ) {
      return res
        .status(400)
        .json({ error: "Invalid dangerous config payload" });
    }

    const config = adminControlService.updateDangerousConfig(
      adminContext.role,
      {
        allowEmergencyConfigChanges,
        maintenanceWindowMinutes,
      },
    );

    auditLogService.recordAdminAction({
      action: "dangerous.config.updated",
      role: adminContext.role,
      method: req.method,
      path: req.path,
      ip: req.ip || "unknown",
      metadata: {
        allowEmergencyConfigChanges: config.allowEmergencyConfigChanges,
        maintenanceWindowMinutes: config.maintenanceWindowMinutes,
        updatedAt: config.updatedAt,
        updatedBy: config.updatedBy,
      },
    });

    res.json({
      success: true,
      config,
      updated_by: adminContext.role,
    });
  },
);

if (require.main === module) {
  app.listen(port, () => {
    console.log(`Backend server running at http://localhost:${port}`);
  });
}

export default app;
