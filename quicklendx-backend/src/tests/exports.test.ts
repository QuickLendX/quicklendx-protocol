/**
 * Export Tests
 * Comprehensive test suite for data export functionality with validation, audit, and integrity
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import ExportService from '../services/exportService';
import AuditService from '../services/auditService';
import ExportController from '../controllers/v1/exports';
import AdminRouter from '../routes/v1/admin';
import { ExportRequest } from '../types/exports';

describe('ExportService', () => {
  let service: ExportService;

  beforeEach(() => {
    service = new ExportService();
    AuditService.clearAuditLog();
  });

  describe('validateExportRequest', () => {
    it('should accept valid parameters', () => {
      const result = service.validateExportRequest(
        'invoices',
        'ndjson',
        '2024-01-01',
        '2024-01-31',
        5000
      );
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should reject invalid data type', () => {
      const result = service.validateExportRequest(
        'invalid' as any,
        'ndjson',
        '2024-01-01',
        '2024-01-31'
      );
      expect(result.valid).toBe(false);
      expect(result.errors).toContain(expect.stringMatching(/Invalid data type/));
    });

    it('should reject invalid format', () => {
      const result = service.validateExportRequest(
        'invoices',
        'xml' as any,
        '2024-01-01',
        '2024-01-31'
      );
      expect(result.valid).toBe(false);
      expect(result.errors).toContain(expect.stringMatching(/Format not allowed/));
    });

    it('should reject invalid date format', () => {
      const result = service.validateExportRequest('invoices', 'ndjson', 'invalid-date');
      expect(result.valid).toBe(false);
      expect(result.errors).toContain(expect.stringMatching(/Invalid startDate format/));
    });

    it('should reject startDate after endDate', () => {
      const result = service.validateExportRequest(
        'invoices',
        'ndjson',
        '2024-12-31',
        '2024-01-01'
      );
      expect(result.valid).toBe(false);
      expect(result.errors).toContain(expect.stringMatching(/startDate must be before endDate/));
    });

    it('should reject date range exceeding 90 days', () => {
      const result = service.validateExportRequest(
        'invoices',
        'ndjson',
        '2024-01-01',
        '2024-05-01'
      );
      expect(result.valid).toBe(false);
      expect(result.errors).toContain(expect.stringMatching(/Date range cannot exceed 90 days/));
    });

    it('should reject limit exceeding maxRowsPerRequest', () => {
      const result = service.validateExportRequest(
        'invoices',
        'ndjson',
        undefined,
        undefined,
        50000
      );
      expect(result.valid).toBe(false);
      expect(result.errors).toContain(expect.stringMatching(/Limit must be between/));
    });

    it('should reject negative limit', () => {
      const result = service.validateExportRequest(
        'invoices',
        'ndjson',
        undefined,
        undefined,
        -100
      );
      expect(result.valid).toBe(false);
      expect(result.errors).toContain(expect.stringMatching(/Limit must be between/));
    });

    it('should accept all valid data types', () => {
      const types = ['invoices', 'bids', 'settlements', 'audit', 'disputes'] as const;
      for (const type of types) {
        const result = service.validateExportRequest(type, 'ndjson');
        expect(result.valid).toBe(true);
      }
    });
  });

  describe('formatData', () => {
    const mockData = [
      { id: '1', amount: 1000, status: 'paid' },
      { id: '2', amount: 2000, status: 'pending' },
    ];

    it('should format data as NDJSON', () => {
      const formatted = service.formatData(mockData, 'ndjson');
      const lines = formatted.split('\n');
      expect(lines).toHaveLength(2);
      expect(JSON.parse(lines[0])).toEqual(mockData[0]);
      expect(JSON.parse(lines[1])).toEqual(mockData[1]);
    });

    it('should format data as JSON', () => {
      const formatted = service.formatData(mockData, 'json');
      expect(JSON.parse(formatted)).toEqual(mockData);
    });

    it('should format data as CSV', () => {
      const formatted = service.formatData(mockData, 'csv');
      const lines = formatted.split('\n');
      expect(lines[0]).toContain('id');
      expect(lines[0]).toContain('amount');
      expect(lines[0]).toContain('status');
    });

    it('should handle empty data', () => {
      const formatted = service.formatData([], 'json');
      expect(JSON.parse(formatted)).toEqual([]);
    });

    it('should escape CSV values with commas', () => {
      const data = [{ id: '1', description: 'Has, comma' }];
      const formatted = service.formatData(data, 'csv');
      expect(formatted).toContain('"Has, comma"');
    });
  });

  describe('calculateChecksum', () => {
    it('should calculate consistent SHA256 checksum', () => {
      const data = JSON.stringify({ test: 'data' });
      const checksum1 = service.calculateChecksum(data);
      const checksum2 = service.calculateChecksum(data);
      expect(checksum1).toBe(checksum2);
      expect(checksum1).toMatch(/^[a-f0-9]{64}$/);
    });

    it('should produce different checksums for different data', () => {
      const checksum1 = service.calculateChecksum('data1');
      const checksum2 = service.calculateChecksum('data2');
      expect(checksum1).not.toBe(checksum2);
    });
  });

  describe('streamExport', () => {
    it('should stream data without buffering in memory', async () => {
      const request: ExportRequest = {
        id: 'test-1',
        userId: 'user-123',
        dataType: 'invoices',
        format: 'ndjson',
        limit: 100,
        timestamp: new Date(),
      };

      const chunks: string[] = [];
      let totalBytes = 0;
      let totalRows = 0;

      for await (const chunk of service.streamExport(request)) {
        if (chunk.data) {
          chunks.push(chunk.data);
          totalBytes = chunk.bytesTransferred;
          totalRows = chunk.rowCount;
        }
      }

      expect(chunks.length).toBeGreaterThan(0);
      expect(totalBytes).toBeGreaterThan(0);
      expect(totalRows).toBe(100);
    });

    it('should record audit entry for export', async () => {
      AuditService.clearAuditLog();

      const request: ExportRequest = {
        id: 'test-2',
        userId: 'user-456',
        dataType: 'bids',
        format: 'json',
        limit: 50,
        timestamp: new Date(),
      };

      for await (const chunk of service.streamExport(request)) {
        // Consume stream
      }

      const auditEntries = AuditService.getUserExportHistory('user-456');
      expect(auditEntries).toHaveLength(1);
      expect(auditEntries[0].userId).toBe('user-456');
      expect(auditEntries[0].dataType).toBe('bids');
      expect(auditEntries[0].rowCount).toBe(50);
      expect(auditEntries[0].status).toBe('completed');
    });

    it('should enforce byte size limit', async () => {
      const customService = new ExportService({
        maxBytesPerRequest: 1, // Set very low limit
      });

      const request: ExportRequest = {
        id: 'test-3',
        userId: 'user-789',
        dataType: 'invoices',
        format: 'ndjson',
        limit: 100,
        timestamp: new Date(),
      };

      try {
        for await (const chunk of customService.streamExport(request)) {
          // Stream should error
        }
        throw new Error('Should have thrown size limit error');
      } catch (error) {
        expect(error).toBeInstanceOf(Error);
        expect((error as Error).message).toContain('size limit exceeded');
      }
    });

    it('should generate integrity checksum for verification', async () => {
      const request: ExportRequest = {
        id: 'test-4',
        userId: 'user-111',
        dataType: 'settlements',
        format: 'ndjson',
        limit: 50,
        timestamp: new Date(),
      };

      let finalChecksum = '';

      for await (const chunk of service.streamExport(request)) {
        finalChecksum = chunk.checksum;
      }

      expect(finalChecksum).toMatch(/^[a-f0-9]{64}$/);
    });
  });

  describe('exportSync', () => {
    it('should export data synchronously', async () => {
      const request: ExportRequest = {
        id: 'test-5',
        userId: 'user-222',
        dataType: 'disputes',
        format: 'csv',
        limit: 25,
        timestamp: new Date(),
      };

      const result = await service.exportSync(request);

      expect(result.data).toBeTruthy();
      expect(result.checksum).toMatch(/^[a-f0-9]{64}$/);
      expect(result.rowCount).toBe(25);
      expect(result.bytesTransferred).toBeGreaterThan(0);
    });

    it('should record audit entry for synchronous export', async () => {
      AuditService.clearAuditLog();

      const request: ExportRequest = {
        id: 'test-6',
        userId: 'user-333',
        dataType: 'invoices',
        format: 'json',
        limit: 30,
        timestamp: new Date(),
      };

      await service.exportSync(request);

      const auditEntries = AuditService.getUserExportHistory('user-333');
      expect(auditEntries).toHaveLength(1);
      expect(auditEntries[0].status).toBe('completed');
    });
  });
});

describe('AuditService', () => {
  beforeEach(() => {
    AuditService.clearAuditLog();
  });

  describe('recordExportAudit', () => {
    it('should record audit entry', () => {
      const entry = AuditService.recordExportAudit(
        'user-1',
        'invoices',
        'ndjson',
        100,
        5000,
        'abc123'
      );

      expect(entry.id).toBeTruthy();
      expect(entry.userId).toBe('user-1');
      expect(entry.dataType).toBe('invoices');
      expect(entry.rowCount).toBe(100);
      expect(entry.status).toBe('in-progress');
    });
  });

  describe('getUserExportHistory', () => {
    it('should retrieve user export history', () => {
      AuditService.recordExportAudit('user-1', 'invoices', 'ndjson', 100, 5000, 'abc1');
      AuditService.recordExportAudit('user-1', 'bids', 'json', 50, 3000, 'abc2');
      AuditService.recordExportAudit('user-2', 'settlements', 'csv', 25, 2000, 'abc3');

      const history = AuditService.getUserExportHistory('user-1');

      expect(history).toHaveLength(2);
      expect(history[0].userId).toBe('user-1');
    });
  });

  describe('getExportStatistics', () => {
    it('should calculate export statistics', () => {
      AuditService.recordExportAudit('user-1', 'invoices', 'ndjson', 100, 5000, 'abc1');
      AuditService.recordExportAudit('user-1', 'bids', 'json', 50, 3000, 'abc2');

      const stats = AuditService.getExportStatistics('user-1');

      expect(stats.totalExports).toBe(2);
      expect(stats.totalRowsExported).toBe(150);
      expect(stats.totalBytesExported).toBe(8000);
      expect(stats.byDataType['invoices']).toBe(1);
      expect(stats.byDataType['bids']).toBe(1);
    });
  });
});

describe('ExportController', () => {
  let controller: ExportController;

  beforeEach(() => {
    controller = new ExportController();
    AuditService.clearAuditLog();
  });

  describe('exportInvoices', () => {
    it('should export invoices with valid parameters', async () => {
      const req = {
        userId: 'user-1',
        query: {
          format: 'ndjson',
          limit: '100',
        },
      };

      const response = await controller.exportInvoices(req);

      expect(response.statusCode).toBe(200);
      expect(response.headers['Content-Type']).toContain('ndjson');
      expect(response.headers['X-Export-Id']).toBeTruthy();
    });

    it('should return 400 for invalid parameters', async () => {
      const req = {
        userId: 'user-1',
        query: {
          format: 'invalid' as any,
        },
      };

      const response = await controller.exportInvoices(req);

      expect(response.statusCode).toBe(400);
      if (typeof response.body === 'string') {
        const body = JSON.parse(response.body);
        expect(body.error).toBe('Invalid export parameters');
      }
    });

    it('should include Content-Disposition header', async () => {
      const req = {
        userId: 'user-1',
        query: {},
      };

      const response = await controller.exportInvoices(req);

      expect(response.headers['Content-Disposition']).toContain('attachment');
      expect(response.headers['Content-Disposition']).toContain('invoices');
    });
  });

  describe('exportBids', () => {
    it('should export bids with valid parameters', async () => {
      const req = {
        userId: 'user-2',
        query: {
          format: 'json',
        },
      };

      const response = await controller.exportBids(req);

      expect(response.statusCode).toBe(200);
      expect(response.headers['X-Export-Id']).toBeTruthy();
    });
  });

  describe('exportSettlements', () => {
    it('should export settlements', async () => {
      const req = {
        userId: 'user-3',
        query: {},
      };

      const response = await controller.exportSettlements(req);

      expect(response.statusCode).toBe(200);
    });
  });

  describe('exportDisputes', () => {
    it('should export disputes', async () => {
      const req = {
        userId: 'user-4',
        query: {},
      };

      const response = await controller.exportDisputes(req);

      expect(response.statusCode).toBe(200);
    });
  });

  describe('exportAuditLog', () => {
    it('should export audit log (admin endpoint)', async () => {
      const req = {
        userId: 'admin-1',
        query: {},
      };

      const response = await controller.exportAuditLog(req);

      expect(response.statusCode).toBe(200);
    });
  });

  describe('getExportStats', () => {
    it('should retrieve export statistics', async () => {
      const req = {
        userId: 'user-1',
      };

      const response = await controller.getExportStats(req);

      expect(response.statusCode).toBe(200);
      if (typeof response.body === 'string') {
        const body = JSON.parse(response.body);
        expect(body.message).toBeTruthy();
      }
    });
  });
});

describe('AdminRouter', () => {
  let router: AdminRouter;

  beforeEach(() => {
    router = new AdminRouter();
  });

  describe('route registration', () => {
    it('should register audit export route', () => {
      const routes = router.getRoutes();
      const auditRoute = routes.find((r) => r.path === '/admin/exports/audit');
      expect(auditRoute).toBeTruthy();
      expect(auditRoute?.method).toBe('GET');
    });

    it('should register statistics route', () => {
      const routes = router.getRoutes();
      const statsRoute = routes.find((r) => r.path === '/admin/exports/stats');
      expect(statsRoute).toBeTruthy();
      expect(statsRoute?.method).toBe('GET');
    });
  });

  describe('request handling', () => {
    it('should handle audit export request', async () => {
      const req = {
        userId: 'admin-1',
        query: {},
      };

      const response = await router.handleRequest('GET', '/admin/exports/audit', req);

      expect(response.statusCode).toBe(200);
    });

    it('should return 404 for unknown route', async () => {
      const req = { userId: 'admin-1' };
      const response = await router.handleRequest('GET', '/unknown/route', req);

      expect(response.statusCode).toBe(404);
    });
  });
});

describe('Integration: Full Export Workflow', () => {
  let service: ExportService;
  let controller: ExportController;

  beforeEach(() => {
    service = new ExportService();
    controller = new ExportController(service);
    AuditService.clearAuditLog();
  });

  it('should complete full export workflow with audit trail', async () => {
    const req = {
      userId: 'user-complete-flow',
      query: {
        startDate: '2024-01-01',
        endDate: '2024-01-31',
        format: 'ndjson',
        limit: '100',
      },
    };

    // Execute export
    const response = await controller.exportInvoices(req);

    expect(response.statusCode).toBe(200);
    expect(response.headers['X-Export-Id']).toBeTruthy();

    // Verify audit entry was created
    const history = AuditService.getUserExportHistory('user-complete-flow');
    expect(history.length).toBeGreaterThan(0);
  });

  it('should validate across the full stack', async () => {
    const req = {
      userId: 'user-invalid',
      query: {
        format: 'invalid',
        limit: '50000',
      },
    };

    const response = await controller.exportBids(req);

    expect(response.statusCode).toBe(400);

    // Verify no audit entry for failed validation
    const history = AuditService.getUserExportHistory('user-invalid');
    expect(history).toHaveLength(0);
  });
});
