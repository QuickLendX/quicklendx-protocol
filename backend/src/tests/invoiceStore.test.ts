import { invoiceStore } from '../services/invoiceStore';
import { getDatabase, closeDatabase } from '../lib/database';
import { Invoice, InvoiceStatus, InvoiceCategory } from '../types/contract';
import migration006 from '../migrations/v006_create_invoices_table';

beforeAll(async () => {
  // Use in-memory DB for tests if possible, or test DB
  process.env.DATABASE_PATH = ':memory:';
  const db = getDatabase();
  await migration006.up({ db, isProduction: false } as any);
});

afterAll(() => {
  closeDatabase();
});

beforeEach(() => {
  invoiceStore.deleteAll();
});

const mockInvoice: Invoice = {
  id: "test-id-1",
  business: "biz-1",
  amount: "1000",
  currency: "USD",
  due_date: 1234567890,
  status: InvoiceStatus.Pending,
  description: "Test",
  category: InvoiceCategory.Services,
  tags: ["test"],
  metadata: {
    customer_name: "John",
    customer_address: "123 Main St",
    tax_id: "123",
    line_items: [],
    notes: ""
  },
  created_at: 1234567800,
  updated_at: 1234567800,
  contract_version: 1,
  event_schema_version: 1,
  indexed_at: new Date().toISOString()
};

describe('invoiceStore', () => {
  it('should insert and find an invoice by id', () => {
    invoiceStore.insertInvoice(mockInvoice);
    const found = invoiceStore.findInvoiceById("test-id-1");
    expect(found).toBeDefined();
    expect(found?.id).toBe("test-id-1");
    expect(found?.business).toBe("biz-1");
    expect(found?.tags).toEqual(["test"]);
    expect(found?.metadata.customer_name).toBe("John");
  });

  it('should return undefined for unknown id', () => {
    const found = invoiceStore.findInvoiceById("unknown");
    expect(found).toBeUndefined();
  });

  it('should find invoices by business filter', () => {
    invoiceStore.insertInvoice(mockInvoice);
    invoiceStore.insertInvoice({ ...mockInvoice, id: "test-id-2", business: "biz-2" });

    const found = invoiceStore.findInvoices({ business: "biz-1" });
    expect(found).toHaveLength(1);
    expect(found[0].id).toBe("test-id-1");
  });

  it('should find invoices by status filter', () => {
    invoiceStore.insertInvoice(mockInvoice);
    invoiceStore.insertInvoice({ ...mockInvoice, id: "test-id-2", status: InvoiceStatus.Verified });

    const found = invoiceStore.findInvoices({ status: InvoiceStatus.Verified });
    expect(found).toHaveLength(1);
    expect(found[0].id).toBe("test-id-2");
  });

  it('should find invoices by both business and status filters', () => {
    invoiceStore.insertInvoice(mockInvoice);
    invoiceStore.insertInvoice({ ...mockInvoice, id: "test-id-2", status: InvoiceStatus.Verified });
    invoiceStore.insertInvoice({ ...mockInvoice, id: "test-id-3", business: "biz-2", status: InvoiceStatus.Verified });

    const found = invoiceStore.findInvoices({ business: "biz-1", status: InvoiceStatus.Verified });
    expect(found).toHaveLength(1);
    expect(found[0].id).toBe("test-id-2");
  });

  it('should return all invoices when no filter is provided', () => {
    invoiceStore.insertInvoice(mockInvoice);
    invoiceStore.insertInvoice({ ...mockInvoice, id: "test-id-2" });

    const found = invoiceStore.findInvoices();
    expect(found).toHaveLength(2);
  });
});
