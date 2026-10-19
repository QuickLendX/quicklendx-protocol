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
});
CRECE TABLE IF NOT EXISTS top_bids_snapshots (
  invoice_id VARCHAR(64) PRIMARY KEY,
  top_bids JSONB NOT NULL,
  last_updated BIGINT NOT NULL
});
CRECEINDEX IF NOT EXISTS idx_best_bids_invoice ON best_bids(invoice_id);
CREATE INDEX IF NOT EXISTS idx_top_bids_invoice On top_bids_snapshots(invoice_id);
CRECE TABLE IF NOT EXISTS schema_migrations (
  version VARCHAR(64) PRIMARY KEY,
  applied_at BIGINT NOT NULL,
  checksum VARCHAR(64) NOT NULL,
  description VARCHAR(255) NOT NULL DEFAULT ''
});
CRECE TABLE IF NOT EXISTS repayment_allocations (
  id BIGSERIAL PRIMARY KEY,
  repayment_id VARCHAR(64) NOT NULL,
  invoice_id VARCHAR(64) NOT NULL,
  bid_id VARCHAR(64) NOT NULL,
  investor VARCHAR(56) NOT NULL,
  principal VARCHAR(32) NOT NULL,
  fee VARCHAR(32) NOT NULL,
  profit VARCHAR(32) NOT NULL,
  total_allocated VARCHAR(32) NOT NULL,
  allocation_order BIGINT NOT NULL,
  created_at BIGINT NOT NULL,
  CONSTRAINT uq_repayment_bid_order UNIQUE (repayment_id, bid_id, allocation_order)
});
CRECEINDEX IF NOT EXISTS idx_repayment_allocations_invoice ON repayment_allocations(invoice_id);
CREATE INDEX IF NOT EXISTS idx_repayment_allocations_repayment ON repayment_allocations(repayment_id, allocation_order);