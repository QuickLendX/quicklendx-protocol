/**
 * Export Service
 * Handles data export operations with validation, streaming, and integrity signing
 */

import crypto from 'crypto';
import { ExportConfig, ExportDataType, ExportFormat, ExportRequest, ValidationResult } from '../types/exports';
import AuditService from './auditService';

/**
 * Default export configuration
 * Configurable to enforce limits on bulk exports
 */
const DEFAULT_CONFIG: ExportConfig = {
  maxRowsPerRequest: 10000,
  maxBytesPerRequest: 50 * 1024 * 1024, // 50 MB
  allowedFormats: ['ndjson', 'json', 'csv'],
  chunkSize: 1000, // Buffer 1000 rows before streaming
};

/**
 * Mock data generator for demonstration
 * In production, these would query actual databases
 */
function generateMockData(
  dataType: ExportDataType,
  rowCount: number,
  startDate?: Date,
  endDate?: Date
): Record<string, unknown>[] {
  const data: Record<string, unknown>[] = [];

  for (let i = 0; i < rowCount; i++) {
    switch (dataType) {
      case 'invoices':
        data.push({
          id: `INV-${i}`,
          amount: Math.floor(Math.random() * 100000),
          currency: 'USD',
          status: 'paid',
          createdAt: new Date(Date.now() - Math.random() * 30 * 24 * 60 * 60 * 1000),
        });
        break;
      case 'bids':
        data.push({
          id: `BID-${i}`,
          invoiceId: `INV-${Math.floor(Math.random() * 1000)}`,
          amount: Math.floor(Math.random() * 100000),
          status: 'accepted',
          createdAt: new Date(Date.now() - Math.random() * 30 * 24 * 60 * 60 * 1000),
        });
        break;
      case 'settlements':
        data.push({
          id: `SETTLE-${i}`,
          amount: Math.floor(Math.random() * 100000),
          status: 'completed',
          createdAt: new Date(Date.now() - Math.random() * 30 * 24 * 60 * 60 * 1000),
        });
        break;
      case 'audit':
        data.push({
          id: `AUDIT-${i}`,
          action: 'export',
          userId: 'user-123',
          resource: dataType,
          timestamp: new Date(Date.now() - Math.random() * 30 * 24 * 60 * 60 * 1000),
        });
        break;
      case 'disputes':
        data.push({
          id: `DISPUTE-${i}`,
          invoiceId: `INV-${Math.floor(Math.random() * 1000)}`,
          status: 'resolved',
          createdAt: new Date(Date.now() - Math.random() * 30 * 24 * 60 * 60 * 1000),
        });
        break;
    }
  }

  return data;
}

export class ExportService {
  private config: ExportConfig;

  constructor(config: Partial<ExportConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Validate export request parameters
   */
  validateExportRequest(
    dataType: ExportDataType,
    format: ExportFormat,
    startDate?: string,
    endDate?: string,
    limit?: number
  ): ValidationResult {
    const errors: string[] = [];

    // Validate data type
    const validDataTypes: ExportDataType[] = ['invoices', 'bids', 'settlements', 'audit', 'disputes'];
    if (!validDataTypes.includes(dataType)) {
      errors.push(`Invalid data type: ${dataType}`);
    }

    // Validate format
    if (!this.config.allowedFormats.includes(format)) {
      errors.push(`Format not allowed: ${format}. Allowed: ${this.config.allowedFormats.join(', ')}`);
    }

    // Validate dates
    if (startDate) {
      const parsedStart = new Date(startDate);
      if (isNaN(parsedStart.getTime())) {
        errors.push(`Invalid startDate format: ${startDate}`);
      }
    }

    if (endDate) {
      const parsedEnd = new Date(endDate);
      if (isNaN(parsedEnd.getTime())) {
        errors.push(`Invalid endDate format: ${endDate}`);
      }
    }

    // Validate date range
    if (startDate && endDate) {
      const start = new Date(startDate);
      const end = new Date(endDate);
      if (start > end) {
        errors.push('startDate must be before endDate');
      }

      // Check date range doesn't exceed 90 days
      const daysDiff = (end.getTime() - start.getTime()) / (1000 * 60 * 60 * 24);
      if (daysDiff > 90) {
        errors.push('Date range cannot exceed 90 days');
      }
    }

    // Validate limit
    if (limit !== undefined) {
      if (limit < 1 || limit > this.config.maxRowsPerRequest) {
        errors.push(
          `Limit must be between 1 and ${this.config.maxRowsPerRequest}, got ${limit}`
        );
      }
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Format data according to export format
   */
  formatData(data: Record<string, unknown>[], format: ExportFormat): string {
    switch (format) {
      case 'ndjson':
        return data.map((row) => JSON.stringify(row)).join('\n');
      case 'json':
        return JSON.stringify(data, null, 2);
      case 'csv':
        if (data.length === 0) return '';
        const headers = Object.keys(data[0]);
        const csvRows = [
          headers.join(','),
          ...data.map((row) =>
            headers
              .map((header) => {
                const value = row[header];
                if (typeof value === 'string' && value.includes(',')) {
                  return `"${value}"`;
                }
                return value;
              })
              .join(',')
          ),
        ];
        return csvRows.join('\n');
      default:
        return JSON.stringify(data);
    }
  }

  /**
   * Calculate checksum for integrity verification
   */
  calculateChecksum(data: string): string {
    return crypto.createHash('sha256').update(data).digest('hex');
  }

  /**
   * Stream export data with back-pressure and integrity signing
   * Yields chunks of data while tracking bytes transferred and row count
   */
  async *streamExport(
    request: ExportRequest
  ): AsyncGenerator<{ data: string; bytesTransferred: number; rowCount: number; checksum: string }> {
    const startTime = Date.now();
    let totalBytesTransferred = 0;
    let totalRowsExported = 0;
    let checksumHash = crypto.createHash('sha256');

    // Record audit entry
    let auditEntry = AuditService.recordExportAudit(
      request.userId,
      request.dataType,
      request.format,
      0,
      0,
      '',
      request.startDate,
      request.endDate,
      'in-progress'
    );

    try {
      // Fetch data (in production, would query database with pagination)
      const mockData = generateMockData(
        request.dataType,
        request.limit,
        request.startDate,
        request.endDate
      );

      // Stream in chunks to avoid buffering large datasets
      for (let i = 0; i < mockData.length; i += this.config.chunkSize) {
        const chunk = mockData.slice(i, i + this.config.chunkSize);
        const formattedChunk = this.formatData(chunk, request.format);

        // Check size limits before streaming
        const chunkSize = Buffer.byteLength(formattedChunk, 'utf8');
        if (totalBytesTransferred + chunkSize > this.config.maxBytesPerRequest) {
          throw new Error(
            `Export size limit exceeded: ${totalBytesTransferred + chunkSize} > ${this.config.maxBytesPerRequest} bytes`
          );
        }

        // Update tracking
        totalBytesTransferred += chunkSize;
        totalRowsExported += chunk.length;

        // Update checksum hash
        checksumHash.update(formattedChunk);

        yield {
          data: formattedChunk,
          bytesTransferred: totalBytesTransferred,
          rowCount: totalRowsExported,
          checksum: checksumHash.digest('hex'),
        };

        // Simulate back-pressure: small delay between chunks
        await new Promise((resolve) => setImmediate(resolve));
      }

      // Update audit with final status
      const finalChecksum = checksumHash.digest('hex');
      auditEntry = AuditService.updateExportAuditStatus(
        auditEntry.id,
        'completed',
        totalRowsExported,
        totalBytesTransferred
      ) || auditEntry;

      // Final yield with complete integrity information
      yield {
        data: '',
        bytesTransferred: totalBytesTransferred,
        rowCount: totalRowsExported,
        checksum: finalChecksum,
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      AuditService.updateExportAuditStatus(auditEntry.id, 'failed', totalRowsExported, totalBytesTransferred, errorMessage);
      throw error;
    }
  }

  /**
   * Export data synchronously (for smaller exports)
   */
  async exportSync(request: ExportRequest): Promise<{
    data: string;
    checksum: string;
    rowCount: number;
    bytesTransferred: number;
  }> {
    const mockData = generateMockData(
      request.dataType,
      request.limit,
      request.startDate,
      request.endDate
    );

    const formattedData = this.formatData(mockData, request.format);
    const checksum = this.calculateChecksum(formattedData);
    const bytesTransferred = Buffer.byteLength(formattedData, 'utf8');

    // Record audit entry
    AuditService.recordExportAudit(
      request.userId,
      request.dataType,
      request.format,
      mockData.length,
      bytesTransferred,
      checksum,
      request.startDate,
      request.endDate,
      'completed'
    );

    return {
      data: formattedData,
      checksum,
      rowCount: mockData.length,
      bytesTransferred,
    };
  }

  /**
   * Get configuration
   */
  getConfig(): ExportConfig {
    return { ...this.config };
  }

  /**
   * Update configuration
   */
  updateConfig(config: Partial<ExportConfig>): void {
    this.config = { ...this.config, ...config };
  }
}

export default ExportService;
