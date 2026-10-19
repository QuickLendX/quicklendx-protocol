# QLX Report Lifecycle

This document describes the lifecycle of an analytics report in the QuickLendX protocol,
from generation request through on-chain delivery to long-term retention. Audience:
**contributors** who need to understand how `BusinessReport` and `InvestorReport`
are produced, stored, and retrieved.

## Phase Diagram

```
              ┌───────────┐
              │ Requested │  ◄─── generate_business_report / generate_investor_report called
              └─────┬─────┘
                    │  computation finishes; report stored on-chain
                    ▼
              ┌───────────┐
              │ Delivered │  ◄─── report persisted; returned to caller; retrievable by ID
              └─────┬─────┘
                    │  report remains immutable on-chain; no further transitions
                    ▼
              ┌───────────┐
              │ Archived  │  ◄─── terminal phase; report is a permanent historical snapshot
              └───────────┘
```

Once a report enters **Delivered** it is an irreversible, immutable snapshot. There is no
update, delete, recycle, or explicit "archive report" operation for individual reports.

## Phase Reference

| Phase        | Terminal? | Description |
|--------------|-----------|-------------|
| `Requested`  | No        | A transaction invokes a report entrypoint; the report exists only as in-flight computation. |
| `Delivered`  | Yes       | Report computed, stored to instance storage, returned from the entrypoint, and retrievable by `report_id`. |
| `Archived`   | Yes       | Conceptual retention phase for an older delivered snapshot. The contract does not record a separate archived flag or transition. |

## Report Types

Two report kinds share the same lifecycle:

| Report            | Source struct | Storage key prefix | Generation entrypoint |
|-------------------|---------------|--------------------|-----------------------|
| Business report   | `BusinessReport` | `biz_rpt`          | `generate_business_report` |
| Investor report   | `InvestorReport` | `inv_rpt`          | `generate_investor_report` |

Source: `BusinessReport` / `InvestorReport` in [`quicklendx-contracts/src/analytics.rs`](../quicklendx-contracts/src/analytics.rs).

## Entrypoints by Phase

### Requested

```rust
// Business
contract.generate_business_report(
    env, business: Address, period: analytics::TimePeriod,
) -> Result<analytics::BusinessReport, QuickLendXError>

// Investor
contract.generate_investor_report(
    env, investor: Address, period: analytics::TimePeriod,
) -> Result<analytics::InvestorReport, QuickLendXError>
```

- **Caller**: any transaction submitter. The current contract implementation does
  not call `require_auth()` inside these report entrypoints.
- **Precondition**: none specific to report generation beyond providing a valid
  `Address` and `TimePeriod`.
- **Effect**: report is computed from live on-chain state, assigned a fresh SHA-256
  `report_id`, persisted to instance storage, and returned to caller.
- **Authorization note**: the contract does not verify that the submitted
  `business` / `investor` address matches the transaction originator.

### Requested → Delivered

Transition is atomic within the entrypoint — there is no observable intermediate state.
The current implementation does **not** emit a dedicated report-generated event.

```rust
let report = contract.generate_business_report(env, business, period)?;
// or
let report = contract.generate_investor_report(env, investor, period)?;
// `report.report_id` can then be used for later retrieval.
```

### Delivered / Archived (read)

```rust
// Business
contract.get_business_report(env, report_id: BytesN<32>)
    -> Option<analytics::BusinessReport>

// Investor
contract.get_investor_report(env, report_id: BytesN<32>)
    -> Option<analytics::InvestorReport>
```

- **Caller**: any address (read-only, no auth required).
- **Precondition**: report exists with the given `report_id`.
- **Effect**: returns the stored report; no state change.
- **Missing report**: returns `None` (no panic).

## Storage Layout

Reports are stored in **instance storage** (permanent, not TTL-limited):

| Key                        | Value              |
|----------------------------|---------------------|
| `(Symbol("biz_rpt"), report_id)` | `BusinessReport`    |
| `(Symbol("inv_rpt"), report_id)` | `InvestorReport`    |

Source: `AnalyticsStorage` storage key functions in [`analytics.rs`](../quicklendx-contracts/src/analytics.rs).

Because instance storage entries persist until explicitly removed and the protocol does
not expose a delete-report entrypoint, every generated report is retained indefinitely
under normal operation. In this document, **Archived** means a delivered snapshot that is
being treated as historical data, not a distinct stored status.

## Key Invariants

1. **Reports are immutable after generation.** The same `report_id` always returns the
   same data. There is no update entrypoint.

2. **Each generation creates a new `report_id`.** `AnalyticsStorage::generate_report_id`
   combines `ledger.timestamp` and `ledger.sequence` through SHA-256, so two calls in
   the same ledger produce different IDs (sequence differs).

3. **No effective limit on stored reports.** Instance storage grows unboundedly with
   each generation call. Operators should manage growth through off-chain archival and
   avoid generating reports at high frequency in production.

4. **Report scope is caller-determined.** The `business` / `investor` address in the
   entrypoint parameters is *not* authenticated by the contract; callers should gate
   access off-chain when surfacing reports to end users.

## Time-Period Semantics

`generate_business_report` and `generate_investor_report` both accept a `TimePeriod`:

| Period     | Window (`start`, `end`)                 |
|------------|------------------------------------------|
| `Daily`    | `(now - 86400, now)`                     |
| `Weekly`   | `(now - 604800, now)`                    |
| `Monthly`  | `(now - 2592000, now)`                   |
| `Quarterly`| `(now - 7776000, now)`                   |
| `Yearly`   | `(now - 31536000, now)`                  |
| `AllTime`  | `(0, now)`                               |

All arithmetic uses `saturating_sub` so windows near ledger genesis collapse to `(0, now)`.

Source: `AnalyticsCalculator::get_period_dates` in [`analytics.rs`](../quicklendx-contracts/src/analytics.rs).

## Error Codes

| Error           | Code | Raised when |
|-----------------|------|-------------|
| `InvalidStatus` | 1401 | Internal investor-report validation detects `end_date < start_date`. Under the current `TimePeriod` calculation, this path is not expected to occur. |

Full error reference: [`docs/ERROR_CODES.md`](ERROR_CODES.md).

## Related Documentation

- [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) — invoice state machine that feeds report data.
- [`docs/STORAGE_LAYOUT.md`](STORAGE_LAYOUT.md) — on-chain storage key layout for all contract data.
- [`docs/QUERIES.md`](QUERIES.md) — read-only query entrypoints including report retrieval.
- [`docs/contracts/analytics.md`](contracts/analytics.md) — broader analytics data structures, API surface, and storage keys.
- [`quicklendx-contracts/src/analytics.rs`](../quicklendx-contracts/src/analytics.rs) — report struct definitions, storage, and calculation logic.
