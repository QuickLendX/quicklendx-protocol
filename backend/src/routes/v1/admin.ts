import { Router, Request, Response } from "express";
import { auditService } from "../../services/auditService";
import { apiKeyAuth, AuthenticatedRequest } from "../../middleware/apiKeyAuth";
import { auditMiddleware } from "../../middleware/auditMiddleware";
import {
  AuditOperationSchema,
  AuditEntrySchema,
} from "../../types/audit";

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

export default router;