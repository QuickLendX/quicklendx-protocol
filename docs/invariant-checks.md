# Invariant Check Suite

## Overview

The invariant check suite detects data integrity violations in the indexed DB. It runs three independent checks and surfaces violations without mutating any state, making it safe to execute periodically in production.

## Checks

### 1. Orphan Detection (`checkOrphans`)

Scans bids, settlements, and disputes for records whose `invoice_id` references a non-existent invoice. Also flags settlements with no corresponding bid entry.

| Violation | Meaning |
|-----------|---------|
| `orphanBids` | A `Bid` references an invoice not in the DB |
| `orphanSettlements` | A `Settlement` references an invoice not in the DB |
| `orphanDisputes` | A `Dispute` references an invoice not in the DB |
| `mismatchSettlements` | A `Settlement` exists but no `Bid` covers that `invoice_id` |

Each counter reports a `count` and up to 5 `sampleIds` for investigation.

### 2. Cursor Sequence (`checkCursorSequence`)

Verifies that a ledger cursor history is strictly monotonically increasing. A cursor that equals or is lower than its predecessor indicates a regression — either a duplicate replay or a rollback that was not followed by a full re-index.

```
cursors = [100, 200, 150]  →  regression at index 2: 150 ≤ 200
```

The function returns every regression with its `index`, `previous`, and `current` values.

### 3. Accounting Totals (`checkAccountingTotals`)

For every `Paid` settlement, verifies that a matching `Accepted` bid exists and that `settlement.amount === bid.bidAmount` (BigInt comparison). Violations indicate a missed event or a corrupted write during indexing.

Settlements with `Pending` or `Defaulted` status are skipped — only `Paid` settlements carry a final accounting commitment.

## Full Suite

`runFullInvariantSuite(provider, cursorHistory)` runs all three checks concurrently and returns a consolidated `FullInvariantReport`:

```typescript
{
  orphans: InvariantReport,       // orphan detection results
  cursorSequence: CursorRegressionReport,
  accounting: AccountingReport,
  timestamp: string,              // ISO 8601 run time
  pass: boolean,                  // true only if all sub-checks are clean
}
```

`pass` is `false` if any single violation is found anywhere across the three checks.

## Soroban Smart Contract Storage Invariants

The QuickLendX Soroban contracts maintain cross-module consistency between `InvoiceStorage`, `BidStorage`, `InvestmentStorage`, and `EscrowStorage`. The following invariants are validated after each lifecycle transition:

### One-Escrow-One-Investment Invariant

Every **Funded** invoice must have exactly **one** associated escrow record and exactly **one** associated investment record. This prevents:
- Double-funding via duplicate escrow creation
- Ghost investments pointing to non-existent invoices
- Stale escrow records after settlement/refund/default

**Validation:**
```rust
// After accept_bid_and_fund, verify:
assert!(client.get_escrow_details(&invoice_id).is_ok());
let inv = client.get_invoice_investment(&invoice_id).unwrap();
assert_eq!(inv.invoice_id, invoice_id);
```

### No-Orphan-Investment Invariant (`validate_no_orphan_investments`)

Every investment in the active index (`act_inv`) must have `status == Active`. Terminal-state investments (Completed, Defaulted, Refunded, Withdrawn) must be removed from the active index during their transition.

**Security Note:** A terminal investment remaining in the active index could allow re-settlement or other exploits.

### Status Index Count Invariance

The sum of `get_invoice_count_by_status(...)` across all statuses equals `get_total_invoice_count()`. Status indexes are updated atomically with invoice state transitions.

**Validation:**
```rust
let total = client.get_total_invoice_count();
let sum = client.get_invoice_count_by_status(&InvoiceStatus::Pending)
    + client.get_invoice_count_by_status(&InvoiceStatus::Verified)
    + client.get_invoice_count_by_status(&InvoiceStatus::Funded)
    + client.get_invoice_count_by_status(&InvoiceStatus::Paid)
    + client.get_invoice_count_by_status(&InvoiceStatus::Defaulted)
    + client.get_invoice_count_by_status(&InvoiceStatus::Cancelled)
    + client.get_invoice_count_by_status(&InvoiceStatus::Refunded);
assert_eq!(total, sum, "Invoice count invariant broken");
```

### Atomic Lifecycle Transitions

Each lifecycle transition (accept, refund, default, settle, cancel) updates **all** related modules atomically:

| Flow | Invoice Status | Bid Status | Investment Status | Escrow Status | Status Index |
|------|--------------|-----------|-------------------|---------------|------------|
| Accept bid | Pending → Verified → Funded | Placed → Accepted | None → Active | None → Held | Verified remove, Funded add |
| Refund | Funded → Refunded | Accepted → Cancelled | Active → Refunded | Held → Refunded | Funded remove, Refunded add |
| Default | Funded → Defaulted | - | Active → Defaulted | - | Funded remove, Defaulted add |
| Settle | Funded → Paid | - | Active → Completed | Held → Released | Funded remove, Paid add |
| Cancel bid | - | Placed → Cancelled | - | - | No module change |
| Withdraw bid | - | Placed → Withdrawn | - | - | No module change |

### Index-Drift Prevention

Status indexes are updated via explicit `remove_from_status_invoices` and `add_to_status_invoices` calls within each transition. The `StorageIntegrityAudit` module provides a full integrity check that can be called after any operation to verify:
- No orphan IDs in any status index
- Every indexed invoice exists in primary storage
- Every invoice's stored status matches its index membership

## Data Provider Interface

The suite decouples data access from check logic via `InvariantDataProvider`:

```typescript
interface InvariantDataProvider {
  getInvoices(): Promise<Invoice[]>;
  getBids(): Promise<Bid[]>;
  getSettlements(): Promise<Settlement[]>;
  getDisputes(): Promise<Dispute[]>;
}
```

For development and CI, use `createInMemoryProvider(invoices, bids, settlements, disputes)` to wrap static arrays. Production implementations should source from the real `InMemoryDerivedTableStore` or a database-backed store.

## Usage

```typescript
import {
  runFullInvariantSuite,
  createInMemoryProvider,
} from "./services/invariantService";

const provider = createInMemoryProvider(invoices, bids, settlements, disputes);
const cursorHistory = [/* ordered list of committed cursors */];
const report = await runFullInvariantSuite(provider, cursorHistory);

if (!report.pass) {
  console.error("Invariant violations detected:", report);
}
```

## Security

- All operations are **read-only** — no mutations.
- Results contain only IDs (no PII or sensitive financial data).
- `sampleIds` are capped at 5 to prevent large payloads from leaking bulk data.

## Tests

Soroban contract invariant tests are located in:
- `quicklendx-contracts/src/test_cross_module_consistency.rs` — Lifecycle transition tests with cross-module assertions
- `quicklendx-contracts/src/test_invariants.rs` — Status/index coherence and orphan detection tests

Run with:
```bash
cargo test test_cross_module --features legacy-tests
cargo test test_invariants --features legacy-tests
```

Tests cover:
- Clean data (no violations)
- Orphan bids, settlements, and investments
- Status index membership after each transition
- Count-index agreement across all statuses
- Lifecycle edge cases (cancel, withdraw, multiple bids, multi-invoice isolation)
