-- Best Bids Snapshot Table
CREATE TABLE IF NOT EXISTS best_bids (
  invoice_id VARCHAR(64) PRIMARY KEY,
  bid_id VARCHAR(64) NOT NULL,
  investor VARCHAR(56) NOT NULL,
  bid_amount VARCHAR(32) NOT NULL,
  expected_return VARCHAR(32) NOT NULL,
  timestamp BIGINT NOT NULL,
  expiration_timestamp BIGINT NOT NULL,
  block_timestamp BIGINT NOT NULL,
  transaction_sequence BIGINT NOT NULL,
  ledger_index BIGINT NOT NULL,
  last_updated BIGINT NOT NULL
);

-- Top Bids Snapshot Table
CREATE TABLE IF NOT EXISTS top_bids_snapshots (
  invoice_id VARCHAR(64) PRIMARY KEY,
  top_bids JSONB NOT NULL,
  last_updated BIGINT NOT NULL
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_best_bids_invoice ON best_bids(invoice_id);
CREATE INDEX IF NOT EXISTS idx_top_bids_invoice ON top_bids_snapshots(invoice_id);