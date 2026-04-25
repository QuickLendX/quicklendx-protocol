# Settlement Contract Flow

## Overview
QuickLendX settlement supports full and partial invoice payments with durable on-chain payment records and hardened finalization safety.

- Partial payments accumulate per invoice.
- Payment progress is queryable at any time.
- Applied payment amount is capped so `total_paid` never exceeds invoice `amount` (total due).
- Every applied payment is persisted as a dedicated payment record with payer, amount, timestamp, and nonce/tx id.
- Settlement finalization is protected against double-execution via a dedicated finalization flag.
- Disbursement invariant (`investor_return + platform_fee == total_paid`) is checked before fund transfer.

## State Machine
QuickLendX uses existing invoice statuses. For settlement:

- `Funded`: open for repayment; may have zero or more partial payments.
- `Paid`: terminal settled state after full repayment and distribution.
- `Cancelled`: terminal non-payable state.

Partial repayment is represented by:

- `status == Funded`
- `total_paid > 0`
- `progress_percent < 100`

## Storage Layout
Settlement storage in `src/settlement.rs` uses keyed records (no large single-value payment vector as source of truth):

- `PaymentCount(invoice_id) -> u32`
- `Payment(invoice_id, idx) -> SettlementPaymentRecord`
- `PaymentNonce(invoice_id, payer, nonce) -> bool`
- `Finalized(invoice_id) -> bool` — double-settlement guard flag

`SettlementPaymentRecord` fields:

- `payer: Address`
- `amount: i128` (applied amount)
- `timestamp: u64` (ledger timestamp)
- `nonce: String` (tx id / nonce)

Invoice fields used for progress:

- `amount` (total due)
- `total_paid`
- `status`

## Overpayment Behavior
Settlement and partial-payment paths intentionally behave differently:

- `process_partial_payment` safely bounds any excess request with `applied_amount = min(requested_amount, remaining_due)`.
- `settle_invoice` rejects explicit overpayment attempts with `InvalidAmount` unless the submitted amount exactly matches the remaining due.
- In both paths, `total_paid` can never exceed `amount`.

Accounting guarantees:

- Rejected settlement overpayments do not mutate invoice state, investment state, balances, or settlement events.
- Accepted final settlements emit `pay_rec` for the exact remaining due and `inv_stlf` for the final settled total.

## Finalization Safety

### Double-Settlement Protection
A dedicated `Finalized(invoice_id)` storage flag is set atomically during settlement finalization. Any subsequent settlement attempt (via `settle_invoice` or auto-settlement through `process_partial_payment`) is rejected immediately with `InvalidStatus`.

### Regression tests added

- `test_partial_payment_rejected_after_explicit_settlement` ensures explicit settlement blocks further partial payments and produces no side effects.
- `test_settlement_idempotency_no_side_effects` verifies repeated settlement attempts are rejected and cause no additional balance, event, or accounting changes.

### Accounting Invariant
Before disbursing funds, the settlement engine asserts:

```
investor_return + platform_fee == total_paid
```

If this invariant is violated (e.g., due to rounding errors in fee calculation), the settlement is rejected with `InvalidAmount`. This prevents any accounting drift between what the business paid and what gets disbursed.

### Payment Count Limit
Each invoice is limited to `MAX_PAYMENT_COUNT` (1,000) discrete payment records. This prevents unbounded storage growth and protects against payment-count overflow attacks.

## Public Query API

| Function | Signature | Description |
|----------|-----------|-------------|
| `get_invoice_progress` | `(env, invoice_id) -> Progress` | Aggregate settlement progress |
| `get_payment_count` | `(env, invoice_id) -> u32` | Total number of payment records |
| `get_payment_record` | `(env, invoice_id, index) -> SettlementPaymentRecord` | Single record by index |
| `get_payment_records` | `(env, invoice_id, from, limit) -> Vec<SettlementPaymentRecord>` | Paginated record slice (ordered ASC) |
| `is_invoice_finalized` | `(env, invoice_id) -> bool` | Whether settlement is complete |

## Events
Settlement emits:

- `pay_rec` (PaymentRecorded): `(invoice_id, payer, applied_amount, total_paid, status)`
- `inv_stlf` (InvoiceSettledFinal): `(invoice_id, final_amount, paid_at)`

## Security Considerations

### Replay/Idempotency
- Non-empty nonce is enforced unique per `(invoice, nonce)`.
- Duplicate payment attempts with the same nonce return the current `Progress` without mutating state (idempotent behavior).

### Ordering and Pagination
- Payments are stored with a strictly increasing index `[0..payment_count)`.
- `get_payment_records` returns records in chronological order (oldest first).
- Pagination supports arbitrary `from` offsets and enforces `MAX_QUERY_LIMIT` (100) per page.
- Duplicate nonce attempts are rejected with `OperationNotAllowed`.
- Nonces are scoped per invoice — the same nonce can be used on different invoices.

### Overpayment Integrity
- Final settlement requires an exact remaining-due payment to avoid ambiguous excess-value handling.
- Partial-payment capping protects incremental repayment flows without allowing accounting drift.

### Arithmetic Safety
- Checked arithmetic (`checked_add`, `checked_sub`, `checked_mul`, `checked_div`) is used for all payment accumulation and progress calculations.
- Invalid/overflowing states reject with contract errors.

### Authorization
- Payer must be the invoice business owner and must authorize payment.

### Closed Invoice Protection
- Payments are rejected for `Paid`, `Cancelled`, `Defaulted`, and `Refunded` states.

### Invariants
- `total_paid <= total_due` is enforced at every payment step.
- `investor_return + platform_fee == total_paid` is enforced at finalization.
- `payment_count <= MAX_PAYMENT_COUNT` (1,000) per invoice.

## Timestamp Consistency Guarantees
Settlement and adjacent lifecycle entrypoints enforce monotonic ledger-time assumptions to avoid
temporal anomalies when validators, simulation environments, or test harnesses move time backward.

- Guarded flows:
  - Create: invoice due date must remain strictly in the future (`due_date > now`).
  - Fund: funding entrypoints reject if `now < created_at`.
  - Settle: settlement rejects if `now < created_at` or `now < funded_at`.
  - Default: default handlers reject if `now < created_at` or `now < funded_at`.
- Error behavior:
  - Non-monotonic transitions fail with `InvalidTimestamp`.
- Data integrity assumptions:
  - `created_at` is immutable once written.
  - If present, `funded_at` must not precede `created_at`.
  - Lifecycle transitions rely only on ledger timestamp (not sequence number) for time checks.

### Threat Model Notes
- Mitigated:
  - Backward-time execution paths that could otherwise settle/default before a valid funding-time
    reference.
  - Cross-step inconsistencies caused by stale temporal assumptions.
  - Double-settlement via finalization flag.
  - Accounting drift via disbursement invariant check.
  - Unbounded storage via payment count limit.
- Not mitigated:
  - Consensus-level manipulation of canonical ledger time beyond protocol tolerance.
  - Misconfigured off-chain automation that never advances time far enough to pass grace windows.

## Running Tests
From `quicklendx-contracts/`:

```bash
cargo test test_partial_payments -- --nocapture
cargo test test_settlement -- --nocapture
```

## Vesting Validation Notes
The vesting flow also relies on ledger-time validation to keep token release schedules sane and reviewable.

- Schedule creation rejects zero-value vesting amounts.
- The creating caller must authorize and must be the configured protocol admin.
- `start_time` cannot be backdated relative to the current ledger timestamp.
- `end_time` must be strictly after `start_time`.
- `cliff_time = start_time + cliff_seconds` must not overflow and must be strictly before `end_time`.
- Release calculations reject impossible stored states such as `released_amount > total_amount` or timelines where `cliff_time` falls outside `[start_time, end_time)`.

These checks prevent schedules that would unlock immediately from stale timestamps, collapse into zero-duration timelines, or defer the entire vesting curve to an invalid cliff boundary.

## Timestamp Consistency Guarantees
Settlement and adjacent lifecycle entrypoints enforce monotonic ledger-time assumptions to avoid
temporal anomalies when validators, simulation environments, or test harnesses move time backward.

- Guarded flows:
  - Create: invoice due date must remain strictly in the future (`due_date > now`).
  - Fund: funding entrypoints reject if `now < created_at`.
  - Settle: settlement rejects if `now < created_at` or `now < funded_at`.
  - Default: default handlers reject if `now < created_at` or `now < funded_at`.
- Error behavior:
  - Non-monotonic transitions fail with `InvalidTimestamp`.
- Data integrity assumptions:
  - `created_at` is immutable once written.
  - If present, `funded_at` must not precede `created_at`.
  - Lifecycle transitions rely only on ledger timestamp (not sequence number) for time checks.

### Threat Model Notes
- Mitigated:
  - Backward-time execution paths that could otherwise settle/default before a valid funding-time
    reference.
  - Cross-step inconsistencies caused by stale temporal assumptions.
- Not mitigated:
  - Consensus-level manipulation of canonical ledger time beyond protocol tolerance.
  - Misconfigured off-chain automation that never advances time far enough to pass grace windows.

## Escrow Release Rules

The escrow release lifecycle follows a strict path to prevent premature or repeated release of funds.

### Release Conditions
- **Invoice Status**: Must be `Funded`. Release is prohibited for `Pending`, `Verified`, `Refunded`, or `Cancelled` invoices.
- **Escrow Status**: Must be `Held`. This ensures funds are only moved once.
- **Verification**: If an invoice is verified *after* being funded, the protocol can automatically trigger the release to ensure the business receives capital promptly.

### Idempotency and Retries
- The release operation is idempotent.
- Atomic Transfer: Funds move before the state update. If the transfer fails, the state is NOT updated, allowing for safe retries.
- Success Guard: Once status becomes `Released`, further attempts are rejected with `InvalidStatus`.

### Lifecycle Transitions
| Action | Invoice Status | Escrow Status | Result |
|--------|----------------|--------------|--------|
| `accept_bid` | `Verified` -> `Funded` | `None` -> `Held` | Funds locked in contract |
| `release_escrow` | `Funded` | `Held` -> `Released` | Funds moved to Business |
| `refund_escrow` | `Funded` -> `Refunded` | `Held` -> `Refunded` | Funds moved to Investor |
| `settle_invoice` | `Funded` -> `Paid` | `Released` | Invoice settled; Investor paid |

## Running Tests
From `quicklendx-contracts/`:

```bash
cargo test test_partial_payments -- --nocapture
cargo test test_settlement -- --nocapture
cargo test test_release_escrow_ -- --nocapture
```
