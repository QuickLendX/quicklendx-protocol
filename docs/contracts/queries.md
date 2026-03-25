# Query Resilience

> **Module:** `quicklendx-contracts/src/lib.rs` — query endpoints
> **Tests:** `quicklendx-contracts/src/test_queries.rs`

---

## Overview

All query endpoints in the QuickLendX protocol are designed to handle missing
or non-existent records gracefully — returning `None`, an empty `Vec`, or a
typed `Err` rather than panicking or producing inconsistent results.

---

## Resilience Guarantees by Endpoint

| Endpoint | Missing record behaviour |
|---|---|
| `get_invoice(id)` | Returns `Err(InvoiceNotFound)` |
| `get_bid(id)` | Returns `None` |
| `get_investment(id)` | Returns `Err(StorageKeyNotFound)` |
| `get_invoice_investment(id)` | Returns `Err(StorageKeyNotFound)` |
| `get_bids_for_invoice(id)` | Returns empty `Vec` |
| `get_best_bid(id)` | Returns `None` |
| `get_ranked_bids(id)` | Returns empty `Vec` |
| `get_bids_by_status(id, status)` | Returns empty `Vec` |
| `get_bids_by_investor(id, investor)` | Returns empty `Vec` |
| `get_all_bids_by_investor(investor)` | Returns empty `Vec` |
| `get_business_invoices(business)` | Returns empty `Vec` |
| `get_investments_by_investor(investor)` | Returns empty `Vec` |
| `get_escrow_details(id)` | Returns `Err(StorageKeyNotFound)` |
| `get_bid_history_paged(id, ...)` | Returns empty `Vec` |
| `get_investor_bids_paged(investor, ...)` | Returns empty `Vec` |
| `cleanup_expired_bids(id)` | Returns `0` |

---

## Security Assumptions

- No query endpoint panics on missing input — all storage lookups use `Option`
  returns (`get` returning `None`) which are handled before unwrapping.
- Paginated endpoints cap results at `MAX_QUERY_LIMIT` (100) to prevent
  unbounded response sizes.
- Query endpoints are read-only and require no authorization — they cannot
  mutate state.
- Missing records never leak information about other records.

---

## Running Tests
```bash
cd quicklendx-contracts
cargo test test_queries
```
