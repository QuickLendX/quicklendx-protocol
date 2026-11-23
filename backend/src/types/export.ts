/**
 * export.ts
 *
 * Domain types for the file-export and body-signature subsystem.
 *
 * Invariants:
 * - Every ExportRecord is created in ExportStatus.Pending and only transitions
 *   Pending -> Ready | Failed, or (from Pending/Ready) -> Expired.
 * - `expiresAt` is always strictly greater than `createdAt`.
 * - `signature`/`signatureAlgorithm` are set together, and only when status is Ready.
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
 * Deterministic, machine-readable failure reasons for export creation.
 * Callers must map these to user-visible errors without leaking internals.
 */
export enum ExportFailureReason {
  /** Input validation failed (missing/invalid fields). */
  InvalidInput = "InvalidInput",
  /** Caller is not authorized to request this export. */
  Unauthorized = "Unauthorized",
  /** A duplicate export request was detected for the same idempotency key. */
  DuplicateRequest = "DuplicateRequest",
  /** A concurrent export for the same scope is already in flight. */
  ConcurrentConflict = "ConcurrentConflict",
  /** Underlying storage/serialization failed; safe to retry. */
  TransientFailure = "TransientFailure",
  /** Unrecoverable internal error; do not retry without operator action. */
  InternalError = "InternalError",
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
  /**
   * When status is Failed, the deterministic reason for the failure.
   * Undefined for non-Failed records.
   */
  failureReason?: ExportFailureReason;
  /**
   * Optional human-readable detail for diagnostics. Must never contain
   * secrets, tokens, or raw file contents.
   */
  failureDetail?: string;
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

/** Returned by ExportService.createExport on failure. */
export interface CreateExportFailure {
  reason: ExportFailureReason;
  /** Safe, non-sensitive detail suitable for logs and user-visible errors. */
  detail?: string;
  /** True when the caller may safely retry with the same inputs. */
  retryable: boolean;
}

/** Discriminated union returned by ExportService.createExport. */
export type CreateExportOutcome =
  | { ok: true; result: CreateExportResult }
  | { ok: false; failure: CreateExportFailure };
