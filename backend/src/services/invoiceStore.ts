import { getDatabase, getPreparedStatement } from '../lib/database';
import { Invoice, InvoiceStatus } from '../types/contract';
import {
  CursorPayload,
  PageResult,
  PaginationError,
  encodeCursor,
  paginationScope,
} from '../utils/pagination';

export const invoiceStore = {
  findInvoices(filter: { business?: string; status?: InvoiceStatus } = {}): Invoice[] {
    let query = 'SELECT * FROM invoices';
    const params: any[] = [];
    const conditions: string[] = [];

    if (filter.business) {
      conditions.push('business = ?');
      params.push(filter.business);
    }
    if (filter.status) {
      conditions.push('status = ?');
      params.push(filter.status);
    }

    if (conditions.length > 0) {
      query += ' WHERE ' + conditions.join(' AND ');
    }

    const rows = getPreparedStatement(query).all(...params);
    return rows.map(mapRowToInvoice);
  },

  listPage({
    filters = {},
    limit,
    cursor,
  }: {
    filters?: { business?: string; status?: InvoiceStatus };
    limit: number;
    cursor: CursorPayload | null;
  }): PageResult<Invoice> {
    const scope = paginationScope(filters);
    if (cursor && cursor.scope !== scope) {
      throw new PaginationError("cursor scope mismatch");
    }

    const conditions: string[] = [];
    const params: unknown[] = [];
    if (filters.business) {
      conditions.push("business = ?");
      params.push(filters.business);
    }
    if (filters.status) {
      conditions.push("status = ?");
      params.push(filters.status);
    }
    if (cursor) {
      conditions.push("(created_at < ? OR (created_at = ? AND id > ?))");
      params.push(cursor.sort_val, cursor.sort_val, cursor.id);
    }

    let sql = "SELECT * FROM invoices";
    if (conditions.length > 0) {
      sql += ` WHERE ${conditions.join(" AND ")}`;
    }
    sql += " ORDER BY created_at DESC, id ASC LIMIT ?";

    const rows = getDatabase().prepare(sql).all(...params, limit + 1) as any[];
    const has_more = rows.length > limit;
    const data = rows.slice(0, limit).map(mapRowToInvoice);
    const last = data[data.length - 1];

    return {
      data,
      has_more,
      next_cursor:
        has_more && last
          ? encodeCursor({ id: last.id, sort_val: last.created_at, scope })
          : null,
    };
  },

  findInvoiceById(id: string): Invoice | undefined {
    const row = getPreparedStatement('SELECT * FROM invoices WHERE id = ?').get(id);
    if (!row) return undefined;
    return mapRowToInvoice(row);
  },

  insertInvoice(invoice: Invoice): void {
    getPreparedStatement(`
      INSERT INTO invoices (
        id, business, amount, currency, due_date, status, description, category, tags, metadata,
        created_at, updated_at, contract_version, event_schema_version, indexed_at
      ) VALUES (
        ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
      )
    `).run(
      invoice.id,
      invoice.business,
      invoice.amount,
      invoice.currency,
      invoice.due_date,
      invoice.status,
      invoice.description,
      invoice.category,
      JSON.stringify(invoice.tags || []),
      JSON.stringify(invoice.metadata || {}),
      invoice.created_at,
      invoice.updated_at,
      invoice.contract_version ?? 1,
      invoice.event_schema_version ?? 1,
      invoice.indexed_at ?? null
    );
  },

  deleteAll(): void {
    getPreparedStatement('DELETE FROM invoices').run();
  }
};

function mapRowToInvoice(row: any): Invoice {
  return {
    id: row.id,
    business: row.business,
    amount: row.amount,
    currency: row.currency,
    due_date: row.due_date,
    status: row.status as InvoiceStatus,
    description: row.description,
    category: row.category as any,
    tags: JSON.parse(row.tags || '[]'),
    metadata: JSON.parse(row.metadata || '{}'),
    created_at: row.created_at,
    updated_at: row.updated_at,
    contract_version: row.contract_version ?? 1,
    event_schema_version: row.event_schema_version ?? 1,
    indexed_at: row.indexed_at ?? null,
  };
}
