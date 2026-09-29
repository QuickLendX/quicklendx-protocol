# Settlement State Machine

## Overview

Settlements track the lifecycle of debt repayment from origination to completion (or default). The state machine is driven by on-chain events (`InvoiceSettled`, `PaymentRecorded`) and persisted in SQLite.

## State Machine

```
┌─────────┐   InvoiceSettled   ┌─────────┐
│ Pending │ ◄────────────────── │ Pending │
└─────────┘                    └────┬────┘
                                    │
                           PaymentRecorded
                                    │
                                    ▼
                              ┌────────────┐
                              │ Processing  │
                              └──────┬─────┘
                                     │
                          ┌──────────┴──────────┐
                          │                     │
                     Confirm                Fail
                          │                     │
                          ▼                     ▼
                    ┌────────┐          ┌──────────┐
                    │  Paid  │          │ Defaulted│
                    └────────┘          └──────────┘
```

### States

| Status | Description | Terminal |
|--------|-------------|----------|
| `Pending` | Settlement created, awaiting payment | No |
| `Processing` | Payment processing in progress | No |
| `Paid` | Payment successfully completed | Yes |
| `Defaulted` | Payment failed / defaulted | Yes |

### Transitions

| From | To | Trigger | Event |
|------|----|---------|-------|
| `Pending` | `Processing` | Payment processing begins | `PaymentRecorded` |
| `Processing` | `Paid` | Payment confirmed | On-chain confirmation |
| `Processing` | `Defaulted` | Payment failed | Failure detection |

### Illegal Transitions

| From | To | Reason |
|------|----|--------|
| `Pending` | `Paid` | Must go through processing |
| `Pending` | `Defaulted` | Must attempt processing first |
| `Processing` | `Pending` | Cannot reverse |
| `Paid` | any | Terminal state |
| `Defaulted` | any | Terminal state |

## Persistence

Settlements are stored in the `settlements` table in SQLite (created by migration `v007_create_settlements.ts`).

### Table Schema

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT (PK) | UUID v4 |
| `invoice_id` | TEXT NOT NULL | Associated invoice |
| `amount` | TEXT NOT NULL | Settlement amount as string (i128 safe) |
| `payer` | TEXT NOT NULL | Payer Stellar address |
| `recipient` | TEXT NOT NULL | Recipient Stellar address |
| `timestamp` | INTEGER NOT NULL | Event timestamp |
| `status` | TEXT NOT NULL | Current state (`Pending`, `Processing`, `Paid`, `Defaulted`) |
| `contract_version` | INTEGER | Protocol version |
| `event_schema_version` | INTEGER | Event schema version |
| `indexed_at` | TEXT NOT NULL | ISO 8601 ingest timestamp |
| `created_at` | TEXT NOT NULL | Row creation time |
| `updated_at` | TEXT NOT NULL | Last update time |
| `event_id` | TEXT (UNIQUE) | Source event for idempotency |

**Indexes:**
- `idx_settlements_invoice` — filter by invoice
- `idx_settlements_status` — filter by status

## Idempotency

Each transition records the originating `event_id`. If the same `event_id` is replayed:
- The lookup finds the existing row at the target status → returns it (no-op)
- If the settlement has already moved past the target status → returns current state (stale event)
- If the `event_id` matches the current row's event_id → no-op

This guarantees safety under re-delivery of events from the indexer.

## Source Code

- **State machine + persistence**: `src/services/settlementOrchestrator.ts`
- **Migration**: `src/migrations/v007_create_settlements.ts`
- **Controller**: `src/controllers/v1/settlements.ts` (reads from orchestrator)
- **Event hooks**: `src/services/eventProcessor.ts` (triggers transitions)
- **Validators**: `src/validators/settlements.ts` (input validation)
- **Tests**: `src/tests/settlementOrchestrator.test.ts`

## Event Flow

1. `InvoiceSettled` event arrives → `eventProcessor.processInvoiceSettled()` creates a `Pending` settlement via `settlementOrchestrator.createPending()`
2. `PaymentRecorded` event arrives → `eventProcessor.processPaymentRecorded()` calls `startProcessing()` (Pending → Processing) then `completeProcessing()` (Processing → Paid)
3. On failure → `failProcessing()` transitions Processing → Defaulted
