/**
 * Audit Service
 * Handles recording and retrieval of audit trail entries
 * Tracks all export operations with security context
 */

import { ExportAuditEntry, ExportStatus, ExportDataType, ExportFormat } from '../types/exports';
import crypto from 'crypto';

/**
 * In-memory audit log (for demo purposes)
 * In production, this would use a database
 */
const auditLog: Map<string, ExportAuditEntry> = new Map();

export class AuditService {
  /**
   * Record an export audit entry
   */
  static recordExportAudit(
    userId: string,
    dataType: ExportDataType,
    format: ExportFormat,
    rowCount: number,
    bytesTransferred: number,
    checksum: string,
    startDate?: Date,
    endDate?: Date,
    status: ExportStatus = 'in-progress'
  ): ExportAuditEntry {
    const id = crypto.randomUUID();

    const entry: ExportAuditEntry = {
      id,
      userId,
      dataType,
      format,
      rowCount,
      bytesTransferred,
      startDate,
      endDate,
      checksum,
      status,
      createdAt: new Date(),
      completedAt: status === 'completed' || status === 'failed' ? new Date() : undefined,
    };

    auditLog.set(id, entry);
    return entry;
  }

  /**
   * Update export audit entry status
   */
  static updateExportAuditStatus(
    auditId: string,
    status: ExportStatus,
    rowCount?: number,
    bytesTransferred?: number,
    errorMessage?: string
  ): ExportAuditEntry | null {
    const entry = auditLog.get(auditId);
    if (!entry) return null;

    if (status === 'completed' || status === 'failed') {
      entry.completedAt = new Date();
    }

    entry.status = status;
    if (rowCount !== undefined) entry.rowCount = rowCount;
    if (bytesTransferred !== undefined) entry.bytesTransferred = bytesTransferred;
    if (errorMessage !== undefined) entry.errorMessage = errorMessage;

    auditLog.set(auditId, entry);
    return entry;
  }

  /**
   * Get audit entry by ID
   */
  static getAuditEntry(auditId: string): ExportAuditEntry | null {
    return auditLog.get(auditId) || null;
  }

  /**
   * Get all audit entries for a user
   */
  static getUserExportHistory(userId: string, limit: number = 100): ExportAuditEntry[] {
    return Array.from(auditLog.values())
      .filter((entry) => entry.userId === userId)
      .sort((a, b) => b.createdAt.getTime() - a.createdAt.getTime())
      .slice(0, limit);
  }

  /**
   * Get audit entries for a date range
   */
  static getAuditsByDateRange(startDate: Date, endDate: Date): ExportAuditEntry[] {
    return Array.from(auditLog.values()).filter(
      (entry) => entry.createdAt >= startDate && entry.createdAt <= endDate
    );
  }

  /**
   * Clear all audit logs (for testing)
   */
  static clearAuditLog(): void {
    auditLog.clear();
  }

  /**
   * Get statistics on exports
   */
  static getExportStatistics(userId?: string) {
    const entries = userId
      ? Array.from(auditLog.values()).filter((e) => e.userId === userId)
      : Array.from(auditLog.values());

    return {
      totalExports: entries.length,
      totalBytesExported: entries.reduce((sum, e) => sum + e.bytesTransferred, 0),
      totalRowsExported: entries.reduce((sum, e) => sum + e.rowCount, 0),
      successfulExports: entries.filter((e) => e.status === 'completed').length,
      failedExports: entries.filter((e) => e.status === 'failed').length,
      byDataType: entries.reduce(
        (acc, e) => {
          acc[e.dataType] = (acc[e.dataType] || 0) + 1;
          return acc;
        },
        {} as Record<string, number>
      ),
    };
  }
}

export default AuditService;
