/**
 * Export Types and Constants
 * Defines data structures and configuration for secure data exports
 */

/** Export format types */
export type ExportFormat = 'ndjson' | 'json' | 'csv';

/** Export data types */
export type ExportDataType = 'invoices' | 'bids' | 'settlements' | 'audit' | 'disputes';

/** Export status */
export type ExportStatus = 'pending' | 'in-progress' | 'completed' | 'failed';

/** Export query parameters */
export interface ExportQueryParams {
  startDate?: string; // ISO 8601 date string
  endDate?: string; // ISO 8601 date string
  format?: ExportFormat;
  limit?: number;
}

/** Export configuration */
export interface ExportConfig {
  maxRowsPerRequest: number;
  maxBytesPerRequest: number;
  allowedFormats: ExportFormat[];
  chunkSize: number; // Rows to buffer before streaming
}

/** Export request context */
export interface ExportRequest {
  id: string;
  userId: string;
  dataType: ExportDataType;
  format: ExportFormat;
  startDate?: Date;
  endDate?: Date;
  limit: number;
  timestamp: Date;
}

/** Export audit entry */
export interface ExportAuditEntry {
  id: string;
  userId: string;
  dataType: ExportDataType;
  format: ExportFormat;
  rowCount: number;
  bytesTransferred: number;
  startDate?: Date;
  endDate?: Date;
  checksum: string;
  status: ExportStatus;
  createdAt: Date;
  completedAt?: Date;
  errorMessage?: string;
}

/** Streaming export response */
export interface ExportStreamOptions {
  request: ExportRequest;
  onData?: (data: string, size: number) => void;
  onError?: (error: Error) => void;
}

/** Export validation result */
export interface ValidationResult {
  valid: boolean;
  errors: string[];
}
