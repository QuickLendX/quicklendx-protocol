# Dispute Time Limits

> **Audience:** Protocol operators — the people who deploy, configure, and monitor QuickLendX Soroban contracts.

This document explains the time window within which disputes can be opened on an invoice, and how the `DisputeTimeLimitExceeded` error protects against late filings.

## Overview

Every invoice has a **dispute deadline** computed as:

```
dispute_deadline = due_date + grace_period_seconds
```

Disputes must be created on-chain *before or at* this deadline. Once the deadline has passed, `create_dispute` rejects the request with `QuickLendXError::DisputeTimeLimitExceeded` (error code 1908, ABI symbol `DSP_TL`).

| Parameter | Default | Max | Where configured |
|-----------|---------|-----|------------------|
| `grace_period_seconds` | `604_800` (7 days) | `2_592_000` (30 days) | Protocol limits (`set_protocol_limits`) |
| `due_date` | Set by business at `store_invoice` | Must be in the future, max `max_due_date_days` ahead | Invoice creation |
| `dispute_deadline` | Computed from the two above | Must satisfy `grace_period_seconds <= max_due_date_days * 86_400` | Enforced by protocol limits validation |

## Error Code

| Error | Code | ABI symbol | When it fires |
|-------|------|-----------|---------------|
| `DisputeTimeLimitExceeded` | 1908 | `DSP_TL` | `create_dispute` is called when `ledger.timestamp() > due_date + grace_period_seconds` |

## How the Deadline Works

1. When a business creates an invoice via `store_invoice`, the `due_date` is stored on the invoice record.
2. The grace period (`grace_period_seconds`) is part of the `ProtocolLimits` configuration and can be updated by the admin.
3. The dispute deadline is `due_date + grace_period_seconds`.
4. When `create_dispute` is called, the contract checks the current ledger timestamp against this deadline.
5. If the timestamp is past the deadline, the call reverts with `DisputeTimeLimitExceeded`.

### Concrete example — dispute within the window

```rust
use soroban_sdk::{testutils::Ledger as _, Address, Env, String};
use quicklendx_contracts::QuickLendXContractClient;

let env = Env::default();
let contract_id = env.register(QuickLendXContract, ());
let client = QuickLendXContractClient::new(&env, &contract_id);

let admin = Address::generate(&env);
client.initialize(&admin);

// Set protocol limits: 7-day grace period (default)
client.set_protocol_limits(
    &admin,
    100,                     // min_invoice_amount
    10,                      // min_bid_amount
    10,                      // min_bid_bps
    365,                     // max_due_date_days
    7 * 24 * 60 * 60,       // grace_period_seconds = 604_800 (7 days)
    0,                       // max_invoices_per_business
    InvestorTier::None,
);

let business = Address::generate(&env);
// ... submit and approve KYC for business ...

let now = 100_000_000u64;
env.ledger().set_timestamp(now);

// Invoice due in 10_000 seconds from now
let due_date = now + 10_000;
let invoice_id = client.store_invoice(
    &business,
    &100,
    &currency,
    &due_date,
    &String::from_str(&env, "desc"),
    &InvoiceCategory::Services,
    &Vec::new(&env),
);

// Dispute created BEFORE the deadline (due_date + 7 days)
let grace_period = 7 * 24 * 60 * 60;
let deadline = due_date + grace_period;
env.ledger().set_timestamp(deadline - 1);

let result = client.create_dispute(
    &invoice_id,
    &business,
    &String::from_str(&env, "reason"),
    &String::from_str(&env, "evidence"),
);
assert!(result.is_ok());
```

### Concrete example — dispute past the deadline is rejected

```rust
// Advance time PAST the deadline
env.ledger().set_timestamp(deadline + 1);

let result = client.try_create_dispute(
    &invoice_id,
    &business,
    &String::from_str(&env, "reason"),
    &String::from_str(&env, "evidence"),
);

assert!(result.is_err());
let err = result.unwrap_err().expect("should have error");
assert_eq!(err, QuickLendXError::DisputeTimeLimitExceeded);
```

## Boundary Cases

| Scenario | Ledger timestamp relative to deadline | Result |
|----------|---------------------------------------|--------|
| Dispute created 1 second before deadline | `timestamp == deadline - 1` | Ok |
| Dispute created exactly at deadline | `timestamp == deadline` | Ok |
| Dispute created 1 second after deadline | `timestamp == deadline + 1` | `Err(DisputeTimeLimitExceeded)` |

## Configuring the Grace Period

The grace period is part of `ProtocolLimits` and can be updated by the admin:

```bash
# Set grace period to 14 days (1,209,600 seconds)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  set_protocol_limits \
  --admin $ADMIN_ADDRESS \
  --min_invoice_amount 100 \
  --min_bid_amount 10 \
  --min_bid_bps 10 \
  --max_due_date_days 365 \
  --grace_period_seconds 1209600 \
  --max_invoices_per_business 0
```

**Boundary rules** (from `protocol_limits.rs`):

- `grace_period_seconds <= 2_592_000` (30 days maximum)
- `grace_period_seconds <= max_due_date_days * 86_400` (cannot exceed the due-date horizon)

## Interaction with the Dispute Lifecycle

```
Invoice created (due_date = D)
       │
       ▼
  Grace period begins (D → D + grace_period)
       │
       ▼
  Dispute window is open
  ┌──────────────────────────────────────────┐
  │  create_dispute() succeeds here         │
  │  Evidence can be updated while Disputed  │
  │  Admin can move to UnderReview          │
  │  Admin can resolve → Resolved            │
  └──────────────────────────────────────────┘
       │
       ▼
  Grace period expires (ledger.timestamp > D + grace_period)
       │
       ▼
  create_dispute() → Err(DisputeTimeLimitExceeded, DSP_TL)
```

## Key Points for Operators

- **Disputes do not auto-close.** Existing open disputes remain active indefinitely; the time limit only gates *new* dispute creation.
- **The grace period is configurable.** When you change `grace_period_seconds` via `set_protocol_limits`, the new value applies to all invoices created or verified after the update (existing invoices keep their original due_date).
- **The deadline is deterministic.** It is always `due_date + grace_period_seconds`, using ledger timestamps (seconds since Unix epoch on Soroban).
- **This is distinct from the invoice default flow.** The grace period also drives default-trigger logic in the invoice module, but dispute time limits apply specifically to dispute creation eligibility.

> **See also:**
> - [Protocol Limits](contracts/protocol-limits.md) — all configurable boundary values with defaults and bounds
> - [Dispute Management](contracts/dispute.md) — full dispute lifecycle and status transitions
> - [Dispute Timeline Invariants](dispute-timeline-invariants.md) — executable specification of dispute state ordering
> - [Error Codes](ERROR_CODES.md) — complete catalog including `DisputeTimeLimitExceeded` (1908)
> - [OPERATOR_HANDBOOK.md](OPERATOR_HANDBOOK.md) — CLI reference for limit updates and dispute operations