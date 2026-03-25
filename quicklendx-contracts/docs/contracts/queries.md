## Bid History Pagination

### get_bid_history_paged

Returns paginated bid history for an invoice.

#### Features
- Supports status filtering
- Offset-based pagination
- Enforces MAX_QUERY_LIMIT

#### Guarantees
- No duplicate records between pages
- No skipped records
- Deterministic ordering

#### Security
- Prevents unbounded queries
- Safe bounds checking