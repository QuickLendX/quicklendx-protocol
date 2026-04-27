import request from 'supertest';
import app from '../../app'; // Assuming app.ts exports the express app
import { SnapshotService } from '../../services/snapshotService';

// Mock the snapshot service
jest.mock('../../services/snapshotService');

const mockSnapshotService = SnapshotService as jest.Mocked<typeof SnapshotService>;

describe('Bids API', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  describe('GET /api/v1/bids/best/:invoiceId', () => {
    it('should return best bid for invoice', async () => {
      const mockBestBid = {
        invoice_id: 'inv1',
        bid_id: 'bid1',
        investor: 'investor1',
        bid_amount: '1000000',
      };

      mockSnapshotService.getBestBid.mockResolvedValue(mockBestBid);

      const response = await request(app)
        .get('/api/v1/bids/best/inv1')
        .expect(200);

      expect(response.body).toEqual(mockBestBid);
      expect(mockSnapshotService.getBestBid).toHaveBeenCalledWith('inv1');
    });

    it('should return 404 if no best bid found', async () => {
      mockSnapshotService.getBestBid.mockResolvedValue(null);

      const response = await request(app)
        .get('/api/v1/bids/best/inv1')
        .expect(404);

      expect(response.body).toEqual({ error: 'No best bid found for this invoice' });
    });
  });

  describe('GET /api/v1/bids/top/:invoiceId', () => {
    it('should return top bids for invoice', async () => {
      const mockTopBids = [
        { bid_id: 'bid1', rank: 1 },
        { bid_id: 'bid2', rank: 2 },
      ];

      mockSnapshotService.getTopBids.mockResolvedValue(mockTopBids);

      const response = await request(app)
        .get('/api/v1/bids/top/inv1')
        .expect(200);

      expect(response.body).toEqual({ top_bids: mockTopBids });
      expect(mockSnapshotService.getTopBids).toHaveBeenCalledWith('inv1');
    });
  });
});