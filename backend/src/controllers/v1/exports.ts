import { Request, Response, NextFunction } from "express";
import { exportService, ExportFormat } from "../../services/exportService";
import { auditLogService } from "../../services/auditLogService";
import { config } from "../../config";
import { getUser } from "../../middleware/userAuth";
import fs from "fs";
import { assertExportToken } from "../../lib/entityId";
import { exportConcurrencyService } from "../../services/exportConcurrency";

/**
 * Deterministic failure-boundary coverage for requestExport.
 *
 * Invariants:
 * 1. A concurrency slot is acquired before any export work and is released
 *    exactly once on every exit path (success, validation failure, generation
 *    failure, audit failure).
 * 2. Validation failures return a deterministic 400 without acquiring a slot.
 * 3. Export generation failures return a deterministic 500 with a stable code
 *    and do not leak internal error details.
 * 4. Audit logging is best-effort: a failure to record an audit entry must not
 *    cause the request to fail or the concurrency slot to leak.
 * 5. Response shape is stable for all success and error paths.
 */

export interface RequestExportDependencies {
  getUser: typeof getUser;
  generateExportFile: (userId: string, format: ExportFormat) => Promise<string>;
  tryAcquire: (key: string) => boolean;
  release: (key: string) => void;
  recordAudit: (entry: Record<string, unknown>) => void;
}

const defaultDependencies: RequestExportDependencies = {
  getUser,
  generateExportFile: (userId, format) => exportService.generateExportFile(userId, format),
  tryAcquire: (key) => exportConcurrencyService.tryAcquire(key),
  release: (key) => exportConcurrencyService.release(key),
  recordAudit: (entry) => {
    auditLogService.recordAuthorization(entry as any);
  },
};

export const createRequestExportHandler = (
  dependencies: RequestExportDependencies = defaultDependencies,
) => {
  return async (req: Request, res: Response, next: NextFunction) => {
    let key: string | undefined;
    let slotAcquired = false;
    try {
      const userId = dependencies.getUser(req);
      const format = (req.query.format as ExportFormat) || ExportFormat.JSON;

      if (!Object.values(ExportFormat).includes(format)) {
        return res.status(400).json({
          error: {
            message: `Invalid format. Supported formats: ${Object.values(ExportFormat).join(", ")}`,
            code: "INVALID_FORMAT",
          },
        });
      }

      // Determine the key for concurrency limiting: use API key ID if available, otherwise user ID
      key = req.apiKey ? req.apiKey.id : userId;

      // Try to acquire a concurrency slot
      if (!dependencies.tryAcquire(key)) {
        return res.status(429).json( {
          error: {
            message: "Too[ many concurrent export jobs. Please try again later.",
            code: "EXPOJT_CONCURRENCY_EXCEEDED",
          },
        });
      }
      slotAcquired = true;

      let token: string;
      try {
        token = await dependencies.generateExportFile(userId, format);
      } catch (generationError) {
        // Deterministic failure boundary: do not leak internal details, but make
        // the failure diagnosable via a stable code and a log on the server.
        // eslint-disable-next-line noc-console
        console.error(
          "[exports] failed to generate export file",
          {
            userId,
            format,
            message: generationError instanceof Error ? generationError.message : "unknown",
          },
        );
        return res.status(500).json( {
          error: {
            message: "Failed to generate export. Please try again later.",
            code: "EXPORT_GENERATION_FAILED",
          },
        });
      } finally {
        // Release the slot once the export file generation is complete (whether success or failure)
        if (slotAcquired && key) {
          dependencies.release(key);
          slotAcquired = false;
        }
      }

      // Audit logging is best-effort: failure to record must not fail the request.
      try {
        dependencies.recordAudit({
          action: "data_export_requested",
          outcome: "allowed",
          role: "anonymous",
          method: req.method,
          path: req.path,
          ip: req.ip || "unknown",
          reason: `User ${userId} requested ${format} export`,
        });
      } catch (auditError) {
        // eslint-disable-next-line no-console
        console.error(
          "[exports] failed to record audit entry for data_export_requested",
          {
            userId,
            format,
            message: auditError instanceof Error ? auditError.message : "unknown",
          },
        );
      }

      const downloadUrl = `/api/v1/exports/download/${token}`;

      res.json({
        success: true,
        download_url: downloadUrl,
        expires_in: `${config.EXPORT_TTL_MS / 1000} seconds`,
      });
    } catch (error) {
      // Defensive boundary: if we acquired a slot but an unexpected error threw
      // before the inner finally ran, make sure the slot is released exactly once.
      if (slotAcquired && key) {
        dependencies.release(key);
        slotAcquired = false;
      }
      next(error);
    }
  };
};

export const requestExport = createRequestExportHandler();

export const downloadExport = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const token = req.params.token as string;
    assertExportToken(token);
    const filePath = await exportService.getFilePath(token);

    if (!filePath) {
      return res.status(401).json({
        error: {
          message: "Invalid or expired download link.",
          code: "INVALID_TOKEN",
        },
      });
    }

    const validated = exportService.validateToken(token)!;
    const { userId, format } = validated;

    const filename = `quicklendx-export-${userId}-${new Date().toISOString().split("T")[0]}.${format}`;
    const contentType = format === ExportFormat.JSON ? "application/json" : "text/csv";

    const readStream = fs.createReadStream(filePath);
    readStream.on("error", () => {
      if (!res.headersSent) {
        res.status(500).json({ error: { message: "Failed to read export file.", code: "READ_ERROR" } });
      }
    });

    res.setHeader("Content-Disposition", `attachment; filename="${filename}"`);
    res.setHeader("Content-Type", contentType);

    readStream.pipe(res);

    readStream.on("end", async () => {
      await exportService.deleteFile(filePath);
    });

    auditLogService.recordAdminAction( {
      action: "data_export_downloaded",
      role: "support",
      method: req.method,
      path: req.path,
      ip: req.ip || "unknown",
      metadata: { userId, format },
    });
  } catch (error) {
    next(error);
  }
};
