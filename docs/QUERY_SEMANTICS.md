# QuickLendX Protocol — Query Semantics & Paged Read Guarantees

> [!IMPORTANT]
> **Target Audience:** Downstream Integrators, Indexer Service Developers, and Client API Engineers.
> This document describes the shared cursor format and guarantees for cursor-enabled list queries.

---

## 1. Overview of Read Semantics

Read-only entrypoints on QuickLendX smart contracts allow off-chain clients, indexers, and frontends to inspect protocol state without submitting mutating transactions.

Cursor-enabled queries use one versioned opaque cursor type, enforce the common page-size cap, and preserve each query's result ordering.

---

## 2. Core Guarantees for Paged Reads

| Semantic Dimension | Guarantee Provided | Behavior & Enforcement |
| :--- | :--- | :--- |
| **Snapshot Consistency** | **Single-Ledger Atomic** | Within a single RPC query invocation, all state storage accesses are executed against a frozen, consistent snapshot of the ledger at sequence $L$. |
| **Cross-Request Validation** | **Generation Bound** | The cursor carries the queried collection's generation. Mutations invalidate continuation and return `UnstableCursor`; historical ledger state is not served. |
| **Ordering Determinism** | **Query-Specific Stable Order** | Results preserve their query's deterministic order, including newest-first `created_at` order for business invoices. |
| **Query Hard Cap** | **Clamped to 50 Items** | No endpoint returns more than `MAX_QUERY_LIMIT = 50` items. Requests specifying `limit > 50` are automatically clamped to 50. |
| **Cursor Validation** | **Typed Rejection** | Malformed, unsupported-version, or modified cursors return `InvalidCursor`; stale generations return `UnstableCursor`. |

---

## 3. Snapshot Consistency vs. Multi-Page Concurrency Drift

### Single-Page Snapshot Guarantee
When calling a paged endpoint such as `get_business_invoices_paged(business, cursor = None, limit = 20)`:
- The entire evaluation occurs within ledger sequence $L$.
- The set of invoices returned, their statuses, and `has_more` represent a point-in-time atomic state.

### Generation Changes Between Pages
Soroban reads are stateless across RPC requests. If a mutation changes a
cursor-enabled query's collection between pages, the next request returns
`UnstableCursor` rather than attempting to continue at a shifted offset.
Discard the partial result and restart without a cursor.

### Recommended Integrator Mitigations
For full historical replication, index contract events rather than relying on
pagination across changing generations.

---

## 4. Resource Bounding & Safe Slice Math

Cursor-enabled query handlers decode the opaque cursor into an offset and then
use the internal `quicklendx_contracts::pagination` bounds helpers:

```
Requested: (offset, limit)
                │
                ▼
        cap_query_limit(limit) ──► Clamped to min(limit, 50)
                │
                ▼
    calculate_safe_bounds(offset, clamped_limit, total_count)
                │
                ▼
       [start_idx, end_idx) ──► Guaranteed 0 <= start <= end <= total_count
```

### Invariants Enforced by `pagination.rs`
1. `cap_query_limit(limit)` $\le 50$.
2. `start = min(offset, total_count)`.
3. `end = min(start + capped_limit, total_count)`.
4. `has_more = (start + effective_limit < total_count)`.

---

## 5. Concrete Entrypoint Specifications & Response Examples

### 1. `get_business_invoices_paged`

Retrieves all invoices created by a specific business address, sorted by creation sequence.

```rust
pub fn get_business_invoices_paged(
    env: Env,
    business: Address,
    status_filter: Option<InvoiceStatus>,
    cursor: Option<String>,
    limit: u32,
) -> Result<InvoicePage, QuickLendXError>
```

**Invocation Parameters**:
- `business`: `G...` (Stellar Address)
- `cursor`: `None` for the first page; otherwise pass through the prior
  response's exact `next_cursor`
- `limit`: `10`

The response carries `items`, `total_count`, `has_more`, `generation`, and an
optional `next_cursor`. Results preserve newest-first `created_at` order. A
mutation to the business invoice index invalidates the cursor with
`UnstableCursor`; restart from the first page.

**Response Output Structure**:
```json
{
  "items": [
    {
      "id": "0x1a2b3c...",
      "owner": "G...BUSINESS",
      "amount": 1000000000,
      "status": "Verified",
      "created_at": 1740000000
    }
  ],
  "total_count": 101,
  "has_more": true,
  "generation": 8,
  "next_cursor": "<opaque token>"
}
```

---

### 2. `get_ranked_bids_paged`

Retrieves all bids placed on a specific invoice, ordered deterministically by bid ranking (highest yield rate & tier first).

```rust
pub fn get_bids_for_invoice_paged(
    env: Env,
    invoice_id: BytesN<32>,
    cursor: Option<String>,
    limit: u32,
) -> Result<CursorBidsPage, QuickLendXError>
```

**Ordering Semantic**: Bids are ordered by deterministic ranking rules (`bid_ranking.rs`). Pagination preserves this exact ranking order across pages.

The cursor is bound to the invoice and the current ordered ranking snapshot.
Changes that affect active bids invalidate it with `UnstableCursor`.

---

### 3. `search_invoices_paged`

Filter & rank search across active invoices.

```rust
pub fn search_invoices_paged(
    env: Env,
    query: InvoiceSearchQuery,
    offset: u32,
    limit: u32,
) -> Result<(Vec<Invoice>, bool), QuickLendXError>
```

**Ordering Semantic**: Filtered search matches are sorted deterministically by match relevance rank, with tie-breaking by invoice ID string byte comparison.

---

## 6. Compilable Soroban SDK Code Example

The following Rust example demonstrates how to implement a safe paginated reader inside a custom Soroban client or contract:

```rust
use soroban_sdk::{contracttype, Address, Env, Vec};
use quicklendx_contracts::pagination::{cap_query_limit, calculate_safe_bounds, MAX_QUERY_LIMIT};
use quicklendx_contracts::errors::QuickLendXError;

/// Client helper: Safely iterates pages from a contract endpoint without panicking.
pub fn fetch_all_items_safely<T: Clone>(
    env: &Env,
    all_items: &Vec<T>,
    requested_offset: u32,
    requested_limit: u32,
) -> (Vec<T>, bool) {
    let total_count = all_items.len();
    
    // Step 1: Calculate safe bounds using protocol pagination logic
    let (start, end) = calculate_safe_bounds(requested_offset, requested_limit, total_count);
    
    let mut page_result = Vec::new(env);
    if start < end {
        for idx in start..end {
            if let Some(item) = all_items.get(idx) {
                page_result.push_back(item);
            }
        }
    }
    
    let effective_limit = end.saturating_sub(start);
    let has_more = start.saturating_add(effective_limit) < total_count;
    
    (page_result, has_more)
}
```

---

## Summary Cheat Sheet for Developers

- **Default Page Limit**: `50` (hard capped).
- **Page Offset**: 0-indexed (`offset = 0` means start of collection).
- **End of Results Indicator**: `has_more == false` or returned array length is `0`.
- **Concurrency Strategy**: Treat multi-page fetches as eventually consistent; deduplicate items client-side by entity ID.
