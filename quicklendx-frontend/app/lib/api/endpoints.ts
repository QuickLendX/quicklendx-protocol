import { apiClient } from './api-client';
import { z } from 'zod';
import {
  Invoice,
  InvoiceStatus,
  Bid,
  BidStatus,
  Settlement,
  SettlementStatus,
  PortfolioEntry,
  Dispute,
  FreshnessMetadata,
  Page,
  HexId,
} from './schemas';
import { AppError, SystemError, ValidationError } from './errors';

// Helper to validate query parameters before making a request
function validateParams<T extends z.ZodTypeAny>(schema: T, params: unknown): T['_output'] {
  const result = schema.safeParse(params);
  if (!result.success) {
    throw new ValidationError('Invalid query parameters', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}

// Query schemas
const ListInvoicesQuery = z.object({
  business: z.string().optional(),
  status: InvoiceStatus.optional(),
  limit: z.number().int().min(1).max(100).optional(),
  cursor: z.string().optional(),
});

export async function listInvoices(q: z.infer<typeof ListInvoicesQuery>) {
  const params = validateParams(ListInvoicesQuery, q);
  const data = await apiClient.get<any>('/invoices', { params });
  // Validate response shape
  const pageSchema = Page(Invoice);
  const result = pageSchema.safeParse(data);
  if (!result.success) {
    throw new SystemError('Unexpected response from server', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}

export async function getInvoice(id: HexId) {
  if (!HexId.safeParse(id).success) {
    throw new ValidationError('Invalid invoice id', {});
  }
  const data = await apiClient.get<any>(`/invoices/${id}`);
  const result = Invoice.safeParse(data);
  if (!result.success) {
    throw new SystemError('Unexpected response from server', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}

// bids helpers
const ListBidsQuery = z.object({
  invoice_id: HexId,
  investor: z.string().optional(),
  status: BidStatus.optional(),
  limit: z.number().int().min(1).max(100).optional(),
  cursor: z.string().optional(),
});

export async function listBids(q: z.infer<typeof ListBidsQuery>) {
  const params = validateParams(ListBidsQuery, q);
  const data = await apiClient.get<any>('/bids', { params });
  const pageSchema = Page(Bid);
  const result = pageSchema.safeParse(data);
  if (!result.success) {
    throw new SystemError('Unexpected response from server', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}

export async function getBestBid(invoiceId: HexId) {
  if (!HexId.safeParse(invoiceId).success) {
    throw new ValidationError('Invalid invoice id', {});
  }
  try {
    const data = await apiClient.get<any>(`/bids/${invoiceId}/best`);
    const result = Bid.safeParse(data);
    if (!result.success) {
      throw new SystemError('Unexpected response from server', {
        additionalData: { issues: result.error.issues },
      });
    }
    return result.data;
  } catch (e) {
    if (e instanceof AppError && e.name === 'BusinessLogicError') {
      // 404 is mapped to BusinessLogicError in api-client
      return null;
    }
    throw e;
  }
}

export async function getRankedBids(invoiceId: HexId) {
  if (!HexId.safeParse(invoiceId).success) {
    throw new ValidationError('Invalid invoice id', {});
  }
  const data = await apiClient.get<any>(`/bids/${invoiceId}/ranked`);
  const result = z.array(Bid).safeParse(data?.data ?? []);
  if (!result.success) {
    throw new SystemError('Unexpected response from server', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}

// settlements helpers
const ListSettlementsQuery = z.object({
  invoice_id: z.string().optional(),
});

export async function listSettlements(q: z.infer<typeof ListSettlementsQuery>) {
  const params = validateParams(ListSettlementsQuery, q);
  const data = await apiClient.get<any>('/settlements', { params });
  const schema = z.object({
    data: z.array(Settlement),
    freshness: FreshnessMetadata,
    _system: z.any().optional(),
  });
  const result = schema.safeParse(data);
  if (!result.success) {
    throw new SystemError('Unexpected response from server', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}

export async function getSettlement(id: HexId) {
  if (!HexId.safeParse(id).success) {
    throw new ValidationError('Invalid settlement id', {});
  }
  const data = await apiClient.get<any>(`/settlements/${id}`);
  const result = Settlement.safeParse(data);
  if (!result.success) {
    throw new SystemError('Unexpected response from server', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}

// portfolio helpers
const ListPortfolioQuery = z.object({
  investor: z.string(),
  limit: z.number().int().min(1).max(100).optional(),
  cursor: z.string().optional(),
});

export async function listPortfolio(q: z.infer<typeof ListPortfolioQuery>) {
  const params = validateParams(ListPortfolioQuery, q);
  const data = await apiClient.get<any>('/portfolio', { params });
  const pageSchema = Page(PortfolioEntry);
  const result = pageSchema.safeParse(data);
  if (!result.success) {
    throw new SystemError('Unexpected response from server', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}

// disputes helper
export async function listInvoiceDisputes(invoiceId: HexId) {
  if (!HexId.safeParse(invoiceId).success) {
    throw new ValidationError('Invalid invoice id', {});
  }
  const data = await apiClient.get<any>(`/invoices/${invoiceId}/disputes`);
  const pageSchema = Page(Dispute);
  const result = pageSchema.safeParse(data);
  if (!result.success) {
    throw new SystemError('Unexpected response from server', {
      additionalData: { issues: result.error.issues },
    });
  }
  return result.data;
}
