# QLX Bid-Matching Algorithm Specification

> **Audience:** Protocol Contributors & Smart Contract Developers  
> **Status:** Standard Specification  
> **Module:** `quicklendx-contracts/src/bid.rs` (`BidStorage::compare_bids`)

---

## 1. Overview & Purpose

In the QuickLendX invoice financing protocol, invoice bids are placed by investors offering capital (`bid_amount`) in exchange for a target payout (`expected_return`). When a business owner or automated settlement workflow selects a winning bid, the system must deterministically identify and order bids across all Stellar Soroban validators.

Non-deterministic ordering can lead to validator consensus failures or state divergence. To prevent this, QuickLendX implements a strict, **5-tier deterministic comparison algorithm** (`compare_bids`) that forms a total ordering over all active bids for a given invoice.

This document specifies the exact logic, Rust entrypoints, total order mathematical axioms, and worked examples for protocol contributors.

---

## 2. Target Data Structures

The algorithm operates on Soroban contract data structures defined in [`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs) and [`quicklendx-contracts/src/bid.rs`](../quicklendx-contracts/src/bid.rs).

```rust
use soroban_sdk::{Address, BytesN, Env, Vector};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BidStatus {
    Placed,
    Accepted,
    Cancelled,
    Withdrawn,
    Expired,
}

#[derive(Clone, Debug)]
pub struct Bid {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub status: BidStatus,
    pub expiration_timestamp: u64,
}
```

### Eligibility Invariant
Only bids with `status == BidStatus::Placed` are eligible for ranking and matching. Bids in status `Accepted`, `Cancelled`, `Withdrawn`, or `Expired` are filtered out prior to execution.

---

## 3. The 5-Tier Deterministic Comparison Hierarchy

The core comparator is implemented in `BidStorage::compare_bids` in [`quicklendx-contracts/src/bid.rs`](../quicklendx-contracts/src/bid.rs):

```rust
pub fn compare_bids(bid1: &Bid, bid2: &Bid) -> Ordering {
    let profit1 = bid1.expected_return.saturating_sub(bid1.bid_amount);
    let profit2 = bid2.expected_return.saturating_sub(bid2.bid_amount);
    if profit1 != profit2 {
        return profit1.cmp(&profit2);
    }
    if bid1.expected_return != bid2.expected_return {
        return bid1.expected_return.cmp(&bid2.expected_return);
    }
    if bid1.bid_amount != bid2.bid_amount {
        return bid1.bid_amount.cmp(&bid2.bid_amount);
    }
    if bid1.timestamp != bid2.timestamp {
        return bid1.timestamp.cmp(&bid2.timestamp);
    }
    if bid1.bid_id != bid2.bid_id {
        return bid1.bid_id.to_array().cmp(&bid2.bid_id.to_array());
    }
    Ordering::Equal
}
```

The comparator evaluates five fields sequentially, returning immediately upon finding the first non-equal condition:

| Tier | Property / Derived Value | Operator / Comparison | Rationale & Outcome |
| :---: | :--- | :--- | :--- |
| **1** | **Profit** (`expected_return.saturating_sub(bid_amount)`) | `profit1.cmp(&profit2)` | Maximizes financial benefit to the business. Higher profit wins. |
| **2** | **Expected Return** (`expected_return`) | `bid1.expected_return.cmp(&bid2.expected_return)` | Breaks profit ties by preferring larger overall total return. |
| **3** | **Bid Amount** (`bid_amount`) | `bid1.bid_amount.cmp(&bid2.bid_amount)` | Breaks secondary ties by preferring larger principal commitments. |
| **4** | **Ledger Timestamp** (`timestamp`) | `bid1.timestamp.cmp(&bid2.timestamp)` | Prefers more recent ledger timestamps when economic terms match. |
| **5** | **Bid Identifier** (`bid_id`) | `bid1.bid_id.to_array().cmp(&bid2.bid_id.to_array())` | Lexicographical 32-byte array comparison ensuring 100% stable tie-breaking across nodes. |

---

## 4. Mathematical Guarantees & Total Order Axioms

The `compare_bids` function defines a **strict total order** over all bids. This guarantees that sorting operations are stable and reproducible across all execution environments.

1. **Reflexivity**: For any bid $A$, `compare_bids(A, A) == Ordering::Equal`.
2. **Antisymmetry**: If `compare_bids(A, B) == Ordering::Greater`, then `compare_bids(B, A) == Ordering::Less`.
3. **Transitivity**: If `compare_bids(A, B) == Ordering::Greater` and `compare_bids(B, C) == Ordering::Greater`, then `compare_bids(A, C) == Ordering::Greater`.
4. **Totality**: For any two distinct bids $A$ and $B$, `compare_bids(A, B)` is strictly determined (never non-deterministic).

---

## 5. Contract Entrypoints & Invariants

The comparator is consumed by higher-level helper functions in `BidStorage`:

### 5.1 `get_best_bid`
Retrieves the single highest-ranked `Placed` bid for a given invoice:
```rust
pub fn get_best_bid(env: &Env, invoice_id: &BytesN<32>) -> Option<Bid>
```

### 5.2 `rank_bids`
Returns a `Vec<Bid>` of all `Placed` bids sorted from best (index 0) to worst:
```rust
pub fn rank_bids(env: &Env, invoice_id: &BytesN<32>) -> Vec<Bid>
```

### Core System Invariant
$$\text{rank\_bids}(env, \text{invoice\_id})[0] \equiv \text{get\_best\_bid}(env, \text{invoice\_id})$$

If `rank_bids` returns a non-empty vector, the element at index `0` is guaranteed to match `get_best_bid`.

---

## 6. Worked Concrete Example

Consider four bids placed on invoice `INV-100`:

| Bid | `bid_amount` | `expected_return` | Derived Profit | `timestamp` | `bid_id` (suffix) |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Bid A** | `1,000` | `1,250` | `250` | `100` | `0x...0001` |
| **Bid B** | `1,000` | `1,300` | `300` | `105` | `0x...0002` |
| **Bid C** | `2,000` | `2,300` | `300` | `110` | `0x...0003` |
| **Bid D** | `2,000` | `2,300` | `300` | `110` | `0x...0004` |

### Step-by-Step Resolution

1. **Tier 1 (Profit)**:
   - Bid B (`300`), Bid C (`300`), and Bid D (`300`) beat Bid A (`250`). Bid A is ranked 4th.
2. **Tier 2 (Expected Return)**:
   - Between B (`1,300`), C (`2,300`), and D (`2,300`), C and D beat B on expected return. Bid B is ranked 3rd.
3. **Tier 3 (Bid Amount)**:
   - Between C (`2,000`) and D (`2,000`), bid amounts tie.
4. **Tier 4 (Timestamp)**:
   - Both C (`110`) and D (`110`) have identical timestamps.
5. **Tier 5 (Bid ID Lexicographical Tiebreaker)**:
   - `0x...0004` (Bid D) > `0x...0003` (Bid C). Bid D wins 1st place!

**Final Ranked Order**: `[Bid D, Bid C, Bid B, Bid A]`.

---

## 7. Related Documentation & References

- [`docs/BID_RANKING.md`](BID_RANKING.md) — Comprehensive bid ranking system documentation.
- [`docs/BID_LIFECYCLE_DIAGRAM.md`](BID_LIFECYCLE_DIAGRAM.md) — Full bid state machine and lifecycle specification.
- [`quicklendx-contracts/src/bid.rs`](../quicklendx-contracts/src/bid.rs) — Smart contract source code for `compare_bids`.
- [`quicklendx-contracts/src/test_bid_compare_order_props.rs`](../quicklendx-contracts/src/test_bid_compare_order_props.rs) — Property-based tests verifying total order guarantees.
