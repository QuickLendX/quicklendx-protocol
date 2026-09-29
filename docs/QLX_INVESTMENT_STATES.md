# Investment State Machine

This document describes the full state machine for investments in the QuickLendX
protocol. Audience: **contributors** who need to understand how investment status
transitions are enforced on-chain and which entrypoints drive them.

For the invoice-side state machine see [docs/INVOICE_LIFECYCLE.md](INVOICE_LIFECYCLE.md).
For the internal rustdoc-style module reference see
[`quicklendx-contracts/docs/investment-lifecycle.md`](../quicklendx-contracts/docs/investment-lifecycle.md).

## State Diagram

```
                     accept_bid_and_fund
                          │
                          ▼
                     ┌──────────┐
                     │  Active  │  ◄── Only non-terminal state
                     └────┬─────┘
                          │
          ┌───────────┬───┴────┬───────────┐
          ▼           ▼        ▼           ▼
    ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
    │Completed │ │Defaulted │ │ Refunded │ │Withdrawn │
    │(Terminal)│ │(Terminal)│ │(Terminal)│ │(Terminal)│
    └──────────┘ └──────────┘ └──────────┘ └──────────┘

Active ──[settle_invoice]──────────► Completed
Active ──[handle_default]──────────► Defaulted
Active ──[refund_escrow_funds]─────► Refunded
Active ──[withdraw_investment]─────► Withdrawn
```

Terminal states (`Completed`, `Defaulted`, `Refunded`, `Withdrawn`) are
**irreversible**. No entrypoint may move an investment out of a terminal state.

## Status Reference

| Status       | Terminal? | Description                                                            |
|--------------|-----------|------------------------------------------------------------------------|
| `Active`     | No        | Funds deployed, tracked in the active-investment index.                |
| `Completed`  | Yes       | Invoice paid in full; investor received return.                        |
| `Defaulted`  | Yes       | Grace period elapsed without payment; insurance claims processed.      |
| `Refunded`   | Yes       | Escrow returned to investor due to invoice cancellation.               |
| `Withdrawn`  | Yes       | Investor voluntarily withdrew funds before a terminal outcome.         |

Source: `InvestmentStatus` in
[`quicklendx-contracts/src/types.rs`](../quicklendx-contracts/src/types.rs).

## Investment Data Structure

```rust
pub struct Investment {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub funded_at: u64,
    pub status: InvestmentStatus,
    pub insurance: Vec<InsuranceCoverage>,
}
```

## Entrypoints by Transition

### Creation → Active

```rust
contract.accept_bid(env, caller: Address, invoice_id: BytesN<32>, bid_id: BytesN<32>)
  -> Result<(), QuickLendXError>
```

- **Caller**: business owner of the invoice.
- **Preconditions**:
  - Invoice exists and is `Verified` (not already funded).
  - Bid is `Placed`, belongs to the invoice, and has not expired.
  - Business is KYC-verified and active.
  - Protocol is not paused.
- **Effect**: bid amount transferred into escrow; investment created with
  `status = Active`; bid marked `Accepted`; all other bids on this invoice
  marked `Cancelled`; escrow created with `Held` status.
- **Index**: investment added to the active-investment index (`act_inv`).

### Active → Completed (Settlement)

```rust
contract.settle_invoice(
    env, caller: Address, invoice_id: BytesN<32>,
    payment_token: Address, payment_amount: i128,
) -> Result<(), QuickLendXError>
```

- **Caller**: business owner or admin.
- **Preconditions**:
  - Invoice is `Funded` and not frozen.
  - No active dispute (`Disputed` or `UnderReview`).
  - `payment_amount` meets or exceeds the invoice amount.
  - Escrow is in `Held` status.
- **Effect**: escrow released to investor (minus platform fee); investment
  transitions to `Completed`; invoice transitions to `Paid`.
- **Insurance**: policies remain active (no automatic claims).

### Active → Defaulted

```rust
contract.trigger_default(env, caller: Address, invoice_id: BytesN<32>)
  -> Result<(), QuickLendXError>
```

- **Caller**: permissionless (anyone may trigger after the deadline).
- **Preconditions**:
  - Invoice is `Funded`.
  - `now > due_date + grace_period_seconds`.
  - Invoice has not already defaulted.
- **Effect**: investment transitions to `Defaulted`; all active insurance
  policies are claimed (`process_all_insurance_claims`); invoice transitions
  to `Defaulted`.
- **Insurance**: providers pay `coverage_amount`; premium is not returned.

### Active → Refunded

```rust
contract.refund_escrow_funds(env, caller: Address, invoice_id: BytesN<32>)
  -> Result<(), QuickLendXError>
```

- **Caller**: contract admin or business owner.
- **Preconditions**:
  - Invoice is `Funded`.
  - Escrow exists and is in `Held` status.
- **Effect**: escrow funds returned to investor; investment transitions to
  `Refunded`; invoice transitions to `Refunded`; bid transitions to
  `Cancelled`.
- **Insurance**: premiums returned to investor.

### Active → Withdrawn

```rust
contract.withdraw_investment(env, investor: Address, invoice_id: BytesN<32>)
  -> Result<(), QuickLendXError>
```

- **Caller**: the investor who owns the investment.
- **Preconditions**:
  - Investment exists and belongs to the caller.
  - Investment is `Active`.
  - Invoice is `Funded`.
  - Escrow exists and is in `Held` status.
  - Protocol is not paused.
- **Effect**: escrow funds returned to investor; investment transitions to
  `Withdrawn`; invoice restored to `Verified` (funding fields cleared);
  bid transitions to `Cancelled`.

## Status Coupling

Each investment status corresponds to a specific invoice and escrow status:

| Investment | Invoice    | Escrow    | Notes                                    |
|------------|------------|-----------|------------------------------------------|
| `Active`   | `Funded`   | `Held`    | Escrow locked; repayment window running. |
| `Completed`| `Paid`     | `Released`| Escrow released to investor minus fees.  |
| `Defaulted`| `Defaulted`| `Held`   | Frozen; insurance claims processed.      |
| `Refunded` | `Refunded` | `Refunded`| Escrow returned to investor.             |
| `Withdrawn`| `Verified` | `Refunded`| Invoice reverted; investment cancelled.  |

## Transition Enforcement

All state changes flow through a single gateway:

```rust
// InvestmentStorage::update_investment (investment.rs)
let previous_status = /* read from storage */;
InvestmentStatus::validate_transition(&previous_status, &new_status)?;
```

`validate_transition` is the canonical validator:

```rust
// quicklendx-contracts/src/investment.rs
impl InvestmentStatus {
    pub fn validate_transition(
        from: &InvestmentStatus,
        to: &InvestmentStatus,
    ) -> Result<(), QuickLendXError> {
        let allowed = match from {
            InvestmentStatus::Active => matches!(
                to,
                InvestmentStatus::Completed
                    | InvestmentStatus::Defaulted
                    | InvestmentStatus::Refunded
                    | InvestmentStatus::Withdrawn
            ),
            _ => false, // all other states are terminal
        };
        if allowed { Ok(()) } else { Err(QuickLendXError::InvalidStatus) }
    }
}
```

Any transition from a terminal state — including to itself — is rejected with
`InvalidStatus` (error code 1401).

## Active-Investment Index

The active-investment index (`act_inv`) tracks all investments currently in
`Active` status:

- **Add**: `store_investment` adds to the index when `status == Active`.
- **Remove**: `update_investment` removes from the index when transitioning
  out of `Active`.
- **Query**: `get_active_investment_ids()` returns all active investment IDs.
- **Integrity check**: `validate_no_orphan_investments()` verifies every ID in
  the index still has `status == Active`.

## Error Codes

| Error                      | Code | When raised                                             |
|----------------------------|------|---------------------------------------------------------|
| `InvalidStatus`            | 1401 | Invalid transition; investment not `Active`.            |
| `InvoiceNotFound`          | 1000 | Referenced invoice does not exist.                      |
| `InvoiceAlreadyFunded`     | 1002 | Attempting to fund an already-funded invoice.           |
| `InvoiceNotAvailableForFunding` | 1001 | Invoice not in a fundable state.                   |
| `InvoiceAlreadyDefaulted`  | 1006 | Default triggered on already-defaulted invoice.         |
| `InvoiceFrozen`            | 1007 | Operation blocked on frozen invoice.                    |
| `Unauthorized`             | 1100 | Caller is not the investor/business-owner/admin.        |
| `NotInvestor`              | 1102 | Caller does not match the investment's investor.        |
| `OperationNotAllowed`      | 1402 | Grace period not elapsed; reentrancy detected.          |
| `PaymentTooLow`            | 1403 | Settlement amount below required minimum.               |
| `ContractPaused`           | 2100 | Protocol is paused.                                     |
| `DuplicateDefaultTransition` | 2202 | Default guard already set for this invoice.           |
| `InsuranceNotActive`       | 2206 | Insurance policies exist but none are active at default.|

Full error reference: [`docs/ERROR_CODES.md`](ERROR_CODES.md).

## Worked Examples

### Example 1: Happy Path (Settlement)

```text
1. Business creates invoice (status: Pending).
2. Admin verifies invoice (status: Verified).
3. Investor places bid; business calls accept_bid_and_fund.
   → Investment created: status = Active
   → Invoice: status = Funded
   → Escrow: status = Held
4. Business calls settle_invoice with payment_amount = invoice.amount.
   → Investment: status = Completed
   → Invoice: status = Paid
   → Escrow: Released
   → Investor receives (amount − platform_fee)
```

### Example 2: Default After Grace Period

```text
1. Steps 1–3 from Example 1 above.
2. Due date + grace period elapses without payment.
3. Anyone calls trigger_default.
   → Investment: status = Defaulted
   → Insurance claims processed (providers pay coverage_amount)
   → Invoice: status = Defaulted
```

### Example 3: Refund on Cancellation

```text
1. Steps 1–3 from Example 1 above.
2. Business (or admin) calls refund_escrow_funds before settlement.
   → Investment: status = Refunded
   → Escrow funds returned to investor
   → Insurance premiums returned
   → Invoice: status = Refunded
```

### Example 4: Investor Withdrawal

```text
1. Steps 1–3 from Example 1 above.
2. Investor calls withdraw_investment before settlement.
   → Investment: status = Withdrawn
   → Escrow funds returned to investor
   → Invoice: status = Verified (funded fields cleared)
   → Invoice is available for a new bid
```

## Key Invariants

1. **Terminal states are final.** No entrypoint may change the status of an
   investment that is `Completed`, `Defaulted`, `Refunded`, or `Withdrawn`.
   Enforced by `validate_transition` in `src/investment.rs`.

2. **No orphan active investments.** The active-investment index contains only
   investments with `status == Active`. Every transition out of `Active`
   atomically removes the investment from the index.

3. **Invoice/investment status synchronization.** Terminal investment status
   always matches the parent invoice terminal state:
   - `InvoiceStatus::Paid` → `InvestmentStatus::Completed`
   - `InvoiceStatus::Defaulted` → `InvestmentStatus::Defaulted`
   - `InvoiceStatus::Refunded` → `InvestmentStatus::Refunded`

4. **Double-payout prevention.** Settlement, default, and refund paths check
   the current invoice status before executing, rejecting duplicate calls.

5. **Insurance lifecycle.** Insurance policies can only be added when the
   investment is `Active`. Claims are processed only on `Defaulted`.
   Premiums are returned only on `Refunded`.

## Related Documentation

- [Invoice Lifecycle](INVOICE_LIFECYCLE.md) — invoice state diagram and entrypoints.
- [docs/ESCROW.md](ESCROW.md) — escrow creation, release, and refund.
- [docs/DISPUTE.md](DISPUTE.md) — dispute open / review / resolve flow.
- [docs/ERROR_CODES.md](ERROR_CODES.md) — complete typed error reference.
- [docs/QUERIES.md](QUERIES.md) — read-only query entrypoints.
- [Investment Lifecycle (module docs)](../quicklendx-contracts/docs/investment-lifecycle.md) — developer reference with storage details.
- [docs/contracts/investment.md](contracts/investment.md) — full rustdoc-style investment reference.
