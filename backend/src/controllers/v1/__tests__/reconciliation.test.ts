import { Request, Response, NextFunction } from 'express';
import { getDriftReports } from '../reconciliation';
import { ReconciliationWorker } from '../../../services/reconciliationWorker';

jest.mock('../../../services/reconciliationWorker');

describe('reconciliation controller', () => {
  describe('getDriftReports', () => {
    let req: Partial<Request>;
    let res: Partial<Response>;
    let next: jest.Mock;
    let json: jest.Mock;
    let status: jest.Mock;

    beforeEach(() => {
      json = jest.fn();
      status = jest.fn().mockReturnValue({ json });
      req = {
        query: {}
      };
      res = {
        status,
        json
      };
      next = jest.fn();
      jest.clearAllMocks();
    });

    const mockReports = (reports: any[]) => {
      (ReconciliationWorker.getAllReports as jest.Mock).mockReturnValue(reports);
    };

    it('returns a paginated list of drift reports with default limit', async () => {
      const reports = Array.from({ length: 60 }, (_, i) => ({
        id: `report-${i.toString().padStart(2, '0')}`,
        createdAt: new Date(`2026-01-01T00:00:${i.toString().padStart(2, '0')}Z`)
      }));
      mockReports(reports);

      await getDriftReports(req as Request, res as Response, next);

      expect(json).toHaveBeenCalledWith(expect.objectContaining({
        data: expect.any(Array),
        pagination: expect.objectContaining({
          limit: 50,
          hasMore: true,
          nextCursor: expect.any(String)
        })
      }));
      const callData = json.mock.calls[0][0];
      expect(callData.data.length).toBe(50);
    });

    it('validates custom limits properly', async () => {
      req.query = { limit: '20' };
      mockReports([]);

      await getDriftReports(req as Request, res as Response, next);

      expect(json).toHaveBeenCalledWith(expect.objectContaining({
        pagination: expect.objectContaining({ limit: 20 })
      }));
    });

    it('rejects invalid limits (negative, zero, above 100, non-numeric)', async () => {
      const invalidLimits = ['-1', '0', '101', 'abc', ' 50x '];

      for (const limit of invalidLimits) {
        req.query = { limit };
        await getDriftReports(req as Request, res as Response, next);
        expect(status).toHaveBeenCalledWith(400);
        expect(json).toHaveBeenCalledWith({ error: 'Invalid limit' });
        status.mockClear();
        json.mockClear();
      }
    });

    it('rejects invalid cursor formats deterministically', async () => {
      const invalidCursors = ['invalid-base64', 'v2:something', Buffer.from('v1').toString('base64url')]; // v1 with no id

      for (const cursor of invalidCursors) {
        req.query = { cursor };
        await getDriftReports(req as Request, res as Response, next);
        expect(status).toHaveBeenCalledWith(400);
        expect(json).toHaveBeenCalledWith({ error: 'Invalid cursor' });
        status.mockClear();
        json.mockClear();
      }
    });

    it('rejects cursor of wrong type deterministically', async () => {
      req.query = { cursor: ['array-is-not-string'] as any };
      await getDriftReports(req as Request, res as Response, next);
      expect(status).toHaveBeenCalledWith(400);
      expect(json).toHaveBeenCalledWith({ error: 'Invalid cursor' });
    });

    it('handles unexpected errors via next()', async () => {
      const error = new Error('Database connection failed');
      (ReconciliationWorker.getAllReports as jest.Mock).mockImplementation(() => {
        throw error;
      });

      await getDriftReports(req as Request, res as Response, next);

      expect(next).toHaveBeenCalledWith(error);
      expect(status).not.toHaveBeenCalled();
    });

    it('paginates correctly using cursor', async () => {
      const reports = Array.from({ length: 10 }, (_, i) => ({
        id: `report-${i}`,
        createdAt: new Date(`2026-01-01T00:00:0${i}Z`)
      }));
      mockReports(reports);

      // Fetch first page
      req.query = { limit: '5' };
      await getDriftReports(req as Request, res as Response, next);
      
      const firstResponse = json.mock.calls[0][0];
      expect(firstResponse.data.length).toBe(5);
      expect(firstResponse.pagination.hasMore).toBe(true);

      // Fetch second page
      req.query = { limit: '5', cursor: firstResponse.pagination.nextCursor };
      json.mockClear();
      await getDriftReports(req as Request, res as Response, next);
      
      const secondResponse = json.mock.calls[0][0];
      expect(secondResponse.data.length).toBe(5);
      expect(secondResponse.pagination.hasMore).toBe(false);
      expect(secondResponse.pagination.nextCursor).toBeNull();

      // Ensure no overlap
      expect(firstResponse.data[4].id).not.toBe(secondResponse.data[0].id);
    });
  });
});
