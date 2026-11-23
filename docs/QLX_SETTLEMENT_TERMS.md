# QLX Settlement Terms

> **Audience:** Contract contributors who need to understand how settlement terms
> are represented on an invoice, how payments accumulate to trigger finalization,
> and what the contract stores on-chain at each step.

Settlement is the process by which a funded invoice moves from `Funded` to `Paid`.
The business makes one or more payments; once the accumulated total reaches the
invoice face value, the contract finalizes settlement, distributes funds, and
marks the invoice terminal.

This document explains every field, storage key, entrypoint, and invariant involved.

---

## 1. Settlement fields on the Invoice struct

All settlement-relevant state for a live invoice lives in `types::Invoice`:

```rust
pub struct Invoice {
    // --- face value / funding ---
    pub amount: i128,         // Invoice face value the business must repay
    pub funded_amount: i128,  // Amount escrowed by the investor at funding time
    pub funded_at: Option<u64>,

    // --- payment accumulation ---
    pub total_paid: i128,     // Cumulative sum of all applied partial payments
    pub payment_history: Vec<PaymentRecord>, // Inline history, capped at 32 entries

    // --- status ---
    pub status: InvoiceStatus,   // Funded → Paid (terminal)
    pub dispute_status: DisputeStatus,
    pub settled_at: Option<u64>,
    // ...
}
```

| Field | Type | Role |
|---|---|---|
| `amount` | `i128` | The target total the business must pay to settle |
| `funded_amount` | `i128` | Funds currently locked in escrow (≤ `amount`) |
| `total_paid` | `i128` | Running total of all accepted payments; capped at `amount` |
| `payment_history` | `Vec<PaymentRecord>` | Last ≤ 32 payments stored inline; older entries are evicted |
| `status` | `InvoiceStatus` | Must be `Funded` before any payment is accepted |
| `dispute_status` | `DisputeStatus` | `Disputed` or `UnderReview` blocks finalization |
| `settled_at` | `Option<u64>` | Ledger timestamp set when invoice transitions to `Paid` |

The inline `payment_history` is a sliding window — it never grows past 32 entries.
Full durable records are written to a separate per-payment storage key (see §2).

### PaymentRecord (inline)

```rust
pub struct PaymentRecord {
    pub amount: i128,
    pub payer: Address,
    pub timestamp: u64,
    pub transaction_id: String,  // nonce supplied by the caller
}
```

---

## 2. Durable per-payment storage

Every accepted payment also writes a `SettlementPaymentRecord` to persistent
Soroban storage, keyed by `(invoice_id, payment_index)`. These records survive
beyond the inline 32-entry cap and are queryable by index.

### Storage keys

```rust
enum SettlementDataKey {
    PaymentCount(BytesN<32>),           // total payments recorded so far
    Payment(BytesN<32>, u32),           // individual record at index i
    PaymentNonce(BytesN<32>, String),   // dedup flag per (invoice, nonce)
    Finalized(BytesN<32>),              // boolean; set once at finalization
}
```

### SettlementPaymentRecord

```rust
pub struct SettlementPaymentRecord {
    pub payer: Address,   // invoice.business at call time
    pub amount: i128,     // actual applied amount (may be capped)
    pub timestamp: u64,   // env.ledger().timestamp()
    pub nonce: String,    // caller-supplied transaction_id
}
```

### Limits

| Constant | Value | Enforced by |
|---|---|---|
| `MAX_PAYMENT_COUNT` | 1 000 | `record_payment` — returns `OperationNotAllowed` at cap |
| `MAX_INLINE_PAYMENT_HISTORY` | 32 | `update_inline_payment_history` — oldest entry evicted |
| `MAX_SETTLEMENT_BATCH_SIZE_SOFT_CAP` | 50 | `get_payment_records` — hard query limit |

---

## 3. Settlement entrypoints

### 3.1 `process_partial_payment`

```rust
pub fn process_partial_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    transaction_id: String,  // caller-supplied nonce for deduplication
) -> Result<(), QuickLendXError>
```

This is the primary business-facing entrypoint. It:

1. Loads the invoice and verifies `status == Funded` (and no active dispute).
2. Caps `payment_amount` to `remaining_due` — the contract never accepts overpayment.
3. Writes a `SettlementPaymentRecord` at `Payment(invoice_id, payment_count)`.
4. Increments `PaymentCount`.
5. Marks the nonce `PaymentNonce(invoice_id, transaction_id) = true` (replay guard).
6. Updates `invoice.total_paid` and appends to the inline `payment_history`.
7. If `total_paid >= amount`, triggers `settle_invoice_internal` automatically.

**Authorization**: `invoice.business.require_auth()` — only the business itself
may record payments.

Concrete test pattern:

```rust
// invoice_id, business funded at 10_000 USDC
client.process_partial_payment(
    &invoice_id,
    &4_000_i128,
    &String::from_str(&env, "tx-0001"),
);
// → total_paid == 4_000, remaining_due == 6_000, status still Funded

client.process_partial_payment(
    &invoice_id,
    &6_000_i128,
    &String::from_str(&env, "tx-0002"),
);
// → total_paid == 10_000 == amount → auto-triggers settlement → status Paid
```

### 3.2 `settle_invoice`

```rust
pub fn settle_invoice(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
) -> Result<(), QuickLendXError>
```

Explicit full-settlement entrypoint. Useful when the business wants to pay the
exact remaining balance in a single call rather than using the partial-payment
path. The payment amount must equal `remaining_due`; over-payment is rejected.

Internally this calls `record_payment` and then `settle_invoice_internal`.

### 3.3 `get_invoice_progress`

```rust
pub struct Progress {
    pub total_due: i128,
    pub total_paid: i128,
    pub remaining_due: i128,
    pub progress_percent: u32,  // 0–100
    pub payment_count: u32,
    pub status: InvoiceStatus,
}

pub fn get_invoice_progress(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<Progress, QuickLendXError>
```

The canonical read entrypoint for settlement progress. Returns a point-in-time
snapshot: all fields are derived from `invoice.amount`, `invoice.total_paid`,
and the durable `PaymentCount` key.

```rust
let progress = client.get_invoice_progress(&invoice_id)?;
assert_eq!(progress.total_due, 10_000);
assert_eq!(progress.total_paid, 4_000);
assert_eq!(progress.remaining_due, 6_000);
assert_eq!(progress.progress_percent, 40);
assert_eq!(progress.payment_count, 1);
```

### 3.4 Payment history queries

```rust
// Total durable records (O(1))
fn get_payment_count(env, invoice_id) -> Result<u32, _>

// Single record by index (O(1))
fn get_payment_record(env, invoice_id, index: u32)
    -> Result<SettlementPaymentRecord, _>

// Paginated slice, chronological order (index 0 = first payment)
fn get_payment_records(env, invoice_id, from: u32, limit: u32)
    -> Result<Vec<SettlementPaymentRecord>, _>
// limit is clamped to MAX_SETTLEMENT_BATCH_SIZE_SOFT_CAP (50)

// Finalization check (O(1))
fn is_invoice_finalized(env, invoice_id) -> Result<bool, _>
```

---

## 4. Settlement formula

When `total_paid >= amount`, `settle_invoice_internal` runs the following
calculation to split funds between the investor and the platform. The source
of truth is `profits::PlatformFee::calculate`.

```text
gross_profit     = max(0, total_paid − investment.amount)
platform_fee     = floor(gross_profit × fee_bps / 10_000)
investor_return  = total_paid − platform_fee
```

**No-dust invariant** (asserted before any transfer):

```text
investor_return + platform_fee == total_paid
```

Because `platform_fee` uses floor division, any fractional remainder stays with
the investor. The assertion `disbursement_total != invoice.total_paid` causes a
hard revert if the identity breaks.

### Worked example

| Variable | Value |
|---|---|
| `investment.amount` | 10 000 USDC |
| `invoice.amount` (= `total_paid` at finalization) | 11 000 USDC |
| `fee_bps` | 200 (2%) |

```text
gross_profit    = 11_000 − 10_000 = 1_000
platform_fee    = floor(1_000 × 200 / 10_000) = 20
investor_return = 11_000 − 20 = 10_980
```

Verification: `10_980 + 20 = 11_000`. ✓

If there is no profit (`total_paid <= investment.amount`):

```text
platform_fee    = 0
investor_return = total_paid
```

---

## 5. Finalization sequence

When `total_paid >= invoice.amount`, `settle_invoice_internal` performs the
following steps in order:

1. **Double-settle guard**: checks `Finalized(invoice_id)` — rejects if already set.
2. **Dispute guard** (via `ensure_payable_status`): rejects if `dispute_status`
   is `Disputed` or `UnderReview`.
3. **Auto-release escrow**: if `escrow.status == Held`, calls `release_escrow`
   to return the funded amount to the business.
4. **Fee calculation**: calls `FeeManager::calculate_platform_fee`; falls back to
   `profits::calculate_profit` for environments without fee config.
5. **No-dust assertion**: `investor_return + platform_fee == total_paid`.
6. **Transfers**:
   - `investor_return` → investor address
   - `platform_fee` → treasury (via `FeeManager::route_platform_fee`)
7. **Mark finalized**: sets `Finalized(invoice_id) = true`.
8. **Status transition**: `invoice.status = Paid`, `investment.status = Completed`.
9. **Index updates**: removes invoice from old status index, adds to `Paid` index.
10. **Events**: emits `InvoiceSettled`, `inv_stlf`, and `PlatformFeeRouted`.
11. **Notifications**: triggers `InvoiceStatusChanged` for downstream consumers.

---

## 6. Dispute interaction

Settlement is **blocked** while a dispute is active. `ensure_payable_status`
returns `InvalidStatus` when:

```rust
invoice.dispute_status == DisputeStatus::Disputed
    || invoice.dispute_status == DisputeStatus::UnderReview
```

However, `record_payment` continues to accept and persist payments during a
dispute, so `total_paid` may reach `invoice.amount` without triggering finalization.
Finalization remains blocked until the dispute is resolved.

| `dispute_status` | Partial payments accepted | Finalization allowed |
|---|---|---|
| `None` | ✓ | ✓ (if `status == Funded`) |
| `Disputed` | ✓ | ✗ |
| `UnderReview` | ✓ | ✗ |
| `Resolved` | ✓ | ✓ |

Resolution paths:
- **Favor investor**: invoice transitions to `Cancelled`/`Refunded` — escrow
  refunded, settlement permanently blocked.
- **Favor business**: invoice returns to `Funded` — business completes remaining
  payments, settlement proceeds normally.

See [`docs/SETTLEMENT.md`](SETTLEMENT.md) and
[`docs/contracts/settlement-dispute-interaction.md`](../quicklendx-contracts/docs/settlement-dispute-interaction.md)
for the full state machine.

---

## 7. Invariants to verify when changing settlement code

Before merging any change that touches `settlement.rs`, `profits.rs`, or
`types::Invoice`, verify these invariants hold:

1. `total_paid <= invoice.amount` — enforced in `record_payment` by the overpayment
   cap: `applied_amount = min(requested, remaining_due)`.
2. `total_paid` is monotonically non-decreasing — only `checked_add` is used;
   there is no decrement path.
3. `payment_count <= MAX_PAYMENT_COUNT` — enforced at the top of `record_payment`.
4. `investor_return + platform_fee == total_paid` — asserted before transfer in
   `settle_invoice_internal`; a mismatch returns `InvalidAmount` and reverts.
5. Finalization is idempotent — `Finalized` flag is checked before and set
   atomically during `settle_invoice_internal`.
6. Settlement is blocked during active disputes — `ensure_payable_status` must
   remain the single gate for both `record_payment` (status) and
   `settle_invoice_internal` (status + dispute).

Tests that cover these invariants:

| Test file | What it checks |
|---|---|
| `test_partial_payments.rs` | Payment cap, pagination, replay protection |
| `test_settlement_accounting_identity.rs` | No-dust invariant across many inputs |
| `test_settlement_dispute_interaction.rs` | Blocked settlement during disputes |
| `test_settlement_history_reconstruction.rs` | Durable records match inline history |
| `test_settlement_capacity_stress.rs` | Behaviour at and beyond `MAX_PAYMENT_COUNT` |

---

## 8. Cross-references

| Document | What it adds |
|---|---|
| [`docs/SETTLEMENT.md`](SETTLEMENT.md) | Operator-level settlement overview and fund-distribution flow |
| [`docs/SETTLEMENT_ACCOUNTING.md`](SETTLEMENT_ACCOUNTING.md) | Accounting model, auto-release trigger, underpayment/default |
| [`docs/PARTIAL_FILLS.md`](PARTIAL_FILLS.md) | Concise overpayment-capping rule with test patterns |
| [`docs/contracts/settlement.md`](contracts/settlement.md) | Storage architecture, bounded design, security threat model |
| [`docs/contracts/settlement-formula.md`](contracts/settlement-formula.md) | Formula derivation and fee-update timing policy |
| `quicklendx-contracts/src/settlement.rs` | Source implementation |
| `quicklendx-contracts/src/profits.rs` | `PlatformFee` calculation and `ProfitFeeBreakdown` struct |
