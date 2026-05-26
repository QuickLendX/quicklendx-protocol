/**
 * Invoice endpoint fixtures
 */

export const validInvoice = {
  id: 'aa0e8400-e29b-41d4-a716-446655440010',
  businessId: '770e8400-e29b-41d4-a716-446655440001',
  amount: '10000.00',
  currency: 'USDC',
  dueDate: '2024-03-15T00:00:00Z',
  status: 'active',
  metadata: {
    invoiceNumber: 'INV-2024-001',
    customerName: 'Acme Corp',
  },
};

export const fundedInvoice = {
  id: 'bb0e8400-e29b-41d4-a716-446655440011',
  businessId: '770e8400-e29b-41d4-a716-446655440001',
  amount: '25000.50',
  currency: 'USDC',
  dueDate: '2024-04-01T00:00:00Z',
  status: 'funded',
  metadata: {
    invoiceNumber: 'INV-2024-002',
    customerName: 'Tech Solutions Inc',
  },
};

export const settledInvoice = {
  id: 'cc0e8400-e29b-41d4-a716-446655440012',
  businessId: '770e8400-e29b-41d4-a716-446655440001',
  amount: '5000.00',
  currency: 'EURC',
  dueDate: '2024-02-01T00:00:00Z',
  status: 'settled',
  metadata: {
    invoiceNumber: 'INV-2024-003',
    customerName: 'Global Trading Ltd',
  },
};

export const validInvoiceListResponse = {
  data: [validInvoice, fundedInvoice],
  total: 2,
  limit: 20,
  offset: 0,
};

export const emptyInvoiceListResponse = {
  data: [],
  total: 0,
  limit: 20,
  offset: 0,
};

export const validCreateInvoiceRequest = {
  amount: '15000.00',
  currency: 'USDC',
  dueDate: '2024-05-01T00:00:00Z',
  metadata: {
    invoiceNumber: 'INV-2024-004',
    customerName: 'New Customer LLC',
  },
};

export const invalidInvoiceRequest = {
  amount: 'invalid-amount',
  currency: 'USD', // Not in enum
  dueDate: 'not-a-date',
};

export const invoiceNotFoundError = {
  error: 'NOT_FOUND',
  message: 'Invoice not found',
};

export const invalidInvoiceError = {
  error: 'VALIDATION_ERROR',
  message: 'Invalid request parameters',
  details: {
    amount: 'Must match pattern ^\\d+(\\.\\d{1,7})?$',
    currency: 'Must be one of: USDC, EURC',
  },
};
