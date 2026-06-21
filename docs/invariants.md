# Invariant Checks Documentation

## Overview

QuickLendX invariant checks detect data integrity violations in the indexed database. They verify accounting correctness and referential integrity across invoices, bids, settlements, and disputes stores. These checks run on a configurable schedule as an active correctness guard.

## Concrete Invariants

### 1. Total Funded Equals Sum of Settlements

**Invariant:** For every invoice with status `Paid`, the total settlement amount must equal the accepted bid amount.

**Purpose:** Ensures that settlement transactions match the expected funding amounts, preventing underpayment or overpayment scenarios that indicate data corruption.

**Violation Detection:** 
- A `Paid` settlement exists without a corresponding `Accepted` bid for the same invoice
- Settlement amount differs from the accepted bid's `bid_amount`

### 2. No Settlement Exceeds Invoice Amount

**Invariant:** Settlement amounts must not exceed the original invoice principal.

**Purpose:** Prevents settlement amounts from exceeding what was originally invoiced, which could indicate fraudulent or corrupted data.

**Violation Detection:**
- Settlement `amount > invoice.amount` for any paid settlement

### 3. No Orphan Bids

**Invariant:** Every `Bid` record must reference an existing invoice.

**Purpose:** Ensures referential integrity between bids and invoices. Orphan bids indicate missed indexing events or data corruption.

**Violation Detection:**
- `Bid.invoice_id` does not match any invoice `id` in the store

### 4. No Orphan Settlements

**Invariant:** Every `Settlement` record must reference an existing invoice.

**Purpose:** Maintains referential integrity between settlements and invoices.

**Violation Detection:**
- `Settlement.invoice_id` does not match any invoice `id` in the store

### 5. No Orphan Disputes

**Invariant:** Every `Dispute` record must reference an existing invoice.

**Purpose:** Ensures all disputes are properly associated with invoices.

**Violation Detection:**
- `Dispute.invoice_id` does not match any invoice `id` in the store

### 6. Settlement Must Have Corresponding Bid

**Invariant:** Every settlement must have at least one bid for the same invoice.

**Purpose:** Settlements cannot exist without a corresponding bid, as they represent the execution of a financing agreement.

**Violation Detection:**
- `Settlement.invoice_id` does not match any bid `invoice_id` in the store

### 7. Cursor Sequence Monotonicity

**Invariant:** Ledger cursor history must be strictly monotonically increasing.

**Purpose:** Detects chain reorganizations or replay issues where cursors go backward or repeat.

**Violation Detection:**
- Cursor at position `i <= cursor at position i-1` in the history

## Scheduler Configuration

The scheduler runs invariant checks on a configurable cadence:

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `INVARIANT_SCHEDULE_INTERVAL_MS` | Interval between checks in milliseconds | 30000 (30 seconds) |
| `INVARIANT_CURSOR_HISTORY` | Comma-separated cursor history values | (empty) |

## API Endpoints

### GET /api/v1/invariants

Returns the latest invariant report or a message if no report is available yet.

**Response on scheduled run:**
```json
{
  "orphans": {
    "orphanBids": { "count": 0, "sampleIds": [] },
    "orphanSettlements": { "count": 0, "sampleIds": [] },
    "orphanDisputes": { "count": 0, "sampleIds": [] },
    "mismatchSettlements": { "count": 0, "sampleIds": [] },
    "timestamp": "2024-01-15T10:30:00.000Z"
  },
  "cursorSequence": {
    "hasRegression": false,
    "regressionCount": 0,
    "regressions": []
  },
  "accounting": {
    "mismatches": { "count": 0, "sampleIds": [] }
  },
  "pass": true,
  "timestamp": "2024-01-15T10:30:00.000Z"
}
```

### GET /api/v1/invariants/metrics

Returns the cumulative metrics from all invariant checks run by the scheduler.

**Response:**
```json
{
  "orphanBidsTotal": 0,
  "orphanSettlementsTotal": 0,
  "orphanDisputesTotal": 0,
  "mismatchSettlementsTotal": 0,
  "cursorRegressionsTotal": 0,
  "accountingMismatchesTotal": 0,
  "violationsDetectedTotal": 0,
  "checksRunTotal": 2
}
```

## Alerting

When violations are detected, the system emits two types of signals:

### Log Alert
A structured JSON log entry at ERROR level:
```json
{
  "level": "ALERT",
  "type": "INVARIANT_VIOLATION",
  "timestamp": "2024-01-15T10:30:00.000Z",
  "violations": ["orphan_bids: 1", "cursor_regression: 1"],
  "message": "Invariant violation detected: orphan_bids: 1, cursor_regression: 1"
}
```

### Metrics Counters
| Metric | Description |
|--------|-------------|
| `checksRunTotal` | Total number of invariant checks executed |
| `violationsDetectedTotal` | Number of check runs that found violations |
| `orphanBidsTotal` | Cumulative count of orphan bids found |
| `orphanSettlementsTotal` | Cumulative count of orphan settlements found |
| `orphanDisputesTotal` | Cumulative count of orphan disputes found |
| `mismatchSettlementsTotal` | Cumulative count of settlement/bid mismatches |
| `cursorRegressionsTotal` | Cumulative count of cursor regressions |
| `accountingMismatchesTotal` | Cumulative count of accounting mismatches |

## Security Considerations

1. **No PII in Output:** All invariant reports contain only IDs (`invoice_id`, `bid_id`, etc.) - never raw customer names, addresses, or tax IDs.

2. **Sample ID Capping:** The `sampleIds` arrays are capped at 5 items to prevent bulk data leakage.

3. **Read-Only Operations:** All invariant checks are read-only - no mutations to persistent state.

4. **Graceful Degradation:** If data stores are unavailable, the scheduler logs an error and continues without crashing.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     InvariantScheduler                           │
│  ┌──────────────────┐     ┌─────────────────────────────────┐   │
│  │   Timer (30s)     │────▶│ runFullInvariantSuite(provider)  │   │
│  └──────────────────┘     └─────────────────────────────────┘   │
│            │                         │                           │
│            ▼                         ▼                           │
│  ┌──────────────────┐     ┌─────────────────────────────────┐   │
│  │   Metrics        │     │    Report Store (in-memory)     │   │
│  │   Recording      │     └─────────────────────────────────┘   │
│  └──────────────────┘                    │                        │
│            │                             ▼                        │
│            │                 ┌──────────────────┐                 │
│            │                 │   Alert Emitter  │                 │
│            │                 │ (log + metric)   │                 │
│            │                 └──────────────────┘                 │
│            ▼                             ▼                        │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │                 monitoring.ts (/invariants)                 │   │
│  └─────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Usage in Production

```typescript
import { getInvariantScheduler, createInMemoryProvider } from "./services/invariantService";
import { derivedTableStore } from "./services/derivedTableStore";

// Set up provider from real data store
const provider = {
  getInvoices: () => derivedTableStore.listInvoices?.() || [],
  getBids: () => derivedTableStore.listBids?.() || [],
  getSettlements: () => derivedTableStore.listSettlements?.() || [],
  getDisputes: () => derivedTableStore.listDisputes?.() || [],
};

const scheduler = getInvariantScheduler();
scheduler.setProvider(provider);
scheduler.setCursorHistory(getRecentCursors());
scheduler.start(); // Uses INVARIANT_SCHEDULE_INTERVAL_MS or defaults to 30s
```

## Testing

Run the test suite:
```bash
npm test -- invariant.scheduled.test.ts
```

Key test scenarios covered:
- Scheduler lifecycle (start/stop)
- Clean state (no violations)
- Injected violations detection
- Graceful handling of store unavailability
- Counter reset functionality
- PII protection in output