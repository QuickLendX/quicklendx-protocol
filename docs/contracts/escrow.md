# Escrow Lifecycle

> **Doc/code alignment verified** against src/escrow.rs, src/payments.rs, src/reentrancy.rs, and src/lib.rs on 2026-06-24.
> Cross-references: [escrow-refund.md](./escrow-refund.md) | [security/settlement-notes.md](../security/settlement-notes.md)

---

## Overview

When a bid is accepted the investor's funds are locked inside the contract (escrow).
They stay there until one of three terminal transitions occurs: **release** to the business,
**refund** to the investor, or an explicit investor **withdrawal**.

---

## EscrowStatus state machine

`
          accept_bid_and_fund
investor ─────────────────────► [ Held ]
                                    │
               release_escrow_funds │ refund_escrow_funds
               (admin / verify)     │ (admin or business)
                                    │ withdraw_investment
                    ┌───────────────┼──────────────┐
                    ▼               │              ▼
               [ Released ]         │         [ Refunded ]
               (terminal)           │         (terminal)
                                    │
`

| Status | Meaning | Token location |
|---|---|---|
| Held | Funds locked in contract | Contract address |
| Released | Funds paid to business | Business address |
| Refunded | Funds returned to investor | Investor address |

Released and Refunded are **terminal** — no further transitions are possible.
Any attempt to release or refund an escrow that is not Held returns InvalidStatus (1401).

---

## Data structures

`
ust
#[contracttype]
pub enum EscrowStatus {
    Held,
    Released,
    Refunded,
}

#[contracttype]
pub struct Escrow {
    pub escrow_id:  BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor:   Address,
    pub business:   Address,
    pub amount:     i128,
    pub currency:   Address,
    pub created_at: u64,
    pub status:     EscrowStatus,
}
`

---

## Public entrypoints (src/lib.rs)

All four entrypoints are **pause-gated** (ContractPaused is returned first) and
**reentrancy-guarded** (with_payment_guard — OperationNotAllowed if already locked).

### ccept_bid_and_fund

`
ust
pub fn accept_bid_and_fund(
    env: Env,
    invoice_id: BytesN<32>,
    bid_id: BytesN<32>,
) -> Result<BytesN<32>, QuickLendXError>
`

Auth: **business owner** (invoice.business.require_auth() inside load_accept_bid_context).

Call graph:
`
accept_bid_and_fund (lib.rs)
  └─ with_payment_guard
       └─ escrow::load_accept_bid_context   ← outer one-escrow guard
       └─ payments::create_escrow           ← inner one-escrow guard + token transfer
            └─ transfer_funds (transfer_from: investor → contract)
       └─ BidStorage::update_bid            (Placed → Accepted)
       └─ invoice.mark_as_funded            (Verified → Funded)
       └─ InvestmentStorage::store_investment
       └─ emit_invoice_funded
`

Errors returned (in check order):

| Error | Code | Source |
|---|---|---|
| ContractPaused | 2100 | pause check |
| OperationNotAllowed | 1402 | reentrancy guard already locked |
| InvoiceNotFound | 1000 | invoice not in storage |
| InvoiceAlreadyFunded | 1002 | invoice.status == Funded |
| InvoiceNotAvailableForFunding | 1001 | invoice not in fundable state |
| InvalidStatus | 1401 | existing escrow/investment record, or bid not Placed/expired |
| InvalidAmount | 1200 | bid_amount <= 0 |
| Unauthorized | 1100 | bid.invoice_id != invoice_id |
| InvoiceAlreadyFunded | 1002 | inner guard: escrow record already exists (create_escrow) |
| InvalidStatus | 1401 | reserve repair in progress |
| ArithmeticOverflow | 1856 | held-reserve addition overflow |
| InsufficientFunds | 1400 | investor balance < amount |
| OperationNotAllowed | 1402 | investor allowance < amount |
| TokenTransferFailed | 2200 | token contract panicked |

---

### 
elease_escrow_funds

`
ust
pub fn release_escrow_funds(
    env: Env,
    invoice_id: BytesN<32>,
) -> Result<(), QuickLendXError>
`

Auth: **admin** (called from erify_invoice which checks admin auth; or directly — no
additional auth check inside 
elease_escrow_funds itself, so it must only be reachable
via admin-gated paths).

Call graph:
`
release_escrow_funds (lib.rs)
  └─ with_payment_guard
       └─ invoice status == Funded check    (InvalidStatus if not)
       └─ EscrowStorage::get_escrow_by_invoice
       └─ payments::release_escrow
            └─ escrow.status == Held check  (InvalidStatus if not)
            └─ transfer_funds (transfer: contract → business)
            └─ EscrowStorage::update_escrow (Held → Released)
       └─ emit_escrow_released
`

Errors:

| Error | Code | Condition |
|---|---|---|
| ContractPaused | 2100 | pause check |
| OperationNotAllowed | 1402 | reentrancy guard |
| InvoiceNotFound | 1000 | invoice missing |
| InvalidStatus | 1401 | invoice not Funded, or escrow not Held, or reserve repair active |
| StorageKeyNotFound | 1301 | no escrow record for invoice |
| InsufficientFunds | 1400 | contract balance < escrow.amount |
| TokenTransferFailed | 2200 | token contract panicked (escrow status NOT updated — safe to retry) |

---

### 
efund_escrow_funds

`
ust
pub fn refund_escrow_funds(
    env: Env,
    invoice_id: BytesN<32>,
    caller: Address,
) -> Result<(), QuickLendXError>
`

Auth: **admin** or **business owner** (caller.require_auth() + matrix check in escrow::refund_escrow_funds).

Call graph:
`
refund_escrow_funds (lib.rs)
  └─ with_payment_guard
       └─ escrow::refund_escrow_funds
            └─ caller.require_auth()
            └─ is_admin || is_business check   (Unauthorized if neither)
            └─ invoice.status == Funded check  (InvalidStatus if not)
            └─ EscrowStorage::get_escrow_by_invoice
            └─ payments::refund_escrow
                 └─ escrow.status == Held check (InvalidStatus if not)
                 └─ transfer_funds (transfer: contract → investor)
                 └─ EscrowStorage::update_escrow (Held → Refunded)
            └─ invoice.mark_as_refunded        (Funded → Refunded)
            └─ bid.status = Cancelled
            └─ investment.status = Refunded
            └─ emit_escrow_refunded
`

Errors:

| Error | Code | Condition |
|---|---|---|
| ContractPaused | 2100 | pause check |
| OperationNotAllowed | 1402 | reentrancy guard |
| InvoiceNotFound | 1000 | invoice missing |
| Unauthorized | 1100 | caller is not admin or business owner |
| InvalidStatus | 1401 | invoice not Funded, or escrow not Held, or reserve repair active |
| StorageKeyNotFound | 1301 | no escrow record for invoice |
| InsufficientFunds | 1400 | contract balance < escrow.amount |
| TokenTransferFailed | 2200 | token contract panicked (escrow status NOT updated — safe to retry) |

---

### withdraw_investment

`
ust
pub fn withdraw_investment(
    env: Env,
    invoice_id: BytesN<32>,
    investor: Address,
) -> Result<(), QuickLendXError>
`

Auth: **investor** (investor.require_auth() inside escrow::withdraw_investment).

Call graph:
`
withdraw_investment (lib.rs)
  └─ with_payment_guard
       └─ escrow::withdraw_investment
            └─ investor.require_auth()
            └─ investment.status == Active check
            └─ investment.investor == investor check
            └─ invoice.status == Funded check
            └─ escrow.status == Held check
            └─ payments::refund_escrow         (Held → Refunded, contract → investor)
            └─ invoice status restored         (Funded → Verified, funded fields cleared)
            └─ bid.status = Cancelled
            └─ investment.status = Withdrawn
            └─ emit_investment_withdrawn
            └─ emit_escrow_refunded
`

Errors:

| Error | Code | Condition |
|---|---|---|
| ContractPaused | 2100 | pause check |
| OperationNotAllowed | 1402 | reentrancy guard |
| StorageKeyNotFound | 1301 | investment or escrow not found |
| InvalidStatus | 1401 | investment not Active, escrow not Held, invoice not Funded |
| Unauthorized | 1100 | caller is not the investment's investor |
| InvoiceNotFound | 1000 | invoice missing |
| InsufficientFunds | 1400 | contract balance < escrow.amount |
| TokenTransferFailed | 2200 | token contract panicked |

---

## Internal functions (src/payments.rs, src/escrow.rs)

These are not public entrypoints but are called by the entrypoints above.

### payments::create_escrow

`
ust
pub fn create_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> Result<BytesN<32>, QuickLendXError>
`

Transfers mount from investor to contract via 	ransfer_from, then writes the
Escrow record. Record is only stored **after** the transfer succeeds — no orphan
state on failure.

### payments::release_escrow

`
ust
pub fn release_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError>
`

Transfers escrow.amount from contract to escrow.business. Updates status
Held → Released only after the token call succeeds.

### payments::refund_escrow

`
ust
pub fn refund_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError>
`

Transfers escrow.amount from contract to escrow.investor. Updates status
Held → Refunded only after the token call succeeds.

### 
eentrancy::with_payment_guard

`
ust
pub fn with_payment_guard<F, R>(env: &Env, f: F) -> Result<R, QuickLendXError>
where
    F: FnOnce() -> Result<R, QuickLendXError>
`

Sets a pay_lock flag in instance storage before running , clears it after
(on both success and failure). If the flag is already set, returns
OperationNotAllowed (1402) immediately without running .

Guard ordering: the pause check in lib.rs runs **before** with_payment_guard,
so ContractPaused is always returned ahead of OperationNotAllowed.

---

## One-escrow-per-invoice invariant (two-layer guard)

Each invoice maps to **at most one** escrow record across its entire lifetime.
Enforced at two independent layers:

**Layer 1 — escrow::load_accept_bid_context (outer)**

Checked before any funds move:
- EscrowStorage::get_escrow_by_invoice(invoice_id).is_some() → InvalidStatus
- InvestmentStorage::get_investment_by_invoice(invoice_id).is_some() → InvalidStatus
- invoice.funded_amount != 0 || invoice.funded_at.is_some() || invoice.investor.is_some() → InvalidStatus
- invoice.status == Funded → InvoiceAlreadyFunded

**Layer 2 — payments::create_escrow (inner)**

Re-checks EscrowStorage::get_escrow_by_invoice **before** the token transfer:
→ InvoiceAlreadyFunded if a record already exists.

This means the invariant holds even if create_escrow is called directly, bypassing
the outer guard.

| Attack vector | Layer that blocks it |
|---|---|
| Double ccept_bid_and_fund on same invoice | Layer 1: status is Funded after first call |
| Direct create_escrow call after funding | Layer 2: escrow record already exists |
| Re-fund after release | Layer 2: Released record still present |
| Re-fund after refund | Layer 2: Refunded record still present |
| Concurrent bids race | Both layers: second call sees existing escrow/investment |
| Pre-existing investment record | Layer 1: InvestmentStorage check returns InvalidStatus |

Test coverage: src/test_escrow_uniqueness.rs — run with cargo test test_escrow_uniqueness.

---

## Atomicity and retry safety

Token state changes follow a strict **transfer-then-write** order:

1. Pre-flight checks (status, amounts, allowance)
2. Token transfer (	ransfer or 	ransfer_from)
3. Storage writes (escrow status, invoice, bid, investment)

If step 2 fails (TokenTransferFailed, InsufficientFunds, OperationNotAllowed),
step 3 never executes. The Soroban host discards all storage mutations for that
invocation. No partial state is written.

**Retry**: after any token-transfer failure, all on-chain state is identical to
before the call. The same arguments can be retried safely once the root cause
(balance, allowance) is resolved.

| Failure | Invoice | Bid | Escrow | Investment | Funds |
|---|---|---|---|---|---|
| Insufficient balance | Verified | Placed | not created | not created | unchanged |
| Zero allowance | Verified | Placed | not created | not created | unchanged |
| Token panic | Verified | Placed | not created | not created | unchanged |

---

## Held-reserve accounting

EscrowStorage maintains a per-currency HeldEscrowReserve that tracks the total
mount across all Held escrows. This is used by the emergency-withdrawal flow
to distinguish protocol-held funds from protocol-owned funds.

- create_escrow increments the reserve before the token transfer.
- 
elease_escrow / 
efund_escrow decrement it after the token transfer.
- If the reserve undercounts (legacy data), the undercounting path clears and
  marks the reserve incomplete rather than blocking user operations; the
  emergency-withdrawal path remains fail-closed until a 
epair_held_reserve_page
  run completes.
- While a repair is in progress (
epair_next_offset != 0 && !complete),
  create_escrow, 
elease_escrow, and 
efund_escrow all return InvalidStatus.

---

## Events emitted

| Topic symbol | Emitted by | Payload |
|---|---|---|
| esc_cr | create_escrow | escrow_id, invoice_id, investor, business, amount, currency |
| inv_fnd | ccept_bid_and_fund | invoice_id, investor, amount |
| esc_rel | 
elease_escrow_funds | escrow_id, invoice_id, business, amount |
| esc_ref | 
efund_escrow_funds, withdraw_investment | escrow_id, invoice_id, investor, amount |
| inv_wth | withdraw_investment | investment_id, invoice_id, investor, amount |

---

## Security summary

| Property | Mechanism |
|---|---|
| Reentrancy | with_payment_guard — pay_lock in instance storage |
| Double-escrow | Two-layer guard (outer: context check; inner: storage check) |
| Double-release / double-refund | Terminal status check in 
elease_escrow / 
efund_escrow |
| Refund authorization | caller.require_auth() + admin-or-business matrix |
| Transfer-then-write | Storage only mutated after token call succeeds |
| Reserve integrity | Per-currency held-reserve; fail-closed on undercount |

See also: [escrow-refund.md](./escrow-refund.md), [security/settlement-notes.md](../security/settlement-notes.md).

---

## Doc/code alignment note

Every function signature, error code, and call-graph step in this document was
validated against the following source files at commit time:

- quicklendx-contracts/src/escrow.rs — ccept_bid_and_fund, 
efund_escrow_funds, withdraw_investment
- quicklendx-contracts/src/payments.rs — create_escrow, 
elease_escrow, 
efund_escrow, EscrowStatus
- quicklendx-contracts/src/reentrancy.rs — with_payment_guard
- quicklendx-contracts/src/lib.rs — public entrypoint wrappers
- quicklendx-contracts/src/errors.rs — all error discriminants

cargo test passes with no doc-test failures.
