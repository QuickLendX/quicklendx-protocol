# QuickLendX Protocol — Query Semantics & Paged Read Guarantees

> [!IMPORTANT]
> **Target Audience:** Downstream Integrators, Indexer Service Developers, and Client API Engineers.
> This document specifies the exact read guarantees, ordering rules, snapshot consistency, cursor stability, and concurrency semantics for all paginated query entrypoints on the QuickLendX Soroban smart contracts.

---

## 1. Overview of Read Semantics

Read-only entrypoints on QuickLendX smart contracts allow off-chain clients, indexers, and frontends to inspect protocol state without submitting mutating transactions.

Every paginated query endpoint follows strict runtime invariants to guarantee deterministic execution, zero panic risk, and resource-bounded execution on the Soroban host.

---

## 2. Core Guarantees for Paged Reads

| Semantic Dimension | Guarantee Provided | Behavior & Enforcement |
| :--- | :--- | :--- |
| **Snapshot Consistency** | **Single-Ledger Atomic** | Within a single RPC query invocation, all state storage accesses are executed against a frozen, consistent snapshot of the ledger at sequence $L$. |
| **Cross-Request Isolation** | **Stateless Read Independent** | Each subsequent paged query (e.g., fetching Page 2 after Page 1) is an independent RPC call evaluated at the current ledger sequence $L'$ (where $L' \ge L$). |
| **Ordering Determinism** | **Stable Index Order** | Results are returned in deterministic index order (insertion sequence or explicit rank score). Page slicing never reorders or skips items within a single page slice. |
| **Query Hard Cap** | **Clamped to 50 Items** | No endpoint returns more than `MAX_QUERY_LIMIT = 50` items. Requests specifying `limit > 50` are automatically clamped to 50. |
| **Overflow Protection** | **Empty Slice (No Panic)** | If `offset >= total_count` or if `offset` approaches `u32::MAX`, endpoints return an empty list (`Vec::new()`) and `has_more = false`. |

---

## 3. Snapshot Consistency vs. Multi-Page Concurrency Drift

### Single-Page Snapshot Guarantee
When calling a paged endpoint such as `get_business_invoices_paged(business, offset = 0, limit = 20)`:
- The entire evaluation occurs within ledger sequence $L$.
- The set of invoices returned, their statuses, and `has_more` represent a point-in-time atomic state.

### Multi-Page Concurrency Drift & Cursor Anomalies
Because Soroban contract reads are **stateless across RPC requests**, state mutations can occur on-chain between fetching Page 1 at ledger $L_1$ and Page 2 at ledger $L_2$:

1. **Insertion Shift**:
   - If a new invoice is created by the business at $L_{1.5}$, item positions shift right.
   - *Result*: Fetching Page 2 with `offset = 20` at $L_2$ might re-read the last element of Page 1.
2. **Deletion / Status Transition Shift**:
   - If an invoice transitions status or is pruned between requests, item positions shift left.
   - *Result*: Fetching Page 2 might skip an item that shifted from index 20 to index 19.

### Recommended Integrator Mitigations
Downstream indexers and API clients **MUST** handle potential multi-page drift:
- **Client-Side Deduplication**: Key processed entities by unique ID (`invoice_id: BytesN<32>` or `bid_id: BytesN<32>`).
- **State Verification**: Re-verify record status or version if operating across multi-block indexing jobs.
- **Index-Based Ingestion**: For full historical replication, stream contract events (`emit_invoice_created`, `emit_bid_placed`) rather than polling paginated queries.

---

## 4. Resource Bounding & Safe Slice Math

All contract query handlers utilize the internal `quicklendx_contracts::pagination` module:

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
    offset: u32,
    limit: u32,
) -> Result<(Vec<Invoice>, bool), QuickLendXError>
```

**Invocation Parameters**:
- `business`: `G...` (Stellar Address)
- `offset`: `0` (First page)
- `limit`: `10`

**Response Output Structure**:
```json
[
  [
    {
      "id": "0x1a2b3c...",
      "owner": "G...BUSINESS",
      "amount": 1000000000,
      "status": "Verified",
      "created_at": 1740000000
    }
  ],
  true  // has_more flag indicating page 2 is available
]
```

---

### 2. `get_bids_for_invoice_paged`

Retrieves all bids placed on a specific invoice, ordered deterministically by bid ranking (highest yield rate & tier first).

```rust
pub fn get_bids_for_invoice_paged(
    env: Env,
    invoice_id: BytesN<32>,
    offset: u32,
    limit: u32,
) -> Result<(Vec<Bid>, bool), QuickLendXError>
```

**Ordering Semantic**: Bids are ordered by deterministic ranking rules (`bid_ranking.rs`). Pagination preserves this exact ranking order across pages.

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
