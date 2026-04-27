import express from "express";
import cors from "cors";
import dotenv from "dotenv";
import { getAdminContext, requireAdminRoles } from "./middleware/rbac";
import { adminControlService } from "./services/adminControlService";
import { auditLogService } from "./services/auditLogService";
import { statusService } from "./services/statusService";
import { apiKeyAuth, AuthenticatedRequest } from "./middleware/apiKeyAuth";
import { auditMiddleware } from "./middleware/auditMiddleware";
import { auditService } from "./services/auditService";
import { AuditOperationSchema } from "./types/audit";
import { requireAdminAuth, getAdminActor } from "./middleware/adminAuth";
import { backfillService, BackfillError } from "./services/backfillService";
import { BackfillActionSchema, BackfillStartRequestSchema } from "./types/backfill";
import { replayService, ReplayError } from "./services/replayService";
import { ReplayActionSchema, ReplayStartRequestSchema } from "./types/replay";
import { DefaultEventValidator } from "./services/eventValidator";
import { InMemoryRawEventStore } from "./services/rawEventStore";
import { InMemoryDerivedTableStore } from "./services/derivedTableStore";

dotenv.config();

function createApp(): express.Express {
  const app = express();
  const port = process.env.PORT || 3001;

  app.set("trust proxy", true);
  app.use(cors());
  app.use(express.json());

  app.use("/api/v1/admin", apiKeyAuth, auditMiddleware);
  app.use("/api/v1/admin/monitoring", monitoringRoutes);

  app.post("/api/v1/admin/maintenance", (req: AuthenticatedRequest, res) => {
    const { enabled } = req.body;
    if (typeof enabled !== "boolean") {
      return res.status(400).json({ error: "Invalid enabled flag" });
    }
    statusService.setMaintenanceMode(enabled);
    res.json({ success: true, maintenance: enabled });

    app.set("trust proxy", true);
app.use(helmet());
app.use(rateLimitMiddleware);
app.use(requestLimitsMiddleware);

app.get("/api/status", async (req, res) => {
  try {
    const status = await statusService.getStatus();
    res.setHeader("Cache-Control", "public, max-age=30");
    res.json(status);
  } catch (error) {
    console.error("Status check failed:", error);
    res.status(500).json({ error: "Internal server error" });
  }
});

app.post("/api/admin/maintenance", requireAdminAuth, (req, res) => {
  const { enabled } = req.body;
  if (typeof enabled !== "boolean") {
    return res.status(400).json({ error: "Invalid enabled flag" });
  }
  statusService.setMaintenanceMode(enabled);
  res.json({ success: true, maintenance: enabled });
});

app.post("/api/admin/backfill", requireAdminAuth, async (req, res) => {
  try {
    const payload = BackfillStartRequestSchema.parse(req.body);
    const actor = getAdminActor(req);
    const result = await backfillService.startBackfill(payload, actor);
    res.status(payload.dryRun ? 200 : 202).json(result);
  } catch (error) {
    if (error instanceof BackfillError) {
      return res.status(error.statusCode).json({ error: error.message, code: error.code });
    }
    return res.status(400).json({ error: "Invalid request payload", code: "VALIDATION_ERROR" });
  }
});

app.get("/api/admin/backfill/runs", requireAdminAuth, (req, res) => {
  res.json({ runs: backfillService.listRuns() });
});

app.get("/api/admin/backfill/:runId", requireAdminAuth, (req, res) => {
  const runId = Array.isArray(req.params.runId) ? req.params.runId[0] : req.params.runId;
  const run = backfillService.getRun(runId);
  if (!run) {
    return res.status(404).json({ error: "Backfill run not found", code: "RUN_NOT_FOUND" });
  }
  res.json({ run });
});

app.post("/api/admin/backfill/pause", requireAdminAuth, async (req, res) => {
  try {
    const { runId } = BackfillActionSchema.parse(req.body);
    const run = await backfillService.pauseRun(runId, getAdminActor(req));
    res.json({ run });
  } catch (error) {
    if (error instanceof BackfillError) {
      return res.status(error.statusCode).json({ error: error.message, code: error.code });
    }
    return res.status(400).json({ error: "Invalid request payload", code: "VALIDATION_ERROR" });
  }
});

app.post("/api/admin/backfill/resume", requireAdminAuth, async (req, res) => {
  try {
    const { runId } = BackfillActionSchema.parse(req.body);
    const run = await backfillService.resumeRun(runId, getAdminActor(req));
    res.json({ run });
  } catch (error) {
    if (error instanceof BackfillError) {
      return res.status(error.statusCode).json({ error: error.message, code: error.code });
    }
    return res.status(400).json({ error: "Invalid request payload", code: "VALIDATION_ERROR" });
  }
});

// Replay endpoints
app.post("/api/admin/replay", requireAdminAuth, async (req, res) => {
  try {
    const payload = ReplayStartRequestSchema.parse(req.body);
    const actor = getAdminActor(req);
    const result = await replayService.startReplay(payload, actor);
    res.status(payload.dryRun ? 200 : 202).json(result);
  } catch (error) {
    if (error instanceof ReplayError) {
      return res.status(error.statusCode).json({ error: error.message, code: error.code });
    }
    return res.status(400).json({ error: "Invalid request payload", code: "VALIDATION_ERROR" });
  }
});

app.get("/api/admin/replay/runs", requireAdminAuth, (req, res) => {
  res.json({ runs: replayService.listRuns() });
});

app.get("/api/admin/replay/:runId", requireAdminAuth, (req, res) => {
  const runId = Array.isArray(req.params.runId) ? req.params.runId[0] : req.params.runId;
  const run = replayService.getRun(runId);
  if (!run) {
    return res.status(404).json({ error: "Replay run not found", code: "RUN_NOT_FOUND" });
  }
  res.json({ run });
});

app.get("/api/admin/replay/:runId/stats", requireAdminAuth, async (req, res) => {
  try {
    const runId = Array.isArray(req.params.runId) ? req.params.runId[0] : req.params.runId;
    const stats = await replayService.getStats(runId);
    res.json({ stats });
  } catch (error) {
    if (error instanceof ReplayError) {
      return res.status(error.statusCode).json({ error: error.message, code: error.code });
    }
    return res.status(500).json({ error: "Internal server error", code: "INTERNAL_ERROR" });
  }
});

app.post("/api/admin/replay/pause", requireAdminAuth, async (req, res) => {
  try {
    const { runId } = ReplayActionSchema.parse(req.body);
    const run = await replayService.pauseRun(runId, getAdminActor(req));
    res.json({ run });
  } catch (error) {
    if (error instanceof ReplayError) {
      return res.status(error.statusCode).json({ error: error.message, code: error.code });
    }
    return res.status(400).json({ error: "Invalid request payload", code: "VALIDATION_ERROR" });
  }
});

app.post("/api/admin/replay/resume", requireAdminAuth, async (req, res) => {
  try {
    const { runId } = ReplayActionSchema.parse(req.body);
    const run = await replayService.resumeRun(runId, getAdminActor(req));
    res.json({ run });
  } catch (error) {
    if (error instanceof ReplayError) {
      return res.status(error.statusCode).json({ error: error.message, code: error.code });
    }
    return res.status(400).json({ error: "Invalid request payload", code: "VALIDATION_ERROR" });
  }
});

if (require.main === module) {
  app.listen(port, () => {
    console.log(`Backend server running at http://localhost:${port}`);
  });

  if (require.main === module) {
    app.listen(port, () => {
      console.log(`Backend server running at http://localhost:${port}`);
    });

    app.post("/api/admin/maintenance", (req, res) => {
      const { enabled } = req.body;
      if (typeof enabled !== "boolean") {
        return res.status(400).json({ error: "Invalid enabled flag" });
      }
      statusService.setMaintenanceMode(enabled);
      res.json({ success: true, maintenance: enabled });
    });

    app.get("/api/v1/admin/audit", (req: AuthenticatedRequest, res) => {
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
        res.status(400).json({ error: { message, code: "INVALID_AUDIT_QUERY" } });
      }
    });

    app.get("/api/v1/admin/audit/operations", (req: AuthenticatedRequest, res) => {
      const operations = AuditOperationSchema.options;
      res.json({ operations });
    });

    app.get("/api/v1/admin/audit/export", (req: AuthenticatedRequest, res) => {
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
        res.status(400).json({ error: { message, code: "AUDIT_EXPORT_FAILED" } });
      }
    });

    app.get("/api/status", async (req, res) => {
      try {
        const status = await statusService.getStatus();
        res.setHeader("Cache-Control", "public, max-age=30");
        res.json(status);
      } catch (error) {
        console.error("Status check failed:", error);
        res.status(500).json({ error: "Internal server error" });
      }
    });
  }

  if (require.main === module) {
    app.listen(port, () => {
      console.log(`Backend server running at http://localhost:${port}`);
    });
  }

  return app;
}

const app = createApp();
export { createApp };
export default app;