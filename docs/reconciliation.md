# Reconciliation Cycle

This document describes how the backend reconciliation cycle detects drift between the indexed store and on-chain state.

- The reconciliation worker enumerates indexed invoices from the derived table store (`derivedTableStore.listInvoices()`).
- It queries the canonical on-chain state via `rpcClient.call("getInvoices")`.
- For each on-chain invoice it checks:
  - MISSING: present on-chain but not in the index.
  - STATUS_MISMATCH: present in both but `status` differs.
  - DATA_MISMATCH: (reserved) other field-level mismatches.

Behavioral notes:
- The worker enforces a single-flight guard (`isRunning`) to prevent concurrent runs.
- RPC failures are captured and surfaced in the produced report's `error` field.
- The `triggerBoundedBackfill` honors `backfillBatchSize` to limit per-run corrections.

Monitoring:
- The monitoring API exposes the latest reconciliation report at `/v1/monitoring/reconciliation`.

Edge cases:
- Empty datasets: returns an empty report with zero checks.
- RPC failure mid-run: produces a report with `error` set and zero checks.
- Concurrent runs: second caller receives an error `Reconciliation already in progress`.

Security:
- RPC access is protected by host allow-list enforcement in `rpcClient.ts`.
