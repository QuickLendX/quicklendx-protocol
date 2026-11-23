# Resolution Policy Per Contract

> **Audience:** operators, downstream contracts, frontend engineers, and
> support staff who need to know what happens to each contract module when
> a dispute is resolved.
>
> **Source of truth:** `quicklendx-contracts/src/resolution_policy.rs`
> (pure mappings, no storage mutation).

## Overview

When a dispute reaches `Resolved` status the platform admin records a
structured outcome via `resolve_dispute_structured`:

| Outcome           | Code | Enum variant             | Meaning |
|-------------------|------|--------------------------|---------|
| FavorBusiness     | 1    | `DisputeResolution::FavorBusiness` | Business's position is upheld |
| FavorInvestor     | 2    | `DisputeResolution::FavorInvestor` | Investor's position is upheld |
| Split             | 3    | `DisputeResolution::Split`         | Funds/obligations split between parties |
| Dismissed         | 4    | `DisputeResolution::Dismissed`     | Closed without finding for either side |

The resolution **does not automatically mutate** invoice status, escrow,
or investment records.  Instead, it records the outcome, and the
**resolution policy** (this document) defines what downstream actions
are expected for each contract.

## Resolution Policy By Contract

### 1. Invoice Contract

Source: `src/invoice.rs`, `src/types.rs`

| Outcome           | Effect on invoice status       | Code constant |
|-------------------|--------------------------------|---------------|
| FavorBusiness     | `StaysFunded` — invoice stays `Funded`; settlement proceeds normally | `InvoiceEffect::StaysFunded` |
| FavorInvestor     | `TransitionToRefunded` — admin moves invoice to `Refunded`; escrow refund unlocks | `InvoiceEffect::TransitionToRefunded` |
| Split             | `PendingAdminAction` — admin determines next invoice status | `InvoiceEffect::PendingAdminAction` |
| Dismissed         | `StaysFunded` — dispute was unfounded; invoice returns to normal processing | `InvoiceEffect::StaysFunded` |

**Details:**

- **FavorBusiness**: The dispute was resolved in favour of the business.
  The invoice remains `Funded` (or returns to `Funded` if it was moved
  during the dispute).  The business may continue making payments toward
  settlement.

- **FavorInvestor**: The dispute was resolved in favour of the investor.
  The admin should call `update_invoice_status(invoice_id, Refunded)`,
  which unlocks `refund_escrow_funds()` so the investor recovers their
  principal.  Settlement is permanently blocked.

- **Split**: Both parties share responsibility.  The admin determines the
  next invoice status based on the specific split terms.  Common choices:
  keep `Funded` with adjusted payment terms, or move to `Refunded` for a
  partial refund (requires a separate `refund_escrow` for the partial
  amount).

- **Dismissed**: The dispute was rejected as unfounded.  The invoice
  returns to normal processing as if the dispute never occurred.

### 2. Settlement Contract

Source: `src/settlement.rs`

| Outcome           | Effect on settlement          | Code constant |
|-------------------|-------------------------------|---------------|
| FavorBusiness     | `Unblocked` — settlement finalisation resumes | `SettlementEffect::Unblocked` |
| FavorInvestor     | `PermanentlyBlocked` — settlement never finalises | `SettlementEffect::PermanentlyBlocked` |
| Split             | `BlockedPendingAdmin` — settlement blocked until admin acts | `SettlementEffect::BlockedPendingAdmin` |
| Dismissed         | `Unblocked` — settlement resumes normally | `SettlementEffect::Unblocked` |

**Details:**

- **FavorBusiness**: The `dispute_status` is `Resolved` but the dispute
  no longer blocks settlement.  `settle_invoice()` and
  `process_partial_payment()` may proceed normally.  The settlement
  module checks `dispute_status == Resolved` with a FavorBusiness
  outcome and allows finalisation.

- **FavorInvestor**: The invoice is expected to transition to `Refunded`,
  which is a terminal non-settleable status (`ensure_payable_status`
  rejects `Refunded`).  Settlement is permanently impossible.

- **Split**: Settlement remains blocked.  The admin must decide whether
  to allow partial settlement or trigger a refund.  While blocked,
  `record_payment()` continues to function (to preserve payment history)
  but `settle_invoice_internal()` rejects finalisation.

- **Dismissed**: Same as FavorBusiness — settlement unblocks.

### 3. Escrow Contract

Source: `src/escrow.rs`, `src/payments.rs`

| Outcome           | Effect on escrow              | Code constant |
|-------------------|-------------------------------|---------------|
| FavorBusiness     | `ReleaseToBusiness` — escrow released when invoice is `Paid` | `EscrowEffect::ReleaseToBusiness` |
| FavorInvestor     | `RefundToInvestor` — escrow refunded to investor | `EscrowEffect::RefundToInvestor` |
| Split             | `HeldPendingAdmin` — escrow stays `Held` until admin acts | `EscrowEffect::HeldPendingAdmin` |
| Dismissed         | `ReleaseToBusiness` — normal release path | `EscrowEffect::ReleaseToBusiness` |

**Details:**

- **FavorBusiness**: The escrow stays `Held`.  Once the business
  completes payment and the invoice reaches `Paid`, `release_escrow()`
  transfers funds to the business.

- **FavorInvestor**: The invoice transitions to `Refunded`, which allows
  `refund_escrow_funds()` to return the escrowed amount to the investor.
  The escrow status changes `Held → Refunded`.

- **Split**: The escrow remains `Held`.  Neither release nor refund is
  automatic.  The admin must determine the split ratio and execute the
  appropriate operation(s).  This may involve a partial release + partial
  refund if the contract supports it, or a full refund followed by an
  off-chain settlement.

- **Dismissed**: Same as FavorBusiness — normal release path.

### 4. Bid Contract

Source: `src/bid.rs`

| Outcome           | Effect on bids                | Code constant |
|-------------------|-------------------------------|---------------|
| FavorBusiness     | `Unchanged` — bids already final at funding time | `BidEffect::Unchanged` |
| FavorInvestor     | `Unchanged` — bid may be restored if invoice reverts to `Verified` | `BidEffect::Unchanged` |
| Split             | `Unchanged` — bid status unchanged | `BidEffect::Unchanged` |
| Dismissed         | `Unchanged` — bid status unchanged | `BidEffect::Unchanged` |

**Details:**

Bids are finalised at funding time (transitioned `Placed → Accepted` or
`Cancelled`).  Dispute resolution does not reverse bid statuses.
In the FavorInvestor case where the invoice returns to `Verified`, the
original bid remains `Accepted` (not `Placed`) and a new bid must be
placed if re-funding is needed.

### 5. Investment Contract

Source: `src/investment.rs`, `src/types.rs`

| Outcome           | Effect on investment          | Code constant |
|-------------------|-------------------------------|---------------|
| FavorBusiness     | `StaysActive` → eventually `Completed` via settlement | `InvestmentEffect::StaysActive` |
| FavorInvestor     | `TransitionToRefunded` — investment marked `Refunded` | `InvestmentEffect::TransitionToRefunded` |
| Split             | `PendingAdminAction` — investment stays `Active` | `InvestmentEffect::PendingAdminAction` |
| Dismissed         | `StaysActive` → normal settlement | `InvestmentEffect::StaysActive` |

**Details:**

- **FavorBusiness**: The investment remains `Active`.  When settlement
  completes, the investment transitions `Active → Completed` and the
  investor receives their return plus platform fees.

- **FavorInvestor**: The invoice transitions to `Refunded`, which causes
  the investment to be marked `Refunded`.  The investor's principal is
  returned via the escrow refund.

- **Split**: The investment stays `Active` while the admin determines
  the split terms.  No automatic transition occurs.

- **Dismissed**: Same as FavorBusiness — investment stays `Active`.

## Policy Versioning

The resolution policy is versioned so past resolutions remain
interpretable after policy updates.

| Version | Date       | Changes |
|---------|------------|---------|
| 1       | 2026-07-24 | Initial policy definition |

Current version: `RESOLUTION_POLICY_VERSION = 1`

## Running the Policy Tests

```bash
cd quicklendx-contracts
cargo test resolution_policy
```

Expected output:

```
running 6 tests
test resolution_policy::tests::test_all_outcomes_have_policy ... ok
test resolution_policy::tests::test_outcome_labels ... ok
test resolution_policy::tests::test_policy_for_dismissed ... ok
test resolution_policy::tests::test_policy_for_favor_business ... ok
test resolution_policy::tests::test_policy_for_favor_investor ... ok
test resolution_policy::tests::test_policy_for_split ... ok
test result: ok. 6 passed; 0 failed
```

## See Also

- [Dispute Lifecycle](../DISPUTE.md) — full dispute state machine
- [Contract docs: dispute.md](dispute.md) — entrypoint reference
- [Settlement Accounting](../../docs/SETTLEMENT_ACCOUNTING.md) — how
  settlement interacts with dispute resolution
- [Escrow Lifecycle](escrow.md) — escrow state machine
- [Investment Lifecycle](investment.md) — investment transitions
- `quicklendx-contracts/src/resolution_policy.rs` — source of truth
