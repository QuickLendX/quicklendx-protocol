export interface BestBidSnapshot {
  invoice_id: string;
  bid_id: string;
  investor: string;
  bid_amount: string;
  expected_return: string;
  timestamp: number;
  expiration_timestamp: number;
  block_timestamp: number; // For tie-breaking
  transaction_sequence: number; // For tie-breaking
  ledger_index: number; // For tie-breaking
  last_updated: number;
}

export interface TopBid {
  bid_id: string;
  investor: string;
  bid_amount: string;
  expected_return: string;
  timestamp: number;
  expiration_timestamp: number;
  rank: number;
}

export interface TopBidsSnapshot {
  invoice_id: string;
  top_bids: TopBid[];
  last_updated: number;
}

export interface BidEvent {
  event_type: 'BidPlaced' | 'BidUpdated' | 'BidWithdrawn';
  bid_id: string;
  invoice_id: string;
  investor: string;
  bid_amount: string;
  expected_return: string;
  timestamp: number;
  expiration_timestamp: number;
  block_timestamp: number;
  transaction_sequence: number;
  ledger_index: number;
}