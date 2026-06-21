import request from 'supertest';
import express from 'express';
import { getInvoices, getInvoiceById, MOCK_INVOICES } from '../controllers/v1/invoices';
import * as cacheHeaders from '../middleware/cache-headers';

const app = express();
app.use(express.json());
app.get('/invoices', getInvoices);
app.get('/invoices/:id', getInvoiceById);

app.use((err: any, req: any, res: any, next: any) => {
  res.status(500).json({ error: err.message });
});

jest.mock('../middleware/cache-headers', () => ({
  applyCacheHeaders: jest.fn().mockReturnValue(false),
  CC_SHORT: 'short',
}));

const KNOWN_ID = MOCK_INVOICES[0].id;
const KNOWN_BUSINESS = MOCK_INVOICES[0].business;

describe('invoices controller', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    (cacheHeaders.applyCacheHeaders as jest.Mock).mockReturnValue(false);
  });

  describe('getInvoices', () => {
    it('should return invoices filtered by business', async () => {
      const res = await request(app).get(`/invoices?business=${KNOWN_BUSINESS}`);
      expect(res.status).toBe(200);
      expect(res.body.data).toHaveLength(1);
      expect(res.body.data[0].id).toBe(KNOWN_ID);
    });

    it('should return invoices filtered by status', async () => {
      const res = await request(app).get('/invoices?status=Pending');
      expect(res.status).toBe(200);
      expect(Array.isArray(res.body.data)).toBe(true);
    });

    it('should return empty list if no match', async () => {
      const res = await request(app).get('/invoices?business=unknown-business');
      expect(res.status).toBe(200);
      expect(res.body.data).toHaveLength(0);
    });

    it('should return all invoices with no filter', async () => {
      const res = await request(app).get('/invoices');
      expect(res.status).toBe(200);
      expect(res.body.data.length).toBeGreaterThanOrEqual(1);
    });
  });

  describe('getInvoiceById', () => {
    it('should return the invoice if found', async () => {
      const res = await request(app).get(`/invoices/${KNOWN_ID}`);
      expect(res.status).toBe(200);
      expect(res.body.id).toBe(KNOWN_ID);
    });

    it('should return 404 if not found', async () => {
      const res = await request(app).get('/invoices/unknown-id');
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe('INVOICE_NOT_FOUND');
    });

    it('should return 304 if cache headers match', async () => {
      (cacheHeaders.applyCacheHeaders as jest.Mock).mockReturnValue(true);

      const res = await request(app).get(`/invoices/${KNOWN_ID}`);
      expect(res.status).toBe(304);
    });
  });
});
