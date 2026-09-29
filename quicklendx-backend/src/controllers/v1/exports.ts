/**
 * Export Controller
 * Handles HTTP requests for data exports with validation and streaming
 */

// test

import { ExportService } from '../services/exportService';
import { ExportDataType, ExportFormat, ExportRequest } from '../types/exports';
import crypto from 'crypto';

/**
 * Represents an HTTP request (simplified for testing)
 */
interface HttpRequest {
  userId: string;
  query?: Record<string, string | string[]>;
  params?: Record<string, string>;
}

/**
 * Represents an HTTP response
 */
interface HttpResponse {
  statusCode: number;
  headers: Record<string, string>;
  body: string | AsyncGenerator<string>;
}

export class ExportController {
  private exportService: ExportService;

  constructor(exportService?: ExportService) {
    this.exportService = exportService || new ExportService();
  }

  /**
   * Extract and validate query parameters
   */
  private parseQueryParams(query?: Record<string, string | string[]>) {
    const params = {
      startDate: undefined as string | undefined,
      endDate: undefined as string | undefined,
      format: 'ndjson' as ExportFormat,
      limit: 10000 as number,
    };

    if (!query) return params;

    if (query.startDate && typeof query.startDate === 'string') {
      params.startDate = query.startDate;
    }

    if (query.endDate && typeof query.endDate === 'string') {
      params.endDate = query.endDate;
    }

    if (query.format && typeof query.format === 'string') {
      params.format = query.format as ExportFormat;
    }

    if (query.limit && typeof query.limit === 'string') {
      const parsed = parseInt(query.limit, 10);
      if (!isNaN(parsed)) {
        params.limit = parsed;
      }
    }

    return params;
  }

  /**
   * Export invoices endpoint
   */
  async exportInvoices(req: HttpRequest): Promise<HttpResponse> {
    const params = this.parseQueryParams(req.query);

    // Validate parameters
    const validation = this.exportService.validateExportRequest(
      'invoices',
      params.format,
      params.startDate,
      params.endDate,
      params.limit
    );

    if (!validation.valid) {
      return {
        statusCode: 400,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          error: 'Invalid export parameters',
          details: validation.errors,
        }),
      };
    }

    return this.handleExportStream(req, 'invoices', params);
  }

  /**
   * Export bids endpoint
   */
  async exportBids(req: HttpRequest): Promise<HttpResponse> {
    const params = this.parseQueryParams(req.query);

    const validation = this.exportService.validateExportRequest(
      'bids',
      params.format,
      params.startDate,
      params.endDate,
      params.limit
    );

    if (!validation.valid) {
      return {
        statusCode: 400,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          error: 'Invalid export parameters',
          details: validation.errors,
        }),
      };
    }

    return this.handleExportStream(req, 'bids', params);
  }

  /**
   * Export settlements endpoint
   */
  async exportSettlements(req: HttpRequest): Promise<HttpResponse> {
    const params = this.parseQueryParams(req.query);

    const validation = this.exportService.validateExportRequest(
      'settlements',
      params.format,
      params.startDate,
      params.endDate,
      params.limit
    );

    if (!validation.valid) {
      return {
        statusCode: 400,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          error: 'Invalid export parameters',
          details: validation.errors,
        }),
      };
    }

    return this.handleExportStream(req, 'settlements', params);
  }

  /**
   * Export disputes endpoint
   */
  async exportDisputes(req: HttpRequest): Promise<HttpResponse> {
    const params = this.parseQueryParams(req.query);

    const validation = this.exportService.validateExportRequest(
      'disputes',
      params.format,
      params.startDate,
      params.endDate,
      params.limit
    );

    if (!validation.valid) {
      return {
        statusCode: 400,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          error: 'Invalid export parameters',
          details: validation.errors,
        }),
      };
    }

    return this.handleExportStream(req, 'disputes', params);
  }

  /**
   * Export audit log endpoint (admin-only)
   */
  async exportAuditLog(req: HttpRequest): Promise<HttpResponse> {
    // In production, verify admin role
    const params = this.parseQueryParams(req.query);

    const validation = this.exportService.validateExportRequest(
      'audit',
      params.format,
      params.startDate,
      params.endDate,
      params.limit
    );

    if (!validation.valid) {
      return {
        statusCode: 400,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          error: 'Invalid export parameters',
          details: validation.errors,
        }),
      };
    }

    return this.handleExportStream(req, 'audit', params);
  }

  /**
   * Common export stream handler with integrity digest
   */
  private async handleExportStream(
    req: HttpRequest,
    dataType: ExportDataType,
    params: { startDate?: string; endDate?: string; format: ExportFormat; limit: number }
  ): Promise<HttpResponse> {
    try {
      const exportRequest: ExportRequest = {
        id: crypto.randomUUID(),
        userId: req.userId,
        dataType,
        format: params.format,
        startDate: params.startDate ? new Date(params.startDate) : undefined,
        endDate: params.endDate ? new Date(params.endDate) : undefined,
        limit: params.limit,
        timestamp: new Date(),
      };

      // Determine content type
      const contentType =
        params.format === 'csv' ? 'text/csv' : 'application/x-ndjson'; // Default to NDJSON

      // Stream the export
      const generator = this.exportService.streamExport(exportRequest);

      return {
        statusCode: 200,
        headers: {
          'Content-Type': contentType,
          'Content-Disposition': `attachment; filename="export-${dataType}-${Date.now()}.${params.format}"`,
          'X-Export-Id': exportRequest.id,
          'X-Content-Digest': 'sha256', // Will be updated with actual checksum
        },
        body: this.wrapStream(generator),
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      return {
        statusCode: 500,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          error: 'Export failed',
          message: errorMessage,
        }),
      };
    }
  }

  /**
   * Wrap export generator to include final checksum header
   */
  private async *wrapStream(
    generator: AsyncGenerator<{ data: string; bytesTransferred: number; rowCount: number; checksum: string }>
  ) {
    let lastChecksum = '';

    for await (const chunk of generator) {
      lastChecksum = chunk.checksum;

      if (chunk.data) {
        yield chunk.data;
      }
    }

    // In production, you'd return the final checksum in a trailer header
    // For now, yield it as a comment in the stream for verification
    if (lastChecksum) {
      yield `\n# Content-Digest: sha256=${lastChecksum}\n`;
    }
  }

  /**
   * Get export statistics endpoint (admin-only)
   */
  async getExportStats(req: HttpRequest): Promise<HttpResponse> {
    try {
      // In production, verify admin role
      // For now, return stats for current user
      return {
        statusCode: 200,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          message: 'Export statistics endpoint',
          note: 'Integrate with AuditService.getExportStatistics(userId)',
        }),
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      return {
        statusCode: 500,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          error: 'Failed to retrieve statistics',
          message: errorMessage,
        }),
      };
    }
  }
}

export default ExportController;
