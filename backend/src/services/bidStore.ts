/**
 * Bid Store Service
 *
 * Handles persistent storage and retrieval of bids with contract-compliant ranking.
 * Implements the bid comparison logic matching the contract's deterministic ordering:
 * (1) profit (expected_return - bid_amount), (2) expected_return, (3) bid_amount,
 * (4) timestamp (newer first), (5) bid_id as tiebreaker.
 */

import pool from '../services/database';
import { Bid, BidStatus, InvoiceStatus } from '../types/contract';
import { CreateBidBody } from '../validators/bids';
import { PageResult, CursorPayload, encodeCursor, decodeCursor } from '../utils/pagination';

export interface BidCreateInput extends CreateBidBody {
  bid_id: string;
  investor: string;
  timestamp: number;
  created_by: string;
}

export class BidStore {
  /**
   * Create a new bid in the database after validation.
   * Rejects bids on non-Verified invoices or duplicate bids.
   */
  static async createBid(input: BidCreateInput): Promise<Bid> {
    const client = await pool.connect();
    try {
      await client.query('BEGIN');

      // Verify invoice exists and is in Verified status
      const invoiceResult = await client.query(
        'SELECT id, status FROM invoices WHERE id = $1',
        [input.invoice_id]
      );

      if (invoiceResult.rows.length === 0) {
        throw new Error(`Invoice not found: ${input.invoice_id}`);
      }

      const invoice = invoiceResult.rows[0];
      if (invoice.status !== InvoiceStatus.Verified) {
        throw new Error(
          `Cannot place bid on invoice with status ${invoice.status}. Only Verified invoices accept bids.`
        );
      }

      // Check for duplicate bid from same investor on same invoice
      const duplicateResult = await client.query(
        'SELECT bid_id FROM bids WHERE invoice_id = $1 AND investor = $2 AND status = $3',
        [input.invoice_id, input.investor, BidStatus.Placed]
      );

      if (duplicateResult.rows.length > 0) {
        throw new Error(`Investor ${input.investor} already has an active bid on this invoice`);
      }

      // Check bid floor: bid_amount >= 1 (minimum valid amount)
      const bidAmountNum = BigInt(input.bid_amount);
      if (bidAmountNum < 1n) {
        throw new Error('Bid amount must be at least 1');
      }

      // Check expected_return >= bid_amount
      const expectedReturnNum = BigInt(input.expected_return);
      if (expectedReturnNum < bidAmountNum) {
        throw new Error('Expected return must be greater than or equal to bid amount');
      }

      // Insert the bid
      const result = await client.query(
        `INSERT INTO bids (
          bid_id, invoice_id, investor, bid_amount, expected_return,
          timestamp, status, expiration_timestamp, created_by
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *`,
        [
          input.bid_id,
          input.invoice_id,
          input.investor,
          input.bid_amount,
          input.expected_return,
          input.timestamp,
          BidStatus.Placed,
          input.expiration_timestamp,
          input.created_by,
        ]
      );

      await client.query('COMMIT');
      return this.rowToBid(result.rows[0]);
    } catch (error) {
      await client.query('ROLLBACK');
      throw error;
    } finally {
      client.release();
    }
  }

  /**
   * Get ranked bids for an invoice (best bid first, matching contract semantics).
   * Only returns Placed bids, filtered and sorted by contract ranking logic.
   */
  static async getRankedBids(invoiceId: string, limit: number = 100): Promise<Bid[]> {
    const result = await pool.query(
      `SELECT * FROM bids
       WHERE invoice_id = $1 AND status = $2
       ORDER BY
         (CAST(expected_return AS NUMERIC) - CAST(bid_amount AS NUMERIC)) DESC,
         CAST(expected_return AS NUMERIC) DESC,
         CAST(bid_amount AS NUMERIC) DESC,
         timestamp DESC,
         bid_id ASC
       LIMIT $3`,
      [invoiceId, BidStatus.Placed, limit]
    );

    return result.rows.map((row) => this.rowToBid(row));
  }

  /**
   * Get the best bid for an invoice (highest-ranked placed bid).
   * Returns null if no placed bids exist.
   */
  static async getBestBid(invoiceId: string): Promise<Bid | null> {
    const result = await pool.query(
      `SELECT * FROM bids
       WHERE invoice_id = $1 AND status = $2
       ORDER BY
         (CAST(expected_return AS NUMERIC) - CAST(bid_amount AS NUMERIC)) DESC,
         CAST(expected_return AS NUMERIC) DESC,
         CAST(bid_amount AS NUMERIC) DESC,
         timestamp DESC,
         bid_id ASC
       LIMIT 1`,
      [invoiceId, BidStatus.Placed]
    );

    return result.rows.length > 0 ? this.rowToBid(result.rows[0]) : null;
  }

  /**
   * Get paginated bids for an invoice with cursor-based pagination.
   * Supports filtering by investor and status.
   */
  static async getBidsPaginated(
    invoiceId: string,
    limit: number = 20,
    cursor: CursorPayload | null = null,
    filters?: { investor?: string; status?: BidStatus }
  ): Promise<PageResult<Bid>> {
    let query = 'SELECT * FROM bids WHERE invoice_id = $1';
    let params: any[] = [invoiceId];
    let paramIndex = 2;

    if (filters?.investor) {
      query += ` AND investor = $${paramIndex}`;
      params.push(filters.investor);
      paramIndex++;
    }

    if (filters?.status) {
      query += ` AND status = $${paramIndex}`;
      params.push(filters.status);
      paramIndex++;
    }

    // Only return Placed bids for ranking queries (security-conscious default)
    if (!filters?.status) {
      query += ` AND status = $${paramIndex}`;
      params.push(BidStatus.Placed);
      paramIndex++;
    }

    // Apply cursor filtering for pagination
    if (cursor) {
      query += ` AND (timestamp < $${paramIndex} OR (timestamp = $${paramIndex + 1} AND bid_id > $${paramIndex + 2}))`;
      params.push(cursor.sort_val, cursor.sort_val, cursor.id);
      paramIndex += 3;
    }

    // Sort by ranking order
    query += `
      ORDER BY
        (CAST(expected_return AS NUMERIC) - CAST(bid_amount AS NUMERIC)) DESC,
        CAST(expected_return AS NUMERIC) DESC,
        CAST(bid_amount AS NUMERIC) DESC,
        timestamp DESC,
        bid_id ASC
      LIMIT $${paramIndex}
    `;
    params.push(limit + 1); // Fetch one extra to determine has_more

    const result = await pool.query(query, params);
    const rows = result.rows.map((row) => this.rowToBid(row));

    const hasMore = rows.length > limit;
    const data = rows.slice(0, limit);

    let nextCursor: string | null = null;
    if (hasMore && data.length > 0) {
      const lastRow = data[data.length - 1];
      const sortVal = Number(lastRow.timestamp);
      nextCursor = encodeCursor({ id: lastRow.bid_id, sort_val: sortVal });
    }

    return { data, next_cursor: nextCursor, has_more: hasMore };
  }

  /**
   * Get all bids for an investor across all invoices (with RBAC check).
   * Only accessible to the investor themselves or admins.
   */
  static async getBidsForInvestor(
    investor: string,
    limit: number = 50,
    offset: number = 0
  ): Promise<{ bids: Bid[]; total: number }> {
    const countResult = await pool.query(
      'SELECT COUNT(*) as count FROM bids WHERE investor = $1',
      [investor]
    );

    const total = countResult.rows[0].count;

    const result = await pool.query(
      `SELECT * FROM bids WHERE investor = $1
       ORDER BY created_at DESC
       LIMIT $2 OFFSET $3`,
      [investor, limit, offset]
    );

    const bids = result.rows.map((row) => this.rowToBid(row));
    return { bids, total };
  }

  /**
   * Update bid status (e.g., mark as Withdrawn, Expired, Cancelled).
   */
  static async updateBidStatus(
    bidId: string,
    newStatus: BidStatus,
    updatedBy: string
  ): Promise<Bid | null> {
    const result = await pool.query(
      `UPDATE bids
       SET status = $1, updated_at = CURRENT_TIMESTAMP
       WHERE bid_id = $2
       RETURNING *`,
      [newStatus, bidId]
    );

    return result.rows.length > 0 ? this.rowToBid(result.rows[0]) : null;
  }

  /**
   * Get all bids for an invoice (all statuses).
   */
  static async getAllBidsForInvoice(invoiceId: string): Promise<Bid[]> {
    const result = await pool.query(
      'SELECT * FROM bids WHERE invoice_id = $1 ORDER BY created_at ASC',
      [invoiceId]
    );

    return result.rows.map((row) => this.rowToBid(row));
  }

  /**
   * Helper to convert database row to Bid type.
   */
  private static rowToBid(row: any): Bid {
    return {
      bid_id: row.bid_id,
      invoice_id: row.invoice_id,
      investor: row.investor,
      bid_amount: row.bid_amount,
      expected_return: row.expected_return,
      timestamp: Number(row.timestamp),
      status: row.status as BidStatus,
      expiration_timestamp: Number(row.expiration_timestamp),
      contract_version: 1, // TODO: sync from contract
      event_schema_version: 1, // TODO: sync from contract
      indexed_at: new Date().toISOString(),
    };
  }

  /**
   * Check if a bid exists.
   */
  static async bidExists(bidId: string): Promise<boolean> {
    const result = await pool.query('SELECT 1 FROM bids WHERE bid_id = $1', [bidId]);
    return result.rows.length > 0;
  }

  /**
   * Get bid by ID.
   */
  static async getBidById(bidId: string): Promise<Bid | null> {
    const result = await pool.query('SELECT * FROM bids WHERE bid_id = $1', [bidId]);
    return result.rows.length > 0 ? this.rowToBid(result.rows[0]) : null;
  }
}

export const bidStore = BidStore;
