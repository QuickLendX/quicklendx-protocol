import request from 'supertest';
import express from 'express';
import { getInvoices, getInvoiceById } from '../controllers/v1/invoices';
import { invoiceStore } from '../services/invoiceStore';
import { InvoiceStatus, InvoiceCategory } from '../types/contract';
import * as cacheHeaders from '../middleware/cache-headers';

const app = express();
app.use(express.json());
app.get('/invoices', getInvoices);
app.get('/invoices/:id', getInvoiceById);

// Add basic error handling for next(error)
app.use((err: any, req: any, res: any, next: any) => {
  res.status(500).json({ error: err.message });
});

jest.mock('../services/invoiceStore');
jest.mock('../middleware/cache-headers', () => ({
  applyCacheHeaders: jest.fn().mockReturnValue(false),
  CC_SHORT: 'short',
}));

const mockInvoice = {
  id: "test-id-1",
  business: "biz-1",
  amount: "1000",
  currency: "USD",
  due_date: 1234567890,
  status: InvoiceStatus.Pending,
  description: "Test",
  category: InvoiceCategory.Services,
  tags: [],
  metadata: { customer_name: "John", customer_address: "123", tax_id: "", line_items: [], notes: "" },
  created_at: 1234567800,
  updated_at: 1234567800,
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString()
};

describe('invoices controller', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    (cacheHeaders.applyCacheHeaders as jest.Mock).mockReturnValue(false);
  });

  describe('getInvoices', () => {
    it('should return filtered invoices', async () => {
      (invoiceStore.findInvoices as jest.Mock).mockReturnValue([mockInvoice]);

      const res = await request(app).get('/invoices?business=biz-1');
      expect(res.status).toBe(200);
      expect(res.body.data).toHaveLength(1);
      expect(res.body.data[0].id).toBe('test-id-1');
      expect(invoiceStore.findInvoices).toHaveBeenCalledWith({ business: 'biz-1' });
    });

    it('should return filtered invoices by status', async () => {
      (invoiceStore.findInvoices as jest.Mock).mockReturnValue([mockInvoice]);

      const res = await request(app).get('/invoices?status=Pending');
      expect(res.status).toBe(200);
      expect(invoiceStore.findInvoices).toHaveBeenCalledWith({ status: 'Pending' });
    });

    it('should return empty list if no match', async () => {
      (invoiceStore.findInvoices as jest.Mock).mockReturnValue([]);

      const res = await request(app).get('/invoices?business=unknown');
      expect(res.status).toBe(200);
      expect(res.body.data).toHaveLength(0);
    });

    it('should handle errors', async () => {
      (invoiceStore.findInvoices as jest.Mock).mockImplementation(() => { throw new Error('DB Error'); });

      const res = await request(app).get('/invoices');
      expect(res.status).toBe(500);
      expect(res.body.error).toBe('DB Error');
    });
  });

  describe('getInvoiceById', () => {
    it('should return the invoice if found', async () => {
      (invoiceStore.findInvoiceById as jest.Mock).mockReturnValue(mockInvoice);

      const res = await request(app).get('/invoices/test-id-1');
      expect(res.status).toBe(200);
      expect(res.body.id).toBe('test-id-1');
      expect(invoiceStore.findInvoiceById).toHaveBeenCalledWith('test-id-1');
    });

    it('should return 404 if not found', async () => {
      (invoiceStore.findInvoiceById as jest.Mock).mockReturnValue(undefined);

      const res = await request(app).get('/invoices/unknown');
      expect(res.status).toBe(404);
      expect(res.body.error.code).toBe('INVOICE_NOT_FOUND');
    });

    it('should return 304 if cache headers match', async () => {
      (invoiceStore.findInvoiceById as jest.Mock).mockReturnValue(mockInvoice);
      (cacheHeaders.applyCacheHeaders as jest.Mock).mockReturnValue(true);

      const res = await request(app).get('/invoices/test-id-1');
      expect(res.status).toBe(304);
    });

    it('should handle errors', async () => {
      (invoiceStore.findInvoiceById as jest.Mock).mockImplementation(() => { throw new Error('DB Error'); });

      const res = await request(app).get('/invoices/test-id-1');
      expect(res.status).toBe(500);
      expect(res.body.error).toBe('DB Error');
    });
  });
});
