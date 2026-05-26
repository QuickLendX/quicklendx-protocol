import { Request, Response, NextFunction } from "express";
import { exportService, ExportFormat } from "../../services/exportService";
import { auditLogService } from "../../services/auditLogService";
import { getUser } from "../../middleware/userAuth";

/**
 * Initiates an export request and returns a signed download link.
 */
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

    const token = exportService.generateSignedToken(userId, format);
    
    // Audit log the request
    // We use recordAdminAction as a generic way to log performed actions, 
    // even though this is a user action. In a real system, we might have recordUserAction.
    auditLogService.recordAuthorization({
      action: "data_export_requested",
      outcome: "allowed",
      role: "anonymous", // role refers to AdminRole in this service, so we use anonymous for users
      method: req.method,
      path: req.path,
      ip: req.ip || "unknown",
      reason: `User ${userId} requested ${format} export`,
    });

    const downloadUrl = `/api/v1/exports/download/${token}`;

    res.json({
      success: true,
      download_url: downloadUrl,
      expires_in: "1 hour",
    });
  } catch (error) {
    next(error);
  }
};

/**
 * Validates the signed token and serves the export file.
 */
export const downloadExport = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const token = req.params.token as string;
    const validated = exportService.validateToken(token);

    if (!validated) {
      return res.status(401).json({
        error: {
          message: "Invalid or expired download link.",
          code: "INVALID_TOKEN",
        },
      });
    }

    const { userId, format } = validated;
    const data = await exportService.getUserData(userId);
    const content = exportService.formatData(data, format);

    const filename = `quicklendx-export-${userId}-${new Date().toISOString().split("T")[0]}.${format}`;
    const contentType = format === ExportFormat.JSON ? "application/json" : "text/csv";

    res.setHeader("Content-Disposition", `attachment; filename="${filename}"`);
    res.setHeader("Content-Type", contentType);
    res.send(content);

    // Audit log the actual download
    auditLogService.recordAdminAction({
      action: "data_export_downloaded",
      role: "support", // lowest privileged admin role — used as proxy for user actions
      method: req.method,
      path: req.path,
      ip: req.ip || "unknown",
      metadata: { userId, format },
    });
  } catch (error) {
    next(error);
  }
};
