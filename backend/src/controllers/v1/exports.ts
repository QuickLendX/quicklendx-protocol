import { Request, Response, NextFunction } from "express";
import { exportService, ExportFormat, ExportErrorCode, ExportError } from "../../services/exportService";
import { auditLogService } from "../../services/auditLogService";
import { config } from "../../config";
import { getUser } from "../../middleware/userAuth";
import * as fs from "fs";
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
            message: "Too many concurrent export jobs. Please try again later.",
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

/**
 * Deterministic failure-boundary coverage for downloadExport.
 *
 * Invariants:
 * 1. Token validation distinguishes INVALID vs EXPIRED with stable codes.
 * 2. File access failures return NOT_FOUND without leaking path details.
 * 3. Stream errors after headers sent are logged but do not throw.
 * 4. File deletion is best-effort: failure to delete does not fail the response.
 * 5. Audit logging is best-effort: failure to record must not fail the request.
 * 6. Concurrency slot for download is acquired and released exactly once per attempt.
 * 7. Response shape is stable for all success and error paths.
 * 8. Retries with the same token are idempotent (single-use semantics).
 */
export interface DownloadExportDependencies {
  getUser: typeof getUser;
  validateTokenStrict: (token: string) => { userId: string; format: ExportFormat; expiresAt: number };
  getFilePathStrict: (token: string) => Promise<string>;
  deleteFile: (filePath: string) => Promise<void>;
  tryAcquire: (key: string) => boolean;
  release: (key: string) => void;
  recordAudit: (entry: Record<string, unknown>) => void;
  createReadStream: (filePath: string) => fs.ReadStream;
}

const defaultDownloadDependencies: DownloadExportDependencies = {
  getUser,
  validateTokenStrict: (token) => exportService.validateTokenStrict(token),
  getFilePathStrict: (token) => exportService.getFilePathStrict(token),
  deleteFile: (filePath) => exportService.deleteFile(filePath),
  tryAcquire: (key) => exportConcurrencyService.tryAcquire(key),
  release: (key) => exportConcurrencyService.release(key),
  recordAudit: (entry) => {
    auditLogService.recordAdminAction(entry as any);
  },
  createReadStream: (filePath) => fs.createReadStream(filePath),
};

export const createDownloadExportHandler = (
  dependencies: DownloadExportDependencies = defaultDownloadDependencies,
) => {
  return async (req: Request, res: Response, next: NextFunction) => {
    let key: string | undefined;
    let slotAcquired = false;
    let filePath: string | undefined;
    let validated: { userId: string; format: ExportFormat; expiresAt: number } | undefined;
    let headersSent = false;

    try {
      const token = req.params.token as string;

      // Deterministic boundary: assert token format early with a stable code.
      // This catches malformed tokens before any I/O or concurrency slot.
      try {
        assertExportToken(token);
      } catch {
        return res.status(400).json({
          error: {
            message: "Export token is malformed",
            code: ExportErrorCode.TOKEN_INVALID,
          },
        });
      }

      // Determine the key for concurrency limiting: use API key ID if available, otherwise user ID from token
      // We need to decode the token first to get the userId for the concurrency key
      let decodedUserId: string;
      try {
        const decoded = JSON.parse(Buffer.from(token, "base64").toString("utf8"));
        const payload = JSON.parse(decoded.payload);
        decodedUserId = payload.userId;
      } catch {
        decodedUserId = "unknown";
      }

      key = req.apiKey ? req.apiKey.id : decodedUserId;

      // Try to acquire a concurrency slot for the download
      if (!dependencies.tryAcquire(key)) {
        return res.status(429).json({
          error: {
            message: "Too many concurrent downloads. Please try again later.",
            code: "DOWNLOAD_CONCURRENCY_EXCEEDED",
          },
        });
      }
      slotAcquired = true;

      // Validate token strictly: distinguishes invalid vs expired with stable codes
      try {
        validated = await dependencies.validateTokenStrict(token);
      } catch (err) {
        if (err instanceof ExportError) {
          // Release slot before returning deterministic error
          if (slotAcquired && key) {
            dependencies.release(key);
            slotAcquired = false;
          }
          const statusCode = err.code === ExportErrorCode.TOKEN_EXPIRED ? 401 : 400;
          return res.status(statusCode).json({
            error: {
              message: err.message,
              code: err.code,
            },
          });
        }
        // Unexpected error: log and return generic failure
        // eslint-disable-next-line no-console
        console.error("[exports] unexpected token validation error", {
          token: token.slice(0, 8) + "...",
          message: err instanceof Error ? err.message : "unknown",
        });
        if (slotAcquired && key) {
          dependencies.release(key);
          slotAcquired = false;
        }
        return res.status(500).json({
          error: {
            message: "Failed to validate export token",
            code: ExportErrorCode.INTERNAL,
          },
        });
      }

      // Get file path with strict validation (checks file exists and is readable)
      try {
        filePath = await dependencies.getFilePathStrict(token);
      } catch (err) {
        if (err instanceof ExportError) {
          // Release slot before returning deterministic error
          if (slotAcquired && key) {
            dependencies.release(key);
            slotAcquired = false;
          }
          return res.status(404).json({
            error: {
              message: err.message,
              code: err.code,
            },
          });
        }
        // Unexpected error
        // eslint-disable-next-line no-console
        console.error("[exports] unexpected file access error", {
          userId: validated?.userId,
          message: err instanceof Error ? err.message : "unknown",
        });
        if (slotAcquired && key) {
          dependencies.release(key);
          slotAcquired = false;
        }
        return res.status(500).json({
          error: {
            message: "Failed to access export file",
            code: ExportErrorCode.STORAGE_FAILURE,
          },
        });
      }

      const { userId, format } = validated;
      const filename = `quicklendx-export-${userId}-${new Date().toISOString().split("T")[0]}.${format}`;
      const contentType = format === ExportFormat.JSON ? "application/json" : "text/csv";

      // Create read stream
      const readStream = dependencies.createReadStream(filePath);

      // Track stream state for deterministic cleanup
      let streamEnded = false;
      let streamErrored = false;

      // Handle stream errors
      readStream.on("error", (streamErr) => {
        streamErrored = true;
        // eslint-disable-next-line no-console
        console.error("[exports] failed to read export file stream", {
          userId,
          format,
          filePath,
          message: streamErr.message,
        });

        // Release concurrency slot on stream error
        if (slotAcquired && key) {
          dependencies.release(key);
          slotAcquired = false;
        }

        // If headers not sent, send error response
        if (!headersSent && !res.headersSent) {
          res.status(500).json({
            error: {
              message: "Failed to read export file",
              code: ExportErrorCode.STORAGE_FAILURE,
            },
          });
          headersSent = true;
        }
        // If headers already sent, we cannot send a response; the connection will be closed
      });

      // Set headers before piping
      res.setHeader("Content-Disposition", `attachment; filename="${filename}"`);
      res.setHeader("Content-Type", contentType);
      headersSent = true;

      // Pipe the file to response
      readStream.pipe(res);

      // Handle stream end - file deletion and audit logging
      readStream.on("end", async () => {
        streamEnded = true;

        // Release concurrency slot
        if (slotAcquired && key) {
          dependencies.release(key);
          slotAcquired = false;
        }

        // Best-effort file deletion (single-use semantics)
        if (filePath) {
          try {
            await dependencies.deleteFile(filePath);
          } catch (deleteErr) {
            // eslint-disable-next-line no-console
            console.error("[exports] failed to delete export file after download", {
              userId,
              filePath,
              message: deleteErr instanceof Error ? deleteErr.message : "unknown",
            });
            // Do not fail the response - deletion is best-effort
          }
        }

        // Best-effort audit logging
        try {
          dependencies.recordAudit({
            action: "data_export_downloaded",
            outcome: "performed",
            role: "support",
            method: req.method,
            path: req.path,
            ip: req.ip || "unknown",
            metadata: { userId, format },
          });
        } catch (auditErr) {
          // eslint-disable-next-line no-console
          console.error("[exports] failed to record audit entry for data_export_downloaded", {
            userId,
            format,
            message: auditErr instanceof Error ? auditErr.message : "unknown",
          });
        }
      });

      // Handle response close (client disconnected) - cleanup
      req.on("close", async () => {
        if (!streamEnded && !streamErrored) {
          // Client disconnected mid-stream
          if (slotAcquired && key) {
            dependencies.release(key);
            slotAcquired = false;
          }

          // Still attempt file deletion for single-use semantics
          if (filePath) {
            try {
              await dependencies.deleteFile(filePath);
            } catch {
              // Best-effort
            }
          }
        }
      });

    } catch (error) {
      // Defensive boundary: if we acquired a slot but an unexpected error threw
      // before the inner handlers ran, make sure the slot is released exactly once.
      if (slotAcquired && key) {
        dependencies.release(key);
        slotAcquired = false;
      }
      next(error);
    }
  };
};

export const downloadExport = createDownloadExportHandler();
