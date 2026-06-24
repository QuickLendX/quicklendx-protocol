import { Request, Response, NextFunction } from "express";
import { exportService, ExportFormat } from "../../services/exportService";
import { auditLogService } from "../../services/auditLogService";
import { config } from "../../config";
import { getUser } from "../../middleware/userAuth";
import fs from "fs";
import { assertExportToken } from "../../lib/entityId";

export const requestExport = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const userId = getUser(req);
    const format = (req.query.format as ExportFormat) || ExportFormat.JSON;

    if (!Object.values(ExportFormat).includes(format)) {
      return res.status(400).json({
        error: {
          message: `Invalid format. Supported formats: ${Object.values(ExportFormat).join(", ")}`,
          code: "INVALID_FORMAT",
        },
      });
    }

    const token = await exportService.generateExportFile(userId, format);

    auditLogService.recordAuthorization({
      action: "data_export_requested",
      outcome: "allowed",
      role: "anonymous",
      method: req.method,
      path: req.path,
      ip: req.ip || "unknown",
      reason: `User ${userId} requested ${format} export`,
    });

    const downloadUrl = `/api/v1/exports/download/${token}`;

    res.json({
      success: true,
      download_url: downloadUrl,
      expires_in: `${config.EXPORT_TTL_MS / 1000} seconds`,
    });
  } catch (error) {
    next(error);
  }
};

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

    auditLogService.recordAdminAction({
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
