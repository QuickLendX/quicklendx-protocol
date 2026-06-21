import { getDatabase, getPreparedStatement } from '../lib/database';
import { Invoice, InvoiceStatus } from '../types/contract';

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
      invoice.contract_version,
      invoice.event_schema_version,
      invoice.indexed_at
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
    tags: JSON.parse(row.tags),
    metadata: JSON.parse(row.metadata),
    created_at: row.created_at,
    updated_at: row.updated_at,
    contract_version: row.contract_version,
    event_schema_version: row.event_schema_version,
    indexed_at: row.indexed_at,
  };
}
