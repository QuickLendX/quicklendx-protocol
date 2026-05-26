import pool from './database';
import { BestBidSnapshot, TopBidsSnapshot, BidEvent, TopBid } from '../types/snapshot';

export class SnapshotService {
  private static readonly TOP_BIDS_COUNT = 5;

  /**
   * Process a bidding event and update snapshots atomically
   */
  static async processBidEvent(event: BidEvent): Promise<void> {
    const client = await pool.connect();
    try {
      await client.query('BEGIN');

      if (event.event_type === 'BidWithdrawn') {
        await this.removeBidFromSnapshots(client, event.invoice_id, event.bid_id);
      } else {
        await this.updateBidInSnapshots(client, event);
      }

      await client.query('COMMIT');
    } catch (error) {
      await client.query('ROLLBACK');
      throw error;
    } finally {
      client.release();
    }
  }

  /**
   * Get the best bid for an invoice (O(1) retrieval)
   */
  static async getBestBid(invoiceId: string): Promise<BestBidSnapshot | null> {
    const result = await pool.query(
      'SELECT * FROM best_bids WHERE invoice_id = $1',
      [invoiceId]
    );
    return result.rows[0] || null;
  }

  /**
   * Get top bids for an invoice
   */
  static async getTopBids(invoiceId: string): Promise<TopBid[]> {
    const result = await pool.query(
      'SELECT top_bids FROM top_bids_snapshots WHERE invoice_id = $1',
      [invoiceId]
    );
    if (result.rows.length === 0) return [];
    return result.rows[0].top_bids;
  }

  /**
   * Validate snapshot consistency against raw events
   */
  static async validateSnapshot(invoiceId: string): Promise<boolean> {
    // This would compare the snapshot against a sum of events
    // For now, return true as placeholder
    return true;
  }

  /**
   * Rebuild snapshot from raw events (for recovery)
   */
  static async rebuildSnapshot(invoiceId: string): Promise<void> {
    // Implementation would fetch all events for invoice and rebuild
    // For now, placeholder
  }

  private static async updateBidInSnapshots(client: any, event: BidEvent): Promise<void> {
    // Update best bid if this bid is better
    await this.updateBestBid(client, event);

    // Update top bids list
    await this.updateTopBids(client, event);
  }

  private static async updateBestBid(client: any, event: BidEvent): Promise<void> {
    const currentBest = await client.query(
      'SELECT * FROM best_bids WHERE invoice_id = $1 FOR UPDATE',
      [event.invoice_id]
    );

    const newBid = {
      invoice_id: event.invoice_id,
      bid_id: event.bid_id,
      investor: event.investor,
      bid_amount: event.bid_amount,
      expected_return: event.expected_return,
      timestamp: event.timestamp,
      expiration_timestamp: event.expiration_timestamp,
      block_timestamp: event.block_timestamp,
      transaction_sequence: event.transaction_sequence,
      ledger_index: event.ledger_index,
      last_updated: Date.now(),
    };

    if (currentBest.rows.length === 0) {
      // No current best bid, insert this one
      await client.query(`
        INSERT INTO best_bids (
          invoice_id, bid_id, investor, bid_amount, expected_return,
          timestamp, expiration_timestamp, block_timestamp,
          transaction_sequence, ledger_index, last_updated
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
      `, [
        newBid.invoice_id, newBid.bid_id, newBid.investor, newBid.bid_amount,
        newBid.expected_return, newBid.timestamp, newBid.expiration_timestamp,
        newBid.block_timestamp, newBid.transaction_sequence, newBid.ledger_index,
        newBid.last_updated
      ]);
    } else {
      // Compare with current best
      const isBetter = this.compareBids(newBid, currentBest.rows[0]);
      if (isBetter) {
        await client.query(`
          UPDATE best_bids SET
            bid_id = $2, investor = $3, bid_amount = $4, expected_return = $5,
            timestamp = $6, expiration_timestamp = $7, block_timestamp = $8,
            transaction_sequence = $9, ledger_index = $10, last_updated = $11
          WHERE invoice_id = $1
        `, [
          newBid.invoice_id, newBid.bid_id, newBid.investor, newBid.bid_amount,
          newBid.expected_return, newBid.timestamp, newBid.expiration_timestamp,
          newBid.block_timestamp, newBid.transaction_sequence, newBid.ledger_index,
          newBid.last_updated
        ]);
      }
    }
  }

  private static async updateTopBids(client: any, event: BidEvent): Promise<void> {
    // Get current top bids
    const current = await client.query(
      'SELECT top_bids FROM top_bids_snapshots WHERE invoice_id = $1 FOR UPDATE',
      [event.invoice_id]
    );

    let topBids: TopBid[] = [];
    if (current.rows.length > 0) {
      topBids = current.rows[0].top_bids;
    }

    // Add or update the bid in the list
    const bidIndex = topBids.findIndex(b => b.bid_id === event.bid_id);
    const bid: TopBid = {
      bid_id: event.bid_id,
      investor: event.investor,
      bid_amount: event.bid_amount,
      expected_return: event.expected_return,
      timestamp: event.timestamp,
      expiration_timestamp: event.expiration_timestamp,
      rank: 0, // Will be set after sorting
    };

    if (bidIndex >= 0) {
      topBids[bidIndex] = bid;
    } else {
      topBids.push(bid);
    }

    // Sort by bid amount descending, then by tie-breakers
    topBids.sort((a, b) => {
      const amountA = BigInt(a.bid_amount);
      const amountB = BigInt(b.bid_amount);
      if (amountA !== amountB) {
        return amountB > amountA ? 1 : -1; // Descending
      }
      // Tie-breaker: earliest timestamp, then lowest sequence, then lowest ledger
      if (a.timestamp !== b.timestamp) return a.timestamp - b.timestamp;
      // Assuming we have sequence and ledger in the bid object
      return 0; // Placeholder
    });

    // Keep only top 5
    topBids = topBids.slice(0, this.TOP_BIDS_COUNT);

    // Update ranks
    topBids.forEach((b, index) => b.rank = index + 1);

    // Save back
    if (current.rows.length === 0) {
      await client.query(`
        INSERT INTO top_bids_snapshots (invoice_id, top_bids, last_updated)
        VALUES ($1, $2, $3)
      `, [event.invoice_id, JSON.stringify(topBids), Date.now()]);
    } else {
      await client.query(`
        UPDATE top_bids_snapshots SET top_bids = $2, last_updated = $3
        WHERE invoice_id = $1
      `, [event.invoice_id, JSON.stringify(topBids), Date.now()]);
    }
  }

  private static async removeBidFromSnapshots(client: any, invoiceId: string, bidId: string): Promise<void> {
    // Remove from best bid if it's the current best
    await client.query(
      'DELETE FROM best_bids WHERE invoice_id = $1 AND bid_id = $2',
      [invoiceId, bidId]
    );

    // Remove from top bids and resort
    const current = await client.query(
      'SELECT top_bids FROM top_bids_snapshots WHERE invoice_id = $1 FOR UPDATE',
      [invoiceId]
    );

    if (current.rows.length > 0) {
      let topBids: TopBid[] = current.rows[0].top_bids;
      topBids = topBids.filter(b => b.bid_id !== bidId);

      if (topBids.length === 0) {
        await client.query(
          'DELETE FROM top_bids_snapshots WHERE invoice_id = $1',
          [invoiceId]
        );
      } else {
        // Resort and update
        topBids.sort((a, b) => {
          const amountA = BigInt(a.bid_amount);
          const amountB = BigInt(b.bid_amount);
          if (amountA !== amountB) {
            return amountB > amountA ? 1 : -1;
          }
          return a.timestamp - b.timestamp;
        });
        topBids.forEach((b, index) => b.rank = index + 1);

        await client.query(`
          UPDATE top_bids_snapshots SET top_bids = $2, last_updated = $3
          WHERE invoice_id = $1
        `, [invoiceId, JSON.stringify(topBids), Date.now()]);
      }
    }
  }

  private static compareBids(newBid: any, currentBest: any): boolean {
    const newAmount = BigInt(newBid.bid_amount);
    const currentAmount = BigInt(currentBest.bid_amount);

    if (newAmount > currentAmount) return true;
    if (newAmount < currentAmount) return false;

    // Tie-breaker: earliest block timestamp
    if (newBid.block_timestamp < currentBest.block_timestamp) return true;
    if (newBid.block_timestamp > currentBest.block_timestamp) return false;

    // Then lowest transaction sequence
    if (newBid.transaction_sequence < currentBest.transaction_sequence) return true;
    if (newBid.transaction_sequence > currentBest.transaction_sequence) return false;

    // Then lowest ledger index
    return newBid.ledger_index < currentBest.ledger_index;
  }
}