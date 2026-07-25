# Batch Operations and Transaction Guarantees

This document describes the execution semantics and guarantees of batch operations across the QuickLendX system. It is written for **contributors** modifying or interacting with the smart contracts, event ingestion pipelines, or backend background jobs.

---

## 1. On-Chain Smart Contract Operations (All-or-Nothing Atomicity)

Batch operations in the smart contracts are executed within a single Stellar/Soroban transaction. If any validation fails or any individual item is invalid, the entire transaction is rolled back, and no state changes are persisted to ledger storage.

### Example: Whitelisting Currencies

The `add_currencies_batch` function allows the admin to register multiple token contract addresses.

#### Entrypoint Signature
```rust
pub fn add_currencies_batch(
    env: &Env,
    admin: &Address,
    currencies: &Vec<Address>,
) -> Result<Vec<bool>, QuickLendXError>;
```

#### Behavior & Guarantees
- **Atomic Rollback**: If any address in the `currencies` list is a duplicate of the admin, the zero address, or the contract itself, the entire transaction aborts with `QuickLendXError::InvalidCurrency`. No tokens from the batch are added.
- **Deduplication**: If some tokens are already whitelisted, they return `false` in the results vector, while newly added tokens return `true`.

```rust
// In quicklendx-contracts/src/currency.rs:
for currency in currencies.iter() {
    if currency == *admin || currency == zero || currency == contract_addr {
        return Err(QuickLendXError::InvalidCurrency); // Entire transaction reverts
    }
}
```

---

## 2. Backend Event Ingestion (Partial Progress / Best Effort)

The backend ingestion layer handles batches of events emitted by on-chain contracts. Because events are independent, the ingestion pipeline allows partial progress.

### Ingestion Pipeline
- **Validation**: If a batch size exceeds `100` events, the entire batch is rejected.
- **Best Effort**: For valid batch sizes, each event is processed individually. If an event is a duplicate or fails validation, it is marked as failed, but processing continues for other events in the same batch.
- **Response**: The API returns an HTTP 400 status if any item was rejected, along with a per-event `results` array to let client applications know which specific events succeeded or failed.

#### Example Response Body
```json
{
  "status": "error",
  "message": "Some events in the batch could not be processed",
  "results": [
    { "event_id": "0000000001-0001", "status": "processed" },
    { "event_id": "0000000001-0002", "status": "duplicate", "error": "Event already indexed" },
    { "event_id": "0000000001-0003", "status": "processed" }
  ]
}
```

---

## 3. Reconciliation and Drift Backfill (Resumable Bounded Batches)

For background maintenance tasks that process large populations of data (such as fixing index drift or performing backfills), QuickLendX uses bounded batches with cursor-based resumption.

### Drift Backfill
The backfill job processes `DriftItem` records in chunks up to `backfillBatchSize` to prevent hitting database lock timeouts or resource depletion.

- **Checkpointing**: After each batch of items is successfully processed, the job updates the `last_processed_id` cursor and the `remaining_count` atomically in the database.
- **Crash Recovery**: If the service crashes mid-batch, the next run reads the persisted cursor and resumes from the start of the last uncommitted batch. It does not restart the entire backfill from scratch.

#### Invariant Tracking
```
[Start Backfill]
       │
       ▼
 ┌───────────┐
 │ Fetch 10  │ ◄─────────────────────────┐
 └─────┬─────┘                           │
       ▼                                 │
 ┌───────────┐                           │
 │ Process   ├─────► [Crash occurs]      │
 └─────┬─────┘             │             │
       ▼                   ▼             │
 ┌───────────┐       [Restart job]       │
 │ Update    │             │             │
 │ Cursor    │             ▼             │
 └─────┬─────┘       [Read cursor from]  │
       ▼             [last success id ] ─┘
 [Finished]
```
