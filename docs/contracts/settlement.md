# Settlement Contract Flow

## Overview
QuickLendX settlement now supports full and partial invoice payments with durable on-chain payment records.

- Partial payments accumulate per invoice.
- Payment progress is queryable at any time.
- Applied payment amount is capped so `total_paid` never exceeds invoice `amount` (total due).
- Every applied payment is persisted as a dedicated payment record with payer, amount, timestamp, and nonce/tx id.

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

## Events
Settlement emits:

- `pay_rec` (PaymentRecorded): `(invoice_id, payer, applied_amount, total_paid, status)`
- `inv_stlf` (InvoiceSettled): `(invoice_id, final_amount, paid_at)`

Backward-compatible events still emitted:

- `inv_pp` (partial payment event)
- `inv_set` (existing settlement event)

## Security Considerations
- Replay/idempotency:
  - Non-empty nonce is enforced unique per invoice (`invoice_id`, `nonce`).
  - Duplicate nonce attempts are rejected with `OperationNotAllowed`.
  - The uniqueness guard executes before invoice totals or payment history are mutated, so rejected replays do not partially apply.
- Overpayment integrity:
  - Final settlement requires an exact remaining-due payment to avoid ambiguous excess-value handling.
  - Partial-payment capping still protects incremental repayment flows without allowing accounting drift.
- Arithmetic safety:
  - Checked arithmetic is used for payment accumulation and progress calculations.
  - Invalid/overflowing states reject with contract errors.
- Authorization:
  - Payer must be the invoice business owner and must authorize payment.
- Closed invoice protection:
  - Payments are rejected for `Paid`, `Cancelled`, `Defaulted`, and `Refunded` states.
- Invariant:
  - `total_paid <= total_due` is enforced.

## Vesting Validation Notes
The vesting flow also relies on ledger-time validation to keep token release schedules sane and reviewable.

- Schedule creation rejects zero-value vesting amounts.
- The creating caller must authorize and must be the configured protocol admin.
- `start_time` cannot be backdated relative to the current ledger timestamp.
- `end_time` must be strictly after `start_time`.
- `cliff_time = start_time + cliff_seconds` must not overflow and must be strictly before `end_time`.
- Release calculations reject impossible stored states such as `released_amount > total_amount` or timelines where `cliff_time` falls outside `[start_time, end_time)`.

These checks prevent schedules that would unlock immediately from stale timestamps, collapse into zero-duration timelines, or defer the entire vesting curve to an invalid cliff boundary.

## Vesting Admin Threat Model

### Admin Powers
The protocol admin holds exclusive power to:
1. **Create vesting schedules** — lock tokens and assign a beneficiary, cliff, and end time.
2. **Transfer the admin role** — hand off all admin powers (including vesting creation) to a new address.

No other address can create or modify schedules. Beneficiaries can only call `release_vested_tokens` on their own schedule.

### Threat Scenarios and Mitigations

| Threat | Mitigation |
|--------|-----------|
| Attacker creates a schedule to drain contract tokens | `require_auth` + `require_admin` gate `create_schedule`; non-admin calls are rejected |
| Admin creates a zero-value or backdated schedule | Input validation rejects `total_amount <= 0`, `start_time < now`, `end_time <= start_time`, `cliff_time >= end_time` |
| Admin creates a schedule with cliff == end (instant full unlock) | `cliff_time >= end_time` check rejects degenerate schedules |
| Beneficiary releases tokens before cliff | `release()` returns `InvalidTimestamp` if `now < cliff_time`; not a silent no-op |
| Beneficiary double-releases at the same timestamp | `released_amount` tracking makes repeated calls idempotent (`Ok(0)`) after full release |
| Beneficiary releases more than total | `released_amount` is checked against `total_amount` after each release; overflow uses checked arithmetic |
| Non-beneficiary releases someone else's tokens | `beneficiary` field compared to caller; mismatch returns `Unauthorized` |
| Admin transfers role; old admin retains vesting power | `require_admin` reads the live admin key; after transfer the old address fails the check |
| Arithmetic overflow in vesting calculation | `checked_mul` / `checked_add` / `checked_sub` used throughout; overflow returns `InvalidAmount` |
| Stale/invalid stored schedule state | `validate_schedule_state` re-checks invariants before every arithmetic operation |

### Not Mitigated
- **Compromised admin key**: A stolen admin key can create arbitrary schedules. Mitigate at the key-management layer (multisig, hardware wallet).
- **Consensus-level time manipulation**: Ledger timestamp is trusted as-is; extreme validator collusion could affect cliff/end boundaries.
- **Token contract bugs**: `transfer_funds` delegates to the token contract; a malicious token can re-enter or fail silently.

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
