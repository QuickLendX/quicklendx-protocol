# Escrow Time Limits

> **Audience:** Protocol operators — the people who deploy, configure, and monitor QuickLendX Soroban contracts.

This document explains how long escrow funds can remain in `Held` status before they must be acted upon, and how the protocol handles unclaimed escrows through the default and refund mechanisms.

## Overview

When a bid is accepted, the investor's funds are locked in an escrow record (`EscrowStatus::Held`). These funds remain locked until one of three terminal transitions occurs:

1. **Release** — funds are paid to the business when the invoice is settled/paid
2. **Refund** — funds are returned to the investor when the invoice defaults, is cancelled, or when the due date has passed
3. **Default handling** — after the grace period expires, the default mechanism processes the invoice and triggers escrow disposition

| Parameter | Default | Max | Where configured |
|-----------|---------|-----|------------------|
| `grace_period_seconds` | `604_800` (7 days) | `2_592_000` (30 days) | Protocol limits (`set_protocol_limits`) |
| `due_date` | Set by business at `store_invoice` | Max `max_due_date_days` ahead | Invoice creation |
| `escrow_deadline` | Computed as `due_date + grace_period` | N/A | Derived — not stored independently |

**There is no explicit auto-refund timer for escrow records.** Instead, the default mechanism detects overdue invoices after the grace period and handles escrow disposition through the insurance claim and refund pathways.

## Timeline

```
Invoice funded (escrow Held)
       │
       ▼
  ┌─────────────────────────────────────────────────────────┐
  │  Invoice is paid before or at due_date                  │
  │  → release_escrow() moves funds to business             │
  └─────────────────────────────────────────────────────────┘
       │ (if invoice is NOT paid)
       ▼
  due_date reached
       │
       ▼
  Overdue notification emitted; late-payment surcharge accrues
       │
       ▼
  grace_period running (due_date + grace_period)
       │
       ▼
  ┌─────────────────────────────────────────────────────────┐
  │  grace_period expired (ledger.timestamp > due_date +    │
  │  grace_period)                                         │
  │  → trigger_default() processes the invoice              │
  │  → insurance claims are paid to the investor            │
  │  → escrow disposition follows from default handling     │
  └─────────────────────────────────────────────────────────┘
```

## Error Codes

| Error | Code | ABI symbol | When it fires |
|-------|------|-----------|---------------|
| `InvalidStatus` | 1401 | `INV_ST` | Escrow is not `Held` (already released or refunded) |
| `InvoiceDueDateInvalid` | 1700 | `IDATE` | `extend_escrow_expiry` called with `new_due_date <= current due_date` |
| `OperationNotAllowed` | 1600 | `OP_NOT` | `extend_escrow_expiry` called when escrow has already been extended, or when escrow is not in `Held` status |

## Release Path (Funds to Business)

`release_escrow` can only be called when the invoice status is `Paid` and the escrow is in `Held` status. There is no time limit on the release — as long as the invoice is paid, the escrow can be released regardless of the due date.

**Rust entrypoint signature:**

```rust
pub fn release_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError>;
```

**Guard checks:**

- Escrow status must be `Held`
- Invoice status must be `Paid`
- Both checks return `InvalidStatus` on failure

### Concrete example

```rust
// After invoice is paid by the investor
let invoice_id = BytesN::from_array(&env, &[42u8; 32]);

// Release escrowed funds to the business
let res = client.release_escrow(&invoice_id);
assert!(res.is_ok());
```

If the escrow has already been released or refunded, the call fails:

```rust
let res = client.try_release_escrow(&invoice_id);
assert_eq!(res.unwrap_err().ok(), Some(QuickLendXError::InvalidStatus));
```

## Refund Path (Funds to Investor)

`refund_escrow` can be called when the escrow is in `Held` status. Unlike release, refund is gated by time: it requires that the invoice's due date has passed (`ledger.timestamp() > due_date`).

**Rust entrypoint signature:**

```rust
pub fn refund_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError>;
```

**Guard checks:**

- Escrow status must be `Held`
- Current timestamp must be strictly greater than `due_date`
- Returns `InvalidStatus` if escrow is already released/refunded
- Returns `OperationNotAllowed` if timestamp is not past due_date

### Concrete example

```rust
use soroban_sdk::{testutils::Ledger as _, BytesN, Env};

let env = Env::default();
let contract_id = env.register(QuickLendXContract, ());
let client = QuickLendXContractClient::new(&env, &contract_id);

// Assume invoice was funded with due_date = now + 1 day
let due_date = env.ledger().timestamp() + 86_400;
let invoice_id = create_and_fund_invoice(&env, &client, due_date);

// Refund blocked before due_date
env.ledger().set_timestamp(due_date - 1);
let res = client.try_refund_escrow(&invoice_id);
assert!(res.is_err()); // OperationNotAllowed: timestamp not past due_date

// Refund allowed at due_date (strictly greater-than check)
env.ledger().set_timestamp(due_date);
let res = client.try_refund_escrow(&invoice_id);
assert!(res.is_err()); // Still blocked: must be STRICTLY greater

// Refund allowed one second past due_date
env.ledger().set_timestamp(due_date + 1);
let res = client.refund_escrow(&invoice_id);
assert!(res.is_ok());
```

## Default Handling (Unclaimed Escrows)

When an invoice remains unpaid after the grace period (`due_date + grace_period`), the default mechanism handles it. This is triggered by the `trigger_default` / `scan_funded_invoice_expirations` admin operation or batch scanner.

### Default transition flow

1. `mark_invoice_defaulted` checks `ledger.timestamp() > due_date + grace_period`
2. Insurance claims are processed for the investor's coverage
3. The invoice status transitions to `Defaulted`
4. The settlement path is permanently blocked (settlement finalization requires `dispute_status == None` and invoice status `Paid`)
5. Escrow disposition follows from the investment status transition and insurance claims — the escrow record itself is not consumed by `trigger_default` but the funds are no longer accessible via normal release

### Boundary check

| Scenario | Timestamp relative to `due_date + grace_period` | Result |
|----------|--------------------------------------------------|--------|
| Before grace period ends | `timestamp <= due_date + grace_period` | Cannot default |
| Exactly at grace deadline | `timestamp == due_date + grace_period` | Cannot default (strictly greater-than required) |
| One second past deadline | `timestamp == due_date + grace_period + 1` | Default allowed |

## Extending the Escrow Expiry

Admins can extend an invoice's due date for a held escrow using `extend_escrow_expiry`. This is a one-time operation per invoice — the extension can only be applied once.

### Rust entrypoint signature

```rust
pub fn extend_escrow_expiry(
    env: Env,
    admin: Address,
    invoice_id: BytesN<32>,
    new_due_date: u64,
) -> Result<(), QuickLendXError>;
```

### Guards

| Guard | Error on failure |
|-------|-----------------|
| Admin must be authorized | `NotAdmin` |
| Invoice must exist | `InvoiceNotFound` |
| Escrow must exist and be in `Held` status | `OperationNotAllowed` |
| Escrow must not have been extended already | `OperationNotAllowed` |
| `new_due_date` must be strictly greater than current `due_date` | `InvoiceDueDateInvalid` |
| `new_due_date` must not exceed `now + max_due_date_days * 86400` | `InvoiceDueDateInvalid` |

### Concrete example

```rust
let admin = Address::generate(&env);
client.initialize(&admin);

// Extend due_date by 30 days for a held escrow
let new_due_date = env.ledger().timestamp() + 30 * 86_400;
let res = client.extend_escrow_expiry(&admin, &invoice_id, &new_due_date);
assert!(res.is_ok());

// Second attempt fails (extension can only be applied once)
let newer_due_date = env.ledger().timestamp() + 60 * 86_400;
let res = client.try_extend_escrow_expiry(&admin, &invoice_id, &newer_due_date);
assert!(res.is_err()); // OperationNotAllowed: already extended
```

## Key Points for Operators

- **Escrows do not auto-refund.** There is no timer that automatically refunds unclaimed escrow funds. The refund/ release must be explicitly triggered by the operator or by the default handling mechanism when the invoice expires.
- **The grace period controls the default window.** After the grace period expires (due_date + grace_period_seconds), the invoice can be defaulted and escrow disposition follows.
- **Refund requires the due_date to have passed.** Investors cannot reclaim escrowed funds before the due_date.
- **Release has no time limit.** As long as the invoice is eventually paid, the escrow can be released regardless of whether the due_date has passed.
- **Extension is one-time.** `extend_escrow_expiry` can only be called once per invoice — plan the extension carefully.
- **Persistent TTL.** Escrow records have a 30-day persistent TTL (`PERSISTENT_TTL_THRESHOLD = 34_732_800` seconds). Records that are not accessed within this window may be evicted from storage, though the underlying token balance remains locked.

> **See also:**
> - [Escrow](contracts/escrow.md) — full escrow lifecycle, creation, release, and refund
> - [Escrow Refund](contracts/escrow-refund.md) — security improvements for the refund path
> - [Default Flow Diagram](DEFAULT_FLOW_DIAGRAM.md) — state machine for overdue → default → recovery
> - [Protocol Limits](contracts/protocol-limits.md) — configurable grace period, due-date horizon, and bounds
> - [OPERATOR_HANDBOOK.md](OPERATOR_HANDBOOK.md) — CLI reference for escrow operations and limit updates