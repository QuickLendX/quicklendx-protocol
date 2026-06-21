/**
 * exports.ts — controller
 *
 * Handles the export lifecycle:
 *
 *  POST /exports          — creates a new export job for a given data type
 *  GET  /exports/:token/download — serves the file with HMAC signature headers
 *
 * ## Security headers on download
 *
 * Every successful download response carries:
 *
 *   X-Body-Signature           : <hex HMAC-SHA256 of the file bytes>
 *   X-Body-Signature-Algorithm : sha256
 *
 * These headers allow downstream consumers (reverse proxies, client SDKs, CDN
 * edge workers) to verify that the payload hasn't been swapped or corrupted
 * between the application layer and the final recipient.
 *
 * ## Integrity re-verification
 *
 * Before serving the file, the controller re-verifies the live file bytes
 * against the stored signature using `crypto.timingSafeEqual` (via
 * `exportService.verifyFileIntegrity`).  If verification fails the request is
 * rejected with a 500 to prevent a corrupted file being silently served.
 */

import { Request, Response, NextFunction } from "express";
import { exportService, ExportService } from "../../services/exportService";
import { ExportStatus } from "../../types/export";

// ---------------------------------------------------------------------------
// POST /exports  — create a new export
// ---------------------------------------------------------------------------

/**
 * Accepts a JSON body describing what to export and returns a download token.
 *
 * Request body (application/json):
 * ```json
 * { "type": "invoices" | "bids" | "settlements", "format": "json" | "csv" }
 * ```
 *
 * Response (201 Created):
 * ```json
 * { "token": "<hex>", "filename": "invoices.json", "signature": "<hex>",
 *   "signatureAlgorithm": "sha256" }
 * ```
 */
export const createExport = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { type = "invoices", format = "json" } = req.body ?? {};

    const ALLOWED_TYPES = new Set(["invoices", "bids", "settlements"]);
    const ALLOWED_FORMATS = new Set(["json", "csv"]);

    if (!ALLOWED_TYPES.has(type)) {
      res.status(400).json({
        error: { message: `Unsupported export type: ${type}`, code: "INVALID_EXPORT_TYPE" },
      });
      return;
    }
    if (!ALLOWED_FORMATS.has(format)) {
      res.status(400).json({
        error: { message: `Unsupported format: ${format}`, code: "INVALID_EXPORT_FORMAT" },
      });
      return;
    }

    // In a real implementation this would query the indexer/DB.
    // Here we produce deterministic mock payloads for testing.
    const { fileBytes, filename, contentType } = buildMockPayload(type, format);

    const result = await exportService.createExport(fileBytes, filename, contentType);

    res.status(201).json(result);
  } catch (error) {
    next(error);
  }
};

// ---------------------------------------------------------------------------
// GET /exports/:token/download  — serve the signed file
// ---------------------------------------------------------------------------

/**
 * Serves the export file identified by `:token`.
 *
 * Response headers on success (200 OK):
 *   Content-Type              : <mime type>
 *   Content-Disposition       : attachment; filename="<name>"
 *   X-Body-Signature          : <hex HMAC>
 *   X-Body-Signature-Algorithm: sha256
 */
export const downloadExport = async (
  req: Request,
  res: Response,
  next: NextFunction
): Promise<void> => {
  try {
    const { token } = req.params;

    if (!token || typeof token !== "string" || token.trim() === "") {
      res.status(400).json({
        error: { message: "Missing or invalid token", code: "INVALID_TOKEN" },
      });
      return;
    }

    const record = exportService.getExport(token);

    if (!record) {
      res.status(404).json({
        error: { message: "Export not found", code: "EXPORT_NOT_FOUND" },
      });
      return;
    }

    if (record.status === ExportStatus.Expired) {
      res.status(410).json({
        error: { message: "Export has expired", code: "EXPORT_EXPIRED" },
      });
      return;
    }

    if (record.status !== ExportStatus.Ready) {
      res.status(503).json({
        error: { message: "Export is not ready yet", code: "EXPORT_NOT_READY" },
      });
      return;
    }

    // -------------------------------------------------------------------------
    // Integrity re-verification (timing-safe)
    // -------------------------------------------------------------------------
    // Re-compute the HMAC over the live file bytes and compare against the
    // stored signature using `timingSafeEqual` (inside verifyFileIntegrity).
    // This catches any in-memory corruption or substitution that may have
    // occurred after the file was written.
    if (!exportService.verifyFileIntegrity(token, record.fileBuffer)) {
      const err: Error & { status?: number; code?: string } = new Error(
        "File integrity check failed: stored signature does not match live bytes"
      );
      err.status = 500;
      err.code = "INTEGRITY_CHECK_FAILED";
      return next(err);
    }

    // -------------------------------------------------------------------------
    // Serve with signature headers
    // -------------------------------------------------------------------------
    res.setHeader("Content-Type", record.contentType);
    res.setHeader(
      "Content-Disposition",
      `attachment; filename="${record.filename}"`
    );
    res.setHeader("X-Body-Signature", record.signature!);
    res.setHeader("X-Body-Signature-Algorithm", record.signatureAlgorithm!);
    res.setHeader("Content-Length", record.fileBuffer.length.toString());

    res.status(200).send(record.fileBuffer);
  } catch (error) {
    next(error);
  }
};

// ---------------------------------------------------------------------------
// Helper: build mock payload
// ---------------------------------------------------------------------------

function buildMockPayload(
  type: string,
  format: string
): { fileBytes: Buffer; filename: string; contentType: string } {
  const data =
    format === "csv"
      ? buildCsvMock(type)
      : JSON.stringify(buildJsonMock(type), null, 2);

  const ext = format === "csv" ? "csv" : "json";
  const contentType =
    format === "csv" ? "text/csv; charset=utf-8" : "application/json; charset=utf-8";

  return {
    fileBytes: Buffer.from(data, "utf-8"),
    filename: `${type}.${ext}`,
    contentType,
  };
}

function buildJsonMock(type: string): unknown {
  const mocks: Record<string, unknown> = {
    invoices: [
      {
        id: "0x1234",
        business: "GDVLRH4G4...7Y",
        amount: "1000000000",
        status: "Verified",
      },
    ],
    bids: [
      {
        bid_id: "0xabcd",
        invoice_id: "0x1234",
        investor: "GA...ABC",
        bid_amount: "950000000",
        status: "Placed",
      },
    ],
    settlements: [
      {
        id: "0xsettle123",
        invoice_id: "0x1234",
        amount: "1000000000",
        status: "Paid",
      },
    ],
  };
  return mocks[type] ?? [];
}

function buildCsvMock(type: string): string {
  const rows: Record<string, string> = {
    invoices: "id,business,amount,status\n0x1234,GDVLRH4G4...7Y,1000000000,Verified",
    bids: "bid_id,invoice_id,bid_amount,status\n0xabcd,0x1234,950000000,Placed",
    settlements: "id,invoice_id,amount,status\n0xsettle123,0x1234,1000000000,Paid",
  };
  return rows[type] ?? "";
}

// ---------------------------------------------------------------------------
// Re-export the service instance so tests can inject a fresh one if needed
// ---------------------------------------------------------------------------
export { ExportService };
