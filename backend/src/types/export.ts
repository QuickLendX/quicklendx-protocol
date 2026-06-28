/**
 * export.ts
 *
 * Domain types for the file-export and body-signature subsystem.
 */

// ---------------------------------------------------------------------------
// Export token & metadata
// ---------------------------------------------------------------------------

export enum ExportStatus {
  Pending = "Pending",
  Ready = "Ready",
  Failed = "Failed",
  Expired = "Expired",
}

/**
 * Represents one export job.  The `token` is an opaque, randomly-generated
 * identifier handed to the client; it is the only credential needed to
 * download the file.
 */
export interface ExportRecord {
  /** Opaque download token (hex string, 32 bytes). */
  token: string;
  /** Content of the file as a Buffer (held in memory for this implementation). */
  fileBuffer: Buffer;
  /** Original filename suggested for Content-Disposition. */
  filename: string;
  /** MIME type of the exported file. */
  contentType: string;
  /** Current lifecycle state of this export. */
  status: ExportStatus;
  /** UTC epoch-ms when this export was created. */
  createdAt: number;
  /** UTC epoch-ms after which this record is considered expired. */
  expiresAt: number;
  /**
   * Hex-encoded HMAC digest computed while streaming/writing the file bytes.
   * Undefined until the file has been written and the HMAC finalised.
   */
  signature?: string;
  /** The HMAC algorithm used (e.g. "sha256"). */
  signatureAlgorithm?: string;
}

// ---------------------------------------------------------------------------
// Service result shapes
// ---------------------------------------------------------------------------

/** Returned by ExportService.createExport on success. */
export interface CreateExportResult {
  token: string;
  filename: string;
  signature: string;
  signatureAlgorithm: string;
}
