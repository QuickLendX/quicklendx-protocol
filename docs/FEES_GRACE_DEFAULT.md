# Fees, Grace Period, and Default Triggers

> **Audience:** Contributors who need to understand how QuickLendX charges fees,
> what the grace period protects, and when an invoice automatically transitions
> into `Defaulted`. Read this before touching `fees.rs`, `defaults.rs`,
> `settlement.rs`, or `profits.rs`.

---

## Table of Contents

1. [Platform Fee Model](#1-platform-fee-model)
2. [Settlement Fee Calculation](#2-settlement-fee-calculation)
3. [Timing Modifiers — Early and Late Payment Fees](#3-timing-modifiers--early-and-late-payment-fees)
4. [Volume-Tier Discounts](#4-volume-tier-discounts)
5. [Grace Period](#5-grace-period)
6. [Default Triggers](#6-default-triggers)
7. [Default Finality Guards](#7-default-finality-guards)
8. [Worked End-to-End Example](#8-worked-end-to-end-example)
9. [Entrypoints Quick-Reference](#9-entrypoints-quick-reference)
10. [Error Codes](#10-error-codes)
11. [Cross-References](#11-cross-references)

---

## 1. Platform Fee Model

The live fee rate and treasury destination are stored together in a
`PlatformFeeConfig` record under instance storage key `plt_fee`:

```rust
// quicklendx-contracts/src/fees.rs
pub struct PlatformFeeConfig {
    pub fee_bps: u32,
    pub treasury_address: Option<Address>,
    pub updated_at: u64,
    pub updated_by: Address,
}
```

### Fee-type catalogue

| Fee type       | Default rate | Purpose                                       |
| -------------- | ------------ | --------------------------------------------- |
| `Platform`     | 200 bps (2%) | Cut from gross profit on settlement            |
| `Processing`   | 50 bps (0.5%)| Per-transaction processing charge             |
| `Verification` | 100 bps (1%) | Applied to newly verified transactions        |
| `EarlyPayment` | — (discount) | Reduces Platform fee when paid early          |
| `LatePayment`  | — (surcharge)| Added when the invoice is paid after due date |

### Hard limits

```
MAX_FEE_BPS      = 1_000  // 10%  — no base fee may exceed this
MAX_PLATFORM_FEE = 1_000  // same cap for the platform fee specifically
```

Any `update_platform_fee` or `update_fee_structure` call with a `fee_bps`
above `1_000` is rejected immediately with **`InvalidFeeBasisPoints`** (error 105)
before touching storage.

> **Reviewer note:** The cap is enforced inside `FeeManager::update_platform_fee`
> *before* `admin.require_auth()` is checked, so misconfigured calls fail fast
> without advancing the auth nonce.

---

## 2. Settlement Fee Calculation

Fee arithmetic is in `profits::PlatformFee::calculate`. All math is integer
basis-point arithmetic; the denominator is `10_000`.

### Case A — No profit (payment ≤ investment)

```
gross_profit  = 0
platform_fee  = 0
investor_return = payment_amount
```

The investor absorbs any shortfall. The protocol takes nothing.

### Case B — Profit (payment > investment)

```
gross_profit    = payment_amount − investment_amount
platform_fee    = floor(gross_profit × fee_bps / 10_000)
investor_return = payment_amount − platform_fee
```

#### Accounting identity (the "no-dust" invariant)

```
investor_return + platform_fee == payment_amount
```

Because `platform_fee` is floored, any sub-stroop remainder stays with the
investor. This identity is asserted on-chain before funds are transferred.

Tests that pin this invariant:
- `test_fee_only_from_profit_not_principal`
- `test_zero_profit_investor_recovers_full_payment`
- `test_loss_settlement_no_fee_investor_gets_payment`

### Example

```
investment_amount = 1_000_000 stroops
payment_amount    = 1_100_000 stroops   (10 % profit)
fee_bps           = 200                 (2 %)

gross_profit      = 1_100_000 - 1_000_000 = 100_000
platform_fee      = floor(100_000 × 200 / 10_000) = 2_000
investor_return   = 1_100_000 - 2_000   = 1_098_000
```

Verification: `1_098_000 + 2_000 = 1_100_000`. ✓

---

## 3. Timing Modifiers — Early and Late Payment Fees

Two constants in `fees.rs` modulate the calculated fee based on when the
business settles relative to the invoice due date:

```rust
const EARLY_PLATFORM_DISCOUNT_BPS: i128 = 1_000; // subtract 10% from platform fee
const LATE_FEE_SURCHARGE_BPS:      i128 = 2_000; // add    20% to late-payment fee
```

### Early payment discount

Applied when `ledger.timestamp() < invoice.due_date`.

```
early_discount  = floor(platform_fee × 1_000 / 10_000)
effective_fee   = platform_fee − early_discount
```

The discount reduces the platform fee only — it does not affect `Processing`
or `Verification` fees, and it never makes the platform fee negative (floored
at 0).

### Late payment surcharge

Applied when `ledger.timestamp() > invoice.due_date + grace_period` but the
escrow has not yet been default-triggered. The surcharge is an **additive**
penalty on the `LatePayment` fee structure:

```
late_surcharge  = floor(late_fee_base × 2_000 / 10_000)
effective_late  = late_fee_base + late_surcharge
```

> **Note:** The timing modifiers are computed at settlement time using the
> ledger timestamp. They do not modify stored fee configuration.

---

## 4. Volume-Tier Discounts

A contributor's (or user's) accumulated transaction volume determines which
fee discount bracket applies. The tier progression is monotone — volume only
grows, so demotion is not possible through normal use.

```rust
// quicklendx-contracts/src/fees.rs
pub enum VolumeTier { Standard, Silver, Gold, Platinum }
```

| Tier       | Volume threshold (stroops)    | Discount |
| ---------- | ----------------------------- | -------- |
| `Standard` | 0                             | 0 bps    |
| `Silver`   | 100,000,000,000 (≈100 k XLM)  | 500 bps  |
| `Gold`     | 500,000,000,000 (≈500 k XLM)  | 1,000 bps|
| `Platinum` | 1,000,000,000,000 (≈1 M XLM)  | 1,500 bps|

Discounts apply to all base fee types **except** the `LatePayment` surcharge.

Volume is updated via `update_user_transaction_volume`. The function is
permissionless today; see `docs/PLATFORM_FEES.md` for the operator override
procedure.

---

## 5. Grace Period

The grace period is the buffer time after `invoice.due_date` during which the
business may still repay without triggering a default. It is specified in
seconds.

### Configuration sources (resolution order)

```
1. Explicit caller override supplied to trigger_default / scan_funded_invoice_expirations
2. Protocol config  (ProtocolConfig.grace_period_seconds — set by admin during init)
3. Hard-coded default DEFAULT_GRACE_PERIOD = 7 × 24 × 3600 = 604_800 s (7 days)
```

The resolver is:

```rust
// quicklendx-contracts/src/defaults.rs
pub const DEFAULT_GRACE_PERIOD:    u64 = 7 * 24 * 60 * 60;  // 7 days
const     MAX_GRACE_PERIOD:        u64 = 30 * 24 * 60 * 60; // 30 days — hard cap
```

```rust
pub fn resolve_grace_period(env: &Env, grace_period: Option<u64>)
    -> Result<u64, QuickLendXError>
{
    match grace_period {
        Some(value) => {
            if value > MAX_GRACE_PERIOD {
                return Err(QuickLendXError::InvalidTimestamp);
            }
            Ok(value)
        }
        None => Ok(ProtocolInitializer::get_protocol_config(env)
            .map(|c| c.grace_period_seconds)
            .unwrap_or(DEFAULT_GRACE_PERIOD)),
    }
}
```

### Grace deadline arithmetic

```
grace_deadline = invoice.due_date + resolved_grace_period
```

A default may **not** be triggered while `ledger.timestamp() <= grace_deadline`.
Calls at exactly the deadline timestamp are rejected (`OperationNotAllowed`) to
prevent off-by-one early liquidation.

---

## 6. Default Triggers

An invoice transitions from `Funded` → `Defaulted` via one of two paths:

### 6.1 Explicit trigger (single invoice)

```rust
// contract entrypoint wired to defaults::mark_invoice_defaulted
contract.trigger_default(
    env,
    caller:       Address,
    invoice_id:   BytesN<32>,
) -> Result<(), QuickLendXError>
```

- **Caller**: permissionless — any account may trigger after the deadline.
- **Preconditions** (checked in order):
  1. Invoice exists (`InvoiceNotFound` otherwise).
  2. Default transition guard is not already set (`DuplicateDefaultTransition`).
  3. Invoice status is **`Funded`** (`InvoiceNotAvailableForFunding` otherwise).
  4. Settlement has **not** been finalized (`InvalidStatus` otherwise).
  5. Escrow status is **`Held`** (`InvalidStatus` otherwise).
  6. `ledger.timestamp() > grace_deadline` (`OperationNotAllowed` otherwise).
- **Effect**:
  - Invoice status → `Defaulted`.
  - Investment status → `Defaulted`.
  - Insurance claims processed (if opted in).
  - Events emitted: `invoice_expired`, `invoice_defaulted`, `insurance_claimed`
    (per provider with non-zero coverage).
  - Notification: `InvoiceDefaulted` sent to business and investor.

### 6.2 Bounded batch scanner

```rust
// contract entrypoint wired to defaults::scan_funded_invoice_expirations
contract.scan_funded_invoice_expirations(
    env,
    grace_period: u64,
    limit:        Option<u32>,
) -> Result<OverdueScanResult, QuickLendXError>
```

- **Caller**: permissionless.
- **Batch size**: `limit` is clamped to `[1, 100]` (`MAX_OVERDUE_SCAN_BATCH_LIMIT`).
  Default when `None` is `25` (`DEFAULT_OVERDUE_SCAN_BATCH_LIMIT`).
- **Cursor**: persisted in instance storage under `ovd_scan`. Each call advances
  the cursor by `limit` positions and wraps back to 0 when the entire funded set
  has been scanned.
- **Returns**:

  ```rust
  pub struct OverdueScanResult {
      pub overdue_count:  u32,  // invoices past due_date in this batch
      pub scanned_count:  u32,  // invoices examined in this batch
      pub total_funded:   u32,  // snapshot size of the funded index
      pub next_cursor:    u32,  // 0 = full cycle complete
  }
  ```

- **To scan all funded invoices**, keep calling until `next_cursor == 0`.

### 6.3 Overdue vs. defaultable distinction

| Condition | `is_overdue` | Defaultable |
| --------- | ------------ | ----------- |
| `now > due_date` | ✓ | ✗ (still in grace) |
| `now > due_date + grace_period` | ✓ | ✓ |

`is_overdue` only triggers the overdue notification; it does not move the
invoice to `Defaulted`. The additional `grace_period` buffer must also have
elapsed before `handle_default` will accept the transition.

---

## 7. Default Finality Guards

Three independent checks protect against double-default and double-finality:

| Guard | Location | Purpose |
| ----- | -------- | ------- |
| Status check | `mark_invoice_defaulted` | Rejects if already `Defaulted` or not `Funded` |
| Settlement finality | `ensure_default_transition_open` | Rejects if `is_invoice_finalized` returns `true` |
| Escrow status | `ensure_default_transition_open` | Rejects if escrow is not `Held` |
| Transition guard | `check_and_set_default_guard` | Atomically sets a persistent flag; rejects on second call with `DuplicateDefaultTransition` |

The transition guard is written **only after** all finality checks pass. This
ensures that a failed attempt (e.g. invoice still within grace) does not
poison the guard and block a legitimate future call.

The exhaustive decision table is maintained in
[`docs/default-finality-matrix.md`](default-finality-matrix.md).

---

## 8. Worked End-to-End Example

This example walks through an invoice from funding to default, showing exactly
when each fee and deadline applies.

```
Timeline (all timestamps in Unix seconds):

t=0          Invoice created.  due_date = t+30d = 2_592_000
t=2_592_000  Due date passes.  Invoice is overdue but NOT yet defaultable.
t=2_592_000
  + 604_800  Grace deadline    = 3_196_800
t=3_196_800  Grace deadline.   ledger.timestamp() == grace_deadline
             → trigger_default rejected (must be STRICTLY greater)
t=3_196_801  First valid default trigger.
```

**Fee path (if settled instead, at t = 2_500_000, before due date):**

```
investment_amount    = 5_000_000 stroops
payment_amount       = 5_500_000 stroops   (10% profit)
fee_bps (Platform)   = 200                 (2%)

gross_profit         = 500_000
platform_fee (base)  = floor(500_000 × 200 / 10_000) = 10_000
early_discount       = floor(10_000 × 1_000 / 10_000) = 1_000  (settled before due date)
effective_platform   = 10_000 − 1_000 = 9_000
investor_return      = 5_500_000 − 9_000 = 5_491_000

Verification:  5_491_000 + 9_000 = 5_500_000  ✓
```

**Entrypoints exercised in order:**

```
1. store_invoice(business, ...)           → Pending
2. verify_invoice(admin, invoice_id)      → Verified
3. accept_bid(business, invoice_id, bid_id) → Funded (escrow locked)
4. settle_invoice(business, invoice_id, payment_token, 5_500_000)
   → fees calculated, funds transferred, status = Paid
```

OR, if not settled in time:

```
4. trigger_default(anyone, invoice_id)    → Defaulted (after t=3_196_801)
```

---

## 9. Entrypoints Quick-Reference

| Entrypoint | Auth | Effect |
| ---------- | ---- | ------ |
| `update_platform_fee(admin, fee_bps)` | Admin | Updates `PlatformFeeConfig.fee_bps`; rejects > 1000 bps |
| `configure_treasury(admin, address)` | Admin | Sets treasury destination for protocol fees |
| `update_fee_structure(admin, fee_type, ...)` | Admin | Updates a specific `FeeStructure`; rejects bps > 1000 |
| `preview_fee_config(admin, proposed)` | Admin | Read-only diff; does not modify state |
| `set_fee_config(admin, proposed)` | Admin | Writes `FeeConfig`; safe update path |
| `trigger_default(caller, invoice_id)` | Permissionless | Single-invoice default after grace expires |
| `scan_funded_invoice_expirations(grace, limit)` | Permissionless | Batch overdue scan with rotating cursor |
| `settle_invoice(caller, invoice_id, token, amount)` | Business/Admin | Finalizes repayment; distributes fees |

---

## 10. Error Codes

| Error | Code | When raised |
| ----- | ---- | ----------- |
| `InvalidFeeBasisPoints` | 105 | `fee_bps > MAX_FEE_BPS (1000)` |
| `InvalidFeeConfiguration` | 106 | Fee system re-initialized, or cross-fee consistency violation |
| `InvalidTimestamp` | — | Grace period override > `MAX_GRACE_PERIOD (30 days)` |
| `OperationNotAllowed` | — | Default triggered before `now > grace_deadline` |
| `DuplicateDefaultTransition` | — | Second default attempt on same invoice |
| `InvoiceNotAvailableForFunding` | 1001 | Default on non-`Funded` invoice |
| `InvoiceAlreadyDefaulted` | 1006 | Default on already-`Defaulted` invoice |
| `InvalidStatus` | — | Settlement finalized, or escrow not `Held` |
| `ArithmeticOverflow` | — | Checked arithmetic failed in fee helpers |

Full error catalog: [`docs/ERROR_CODES.md`](ERROR_CODES.md).

---

## 11. Cross-References

| Topic | Source |
| ----- | ------ |
| Fee types, constants, `FeeManager` | [`src/fees.rs`](../quicklendx-contracts/src/fees.rs) |
| Settlement formula, "no-dust" invariant | [`src/settlement.rs`](../quicklendx-contracts/src/settlement.rs) |
| Profit-fee calculation | [`src/profits.rs`](../quicklendx-contracts/src/profits.rs) |
| Grace resolution, overdue scanner | [`src/defaults.rs`](../quicklendx-contracts/src/defaults.rs) |
| Admin fee config entrypoints | [`src/admin.rs`](../quicklendx-contracts/src/admin.rs) |
| Default finality decision table | [`docs/default-finality-matrix.md`](default-finality-matrix.md) |
| Invoice state machine | [`docs/INVOICE_LIFECYCLE.md`](INVOICE_LIFECYCLE.md) |
| Platform fee schedule and tier overrides | [`docs/PLATFORM_FEES.md`](PLATFORM_FEES.md) |
| Settlement fund distribution detail | [`docs/SETTLEMENT.md`](SETTLEMENT.md) |
| Contract-level fee deep-dive | [`docs/contracts/fees.md`](contracts/fees.md) |
| Operator fee change playbook | [`docs/contracts/platform-fee-ops.md`](contracts/platform-fee-ops.md) |
| Profit-fee formula internals | [`docs/contracts/profit-fee-formula.md`](contracts/profit-fee-formula.md) |
| All error codes | [`docs/ERROR_CODES.md`](ERROR_CODES.md) |
