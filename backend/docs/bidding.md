# Bidding Snapshot System

## Overview

The Best-Bid snapshot system provides high-performance, O(1) retrieval of the best bid and top bids for invoices, eliminating the need for expensive runtime aggregations. The system maintains perfect synchronization with on-chain bidding events through an event-driven snapshot indexer.

## Architecture

### Components

1. **Snapshot Indexer**: Processes bidding events (BidPlaced, BidUpdated, BidWithdrawn) and updates snapshots atomically.
2. **BestBid Table**: Stores the current best bid per invoice with O(1) retrieval.
3. **TopBids Table**: Maintains a ranked list of top 5 bids per invoice.
4. **Validation Service**: Compares snapshots against raw event logs for consistency.
5. **Rebuild Service**: Recreates snapshots from event history in case of corruption.

### Database Schema

#### best_bids table
```sql
CREATE TABLE best_bids (
  invoice_id VARCHAR(64) PRIMARY KEY,
  bid_id VARCHAR(64) NOT NULL,
  investor VARCHAR(56) NOT NULL,
  bid_amount VARCHAR(32) NOT NULL,
  expected_return VARCHAR(32) NOT NULL,
  timestamp BIGINT NOT NULL,
  expiration_timestamp BIGINT NOT NULL,
  block_timestamp BIGINT NOT NULL,  -- For tie-breaking
  transaction_sequence BIGINT NOT NULL,  -- For tie-breaking
  ledger_index BIGINT NOT NULL,  -- For tie-breaking
  last_updated BIGINT NOT NULL
);
```

#### top_bids_snapshots table
```sql
CREATE TABLE top_bids_snapshots (
  invoice_id VARCHAR(64) PRIMARY KEY,
  top_bids JSONB NOT NULL,
  last_updated BIGINT NOT NULL
);
```

## Tie-Breaker Logic

When two bids have identical amounts, priority is determined by:

1. **Earliest block timestamp** (lowest value wins)
2. **Lowest transaction sequence** (within the block)
3. **Lowest ledger index**

## API Endpoints

### Get Best Bid
```
GET /api/v1/bids/best/:invoiceId
```
Returns the current best bid for the specified invoice.

**Response:**
```json
{
  "invoice_id": "0x...",
  "bid_id": "0x...",
  "investor": "GA...",
  "bid_amount": "1000000",
  "expected_return": "50000",
  "timestamp": 1640995200,
  "expiration_timestamp": 1641081600,
  "block_timestamp": 1640995200,
  "transaction_sequence": 1,
  "ledger_index": 12345,
  "last_updated": 1640995200
}
```

### Get Top Bids
```
GET /api/v1/bids/top/:invoiceId
```
Returns the top 5 bids for the specified invoice, ranked by bid amount and tie-breakers.

**Response:**
```json
{
  "top_bids": [
    {
      "bid_id": "0x...",
      "investor": "GA...",
      "bid_amount": "1000000",
      "expected_return": "50000",
      "timestamp": 1640995200,
      "expiration_timestamp": 1641081600,
      "rank": 1
    }
  ]
}
```

## Event Processing

The indexer processes events atomically using database transactions:

1. **BidPlaced/BidUpdated**: Compare with current best bid, update if better. Update top bids list.
2. **BidWithdrawn**: Remove from best bid (if it's the current best) and top bids list, then resort.

## Consistency & Recovery

### Validation
The `validateSnapshot()` method compares the current snapshot against a sum of raw events to detect divergence.

### Rebuilding
If corruption is detected, `rebuildSnapshot()` recreates the snapshot by replaying all events for the invoice.

## Performance Characteristics

- **Retrieval**: O(1) for best bid, O(1) for top bids (stored as JSONB)
- **Updates**: O(log n) for top bids sorting (n=5, effectively O(1))
- **Storage**: Minimal - one row per invoice for best bids, one row per invoice for top bids

## Testing

### Unit Tests
- Tie-breaker logic with various scenarios
- Event processing for all event types
- Snapshot retrieval methods

### Integration Tests
- API endpoints with mocked database
- Event processing with real database transactions

### Coverage Target: 95%

## Deployment

1. Run schema.sql to create tables
2. Set environment variables for database connection
3. Start processing events through the indexer
4. Monitor for consistency using validation utility

## Monitoring

- Track event processing latency
- Monitor snapshot validation results
- Alert on rebuild operations