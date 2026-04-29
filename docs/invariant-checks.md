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

For every `Paid` settlement, verifies that a matching `Accepted` bid exists and that `settlement.amount === bid.bid_amount` (BigInt comparison). Violations indicate a missed event or a corrupted write during indexing.

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

See `backend/src/tests/invariant.test.ts` for the full test suite covering:
- Clean data (no violations)
- Orphan bids, settlements, and disputes
- Cursor regressions at various positions
- Accounting mismatches (amount mismatch, missing accepted bid)
- `sampleIds` capping at 5
- BigInt overflow safety
- Backward-compat `getInvariantCounters()` shim
