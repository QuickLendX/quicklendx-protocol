import { SnapshotService } from '../../services/snapshotService';
import { BidEvent } from '../../types/snapshot';

// Mock the database pool
jest.mock('../../services/database');
import pool from '../../services/database';

const mockPool = pool as jest.Mocked<typeof pool>;

describe('SnapshotService', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  describe('processBidEvent', () => {
    it('should process BidPlaced event correctly', async () => {
      const event: BidEvent = {
        event_type: 'BidPlaced',
        bid_id: 'bid1',
        invoice_id: 'inv1',
        investor: 'investor1',
        bid_amount: '1000000',
        expected_return: '50000',
        timestamp: 1000,
        expiration_timestamp: 2000,
        block_timestamp: 1000,
        transaction_sequence: 1,
        ledger_index: 1,
      };

      mockPool.connect.mockResolvedValue({
        query: jest.fn().mockResolvedValue({}),
        release: jest.fn(),
      } as any);

      await SnapshotService.processBidEvent(event);

      expect(mockPool.connect).toHaveBeenCalled();
    });

    it('should handle BidWithdrawn event', async () => {
      const event: BidEvent = {
        event_type: 'BidWithdrawn',
        bid_id: 'bid1',
        invoice_id: 'inv1',
        investor: 'investor1',
        bid_amount: '1000000',
        expected_return: '50000',
        timestamp: 1000,
        expiration_timestamp: 2000,
        block_timestamp: 1000,
        transaction_sequence: 1,
        ledger_index: 1,
      };

      const mockClient = {
        query: jest.fn().mockResolvedValue({ rows: [] }),
        release: jest.fn(),
      };
      mockPool.connect.mockResolvedValue(mockClient as any);

      await SnapshotService.processBidEvent(event);

      expect(mockClient.query).toHaveBeenCalledWith('BEGIN');
      expect(mockClient.query).toHaveBeenCalledWith('COMMIT');
    });
  });

  describe('compareBids', () => {
    it('should prefer higher bid amount', () => {
      const newBid = { bid_amount: '2000000', block_timestamp: 1000, transaction_sequence: 1, ledger_index: 1 };
      const currentBest = { bid_amount: '1000000', block_timestamp: 1000, transaction_sequence: 1, ledger_index: 1 };

      // Access private method for testing
      const result = (SnapshotService as any).compareBids(newBid, currentBest);
      expect(result).toBe(true);
    });

    it('should use tie-breakers when amounts are equal', () => {
      const newBid = { bid_amount: '1000000', block_timestamp: 999, transaction_sequence: 1, ledger_index: 1 };
      const currentBest = { bid_amount: '1000000', block_timestamp: 1000, transaction_sequence: 1, ledger_index: 1 };

      const result = (SnapshotService as any).compareBids(newBid, currentBest);
      expect(result).toBe(true);
    });
  });

  describe('getBestBid', () => {
    it('should return best bid for invoice', async () => {
      const mockResult = {
        rows: [{
          invoice_id: 'inv1',
          bid_id: 'bid1',
          investor: 'investor1',
          bid_amount: '1000000',
        }],
      };
      mockPool.query.mockResolvedValue(mockResult as any);

      const result = await SnapshotService.getBestBid('inv1');

      expect(result).toEqual(mockResult.rows[0]);
      expect(mockPool.query).toHaveBeenCalledWith(
        'SELECT * FROM best_bids WHERE invoice_id = $1',
        ['inv1']
      );
    });

    it('should return null if no best bid', async () => {
      mockPool.query.mockResolvedValue({ rows: [] } as any);

      const result = await SnapshotService.getBestBid('inv1');

      expect(result).toBeNull();
    });
  });

  describe('getTopBids', () => {
    it('should return top bids for invoice', async () => {
      const mockTopBids = [
        { bid_id: 'bid1', rank: 1 },
        { bid_id: 'bid2', rank: 2 },
      ];
      mockPool.query.mockResolvedValue({ rows: [{ top_bids: mockTopBids }] } as any);

      const result = await SnapshotService.getTopBids('inv1');

      expect(result).toEqual(mockTopBids);
    });

    it('should return empty array if no top bids', async () => {
      mockPool.query.mockResolvedValue({ rows: [] } as any);

      const result = await SnapshotService.getTopBids('inv1');

      expect(result).toEqual([]);
    });
  });
});