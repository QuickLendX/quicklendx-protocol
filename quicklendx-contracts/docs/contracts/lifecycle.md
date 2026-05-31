# Invoice Lifecycle — Full Reference

This document describes every stage of the QuickLendX invoice lifecycle, the
valid state transitions, the on-chain functions that drive each transition, and
the three end-to-end test scenarios that validate the complete flow.

---

## 1. Lifecycle Stages

```
                  ┌──────────┐
                  │  Pending │  ← upload_invoice / store_invoice
                  └────┬─────┘
                       │ verify_invoice (admin)
                  ┌────▼─────┐
                  │ Verified │
                  └────┬─────┘
                       │ accept_bid_and_fund (business + investor bid)
                  ┌────▼─────┐
                  │  Funded  │
                  └────┬─────┘
          ┌────────────┼────────────┐
          │            │            │
  process_partial  settle_invoice  mark_invoice_defaulted
  _payment (×N)        │            │
          │        ┌───▼───┐   ┌───▼──────┐
          └───────►│  Paid │   │ Defaulted│
                   └───────┘   └──────────┘
                                    │
                               refund_escrow
                                    │
                              (investor refunded)
```

### Stage descriptions

| Stage | Status | Entry function | Exit function(s) |
|-------|--------|----------------|-----------------|
| Upload | `Pending` | `upload_invoice` | `verify_invoice`, `cancel_invoice` |
| Verification | `Verified` | `verify_invoice` | `accept_bid_and_fund`, `cancel_invoice` |
| Funding | `Funded` | `accept_bid_and_fund` | `settle_invoice`, `process_partial_payment`, `mark_invoice_defaulted` |
| Settlement | `Paid` | `settle_invoice` (or auto via `process_partial_payment`) | *(terminal)* |
| Default | `Defaulted` | `mark_invoice_defaulted` | *(terminal — refund via `refund_escrow`)* |
| Cancellation | `Cancelled` | `cancel_invoice` | *(terminal)* |
| Refund | `Refunded` | `refund_escrow_funds` | *(terminal)* |

---

## 2. State Transition Table

| From | To | Trigger | Auth | Side effects |
|------|----|---------|------|--------------|
| `Pending` | `Verified` | `verify_invoice` | Admin | Status index updated; `InvoiceVerified` event |
| `Pending` | `Cancelled` | `cancel_invoice` | Business | Status index updated; `InvoiceCancelled` event |
| `Verified` | `Funded` | `accept_bid_and_fund` | Business (bid accepted) | Escrow created; `Investment` record created (`Active`); `BidAccepted`, `EscrowCreated` events |
| `Verified` | `Cancelled` | `cancel_invoice` | Business | Status index updated; `InvoiceCancelled` event |
| `Funded` | `Paid` | `settle_invoice` / `process_partial_payment` (full) | Business (payer) | Escrow released; investor return transferred; platform fee routed; `Investment → Completed`; `InvoiceSettled` event |
| `Funded` | `Defaulted` | `mark_invoice_defaulted` | Admin | `Investment → Defaulted`; `InvoiceDefaulted` event |
| `Funded` | `Refunded` | `refund_escrow_funds` | Admin / Business | Escrow refunded to investor; `Investment → Refunded`; `EscrowRefunded` event |

---

## 3. Key Functions

### `upload_invoice`
```rust
pub fn upload_invoice(
    env: Env,
    business: Address,
    amount: i128,
    currency: Address,
    due_date: u64,
    description: String,
    category: InvoiceCategory,
    tags: Vec<String>,
) -> Result<BytesN<32>, QuickLendXError>
```
- Requires `business.require_auth()`.
- Business must be KYC-verified (`Verified` status).
- Currency must be whitelisted.
- Returns the new invoice ID.

### `verify_invoice`
```rust
pub fn verify_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError>
```
- Admin-only.
- Transitions `Pending → Verified`.

### `place_bid`
```rust
pub fn place_bid(
    env: Env,
    investor: Address,
    invoice_id: BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> Result<BytesN<32>, QuickLendXError>
```
- Requires `investor.require_auth()`.
- Investor must be KYC-verified with sufficient investment limit.
- Invoice must be `Verified`.

### `accept_bid_and_fund`
```rust
pub fn accept_bid_and_fund(
    env: Env,
    invoice_id: BytesN<32>,
    bid_id: BytesN<32>,
) -> Result<BytesN<32>, QuickLendXError>
```
- Transfers `bid_amount` tokens from investor to contract (escrow).
- Creates `Investment` record with `Active` status.
- Transitions invoice `Verified → Funded`.
- Protected by reentrancy guard.

### `process_partial_payment`
```rust
pub fn process_partial_payment(
    env: Env,
    invoice_id: BytesN<32>,
    payment_amount: i128,
    transaction_id: String,
) -> Result<(), QuickLendXError>
```
- Requires `business.require_auth()` (payer = invoice business).
- Caps applied amount to remaining due (no overpayment).
- Deduplicates by `transaction_id` (nonce).
- Auto-settles when `total_paid >= invoice.amount`.

### `settle_invoice`
```rust
pub fn settle_invoice(
    env: Env,
    invoice_id: BytesN<32>,
    payment_amount: i128,
) -> Result<(), QuickLendXError>
```
- Requires exact remaining-due amount (no overpayment).
- Releases escrow to business.
- Transfers investor return and platform fee.
- Transitions invoice `Funded → Paid`, investment `Active → Completed`.

### `expire_invoice`
```rust
pub fn expire_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError>
```
- Emits `InvoiceExpired` event.
- Requires `current_timestamp > invoice.due_date`.

### `mark_invoice_defaulted`
```rust
pub fn mark_invoice_defaulted(
    env: Env,
    invoice_id: BytesN<32>,
    grace_period: Option<u64>,
) -> Result<(), QuickLendXError>
```
- Admin-only.
- Requires `current_timestamp > due_date + grace_period`.
- Transitions invoice `Funded → Defaulted`, investment `Active → Defaulted`.

### `refund_escrow`
```rust
pub fn refund_escrow(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError>
```
- Admin-only convenience wrapper.
- Returns escrowed tokens to investor.

---

## 4. E2E Test Scenarios

The integration tests live in `tests/invoice_lifecycle_e2e.rs`.

### 4.1 Happy Path — `test_invoice_lifecycle_happy_path`

**Flow:** Upload → Verify → Bid → Fund → Partial payment → Settle

**Balance flow:**
```
Initial:   business = 20 000 | investor = 15 000 | contract = 1
After fund (bid = 9 000):
           investor = 6 000  | contract = 9 001
After settle (invoice = 10 000):
           business = 19 000 | investor = 6 000 + return | contract ≈ 1 + fee
```

**Assertions at each stage:**
1. Upload → `status == Pending`, `amount` correct, analytics `total_invoices == 1`
2. Verify → `status == Verified`, invoice in Verified bucket
3. Bid → `bid.status == Placed`, one bid on invoice
4. Fund → `status == Funded`, investor paid `bid_amount`, escrow held, `investment.status == Active`, analytics `total_investments == 1`
5. Partial → `total_paid == partial_amount`, `status == Funded`, payment history length 1
6. Settle → `status == Paid`, `total_paid == invoice_amount`, `investment.status == Completed`, balance reconciliation, `success_rate > 0`, `default_rate == 0`

---

### 4.2 Default Branch — `test_invoice_lifecycle_default_branch`

**Flow:** Upload → Verify → Bid → Fund → Expire → Default → Refund

**Balance flow:**
```
Initial:   business = 20 000 | investor = 15 000 | contract = 1
After fund (bid = 9 000):
           investor = 6 000  | contract = 9 001
After refund:
           investor = 15 000 | contract = 1      (fully restored)
           business = 20 000                     (unchanged)
```

**Assertions at each stage:**
1–4. Same as happy path through Funded.
5. Advance time past `due_date`.
6. `expire_invoice` + `mark_invoice_defaulted(grace=0)` → `status == Defaulted`, `investment.status == Defaulted`
7. `refund_escrow` → investor refunded `bid_amount`, contract balance restored, business balance unchanged, `default_rate > 0`, `success_rate == 0`

---

### 4.3 Multiple Partials Then Full Settle — `test_partial_then_full_settle`

**Flow:** Upload → Verify → Bid → Fund → 3× Partial → Final settle

**Balance flow:**
```
Initial:   business = 20 000 | investor = 15 000 | contract = 1
After fund (bid = 8 000):
           investor = 7 000  | contract = 8 001
After 3 partials (3 × 2 000 = 6 000):
           business = 14 000
After final settle (remaining 4 000):
           business = 18 000 | investor = 7 000 + return
           net business outflow = 10 000 - 8 000 = 2 000
```

**Assertions at each stage:**
1–4. Same as happy path through Funded.
5a. Partial 1 → `total_paid == 2 000`, `status == Funded`
5b. Partial 2 → `total_paid == 4 000`, `status == Funded`
5c. Partial 3 → `total_paid == 6 000`, `status == Funded`, `payment_history.len() == 3`
6. Final settle → `status == Paid`, `total_paid == 10 000`, `investment.status == Completed`, balance reconciliation, `success_rate > 0`, `default_rate == 0`

---

## 5. Running the Tests

```bash
# From the quicklendx-contracts directory
cargo test --test invoice_lifecycle_e2e 2>&1 | tail -20
```

All three tests run without the `legacy-tests` feature flag — they use only
the stable public API surface.

---

## 6. Analytics Integration

After each lifecycle-altering operation the `get_platform_metrics` endpoint
reflects the updated state:

| After stage | `total_invoices` | `total_investments` | `success_rate` | `default_rate` |
|-------------|-----------------|---------------------|----------------|----------------|
| Upload | 1 | 0 | 0 | 0 |
| Fund | 1 | 1 | 0 | 0 |
| Settle (Paid) | 1 | 1 | 10 000 | 0 |
| Default | 1 | 1 | 0 | 10 000 |

Rates are expressed in basis points (10 000 = 100 %).
