# Settlement & Fund Splits

This document explains the settlement lifecycle in the QuickLendX Protocol, specifically detailing how funds are routed and split across the investor, platform treasury, and the borrowing business upon successful invoice repayment. 

This guide is primarily for **contributors** needing to understand the settlement logic or verify the accounting invariants in the smart contract.

## Overview

When an invoice reaches the `Funded` state, the business must repay the `invoice_amount` by the due date. Once the total payments equal the invoice amount, the contract automatically finalizes the settlement, marks the invoice as `Paid`, and disburses the collected funds.

The final repayment is split into two parts:
1. **Platform Fee**: A percentage of the **profit** (not the principal), routed to the platform treasury.
2. **Investor Return**: The remaining amount, which covers the original principal (`investment_amount`) plus their share of the profit, routed to the investor.

> **Note**: Settlement can be triggered either by a single, full payment (`settle_invoice`) or cumulatively via multiple partial payments (`process_partial_payment`).

## Accounting Invariants

The protocol enforces a strict accounting invariant during finalization to prevent fund drift:
`investor_return + platform_fee == invoice.total_paid`

Additionally, the system ensures that the business is never charged more than the remaining due amount. Overpayment attempts are capped at the remaining due balance.

## The Split Formula

The split is calculated based on the configured Platform Fee Basis Points (BPS), where 10,000 BPS = 100%.

1. `profit = payment_amount - investment_amount`
2. `platform_fee = (profit * fee_bps) / 10_000`
3. `investor_return = payment_amount - platform_fee`

If the payment amount is less than or equal to the investment amount (e.g., in a forced recovery or specific dispute resolution), the platform fee is exactly `0`, and all funds go to the investor.

## Concrete Example

Let's assume the following scenario:
- **Invoice Amount (`payment_amount`)**: `1,000` USDC
- **Investment Amount (Principal)**: `900` USDC
- **Platform Fee Rate**: `200` BPS (2%)

### Step-by-Step Calculation:
1. **Profit**: `1,000 - 900 = 100` USDC
2. **Platform Fee**: `(100 * 200) / 10,000 = 2` USDC
3. **Investor Return**: `1,000 - 2 = 998` USDC

**Output Verification:**
- Investor receives: `998` USDC (which is `900` principal + `98` profit)
- Platform receives: `2` USDC
- Total disbursed: `998 + 2 = 1,000` USDC (matches the payment amount).

## Contract Entrypoints

### `settle_invoice`
Used for exact remaining-due settlement.

```rust
pub fn settle_invoice(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
) -> Result<(), QuickLendXError>
```
**Pre-conditions**:
- `invoice.status` must be `Funded`.
- `payment_amount` must exactly equal the remaining amount due.
- Double-settlement must not have occurred.

### `process_partial_payment`
Used for incremental payments. If the cumulative `total_paid` reaches `invoice.amount`, it triggers the internal `settle_invoice_internal` automatically.

```rust
pub fn process_partial_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    transaction_id: String,
) -> Result<(), QuickLendXError>
```

## Emitted Events

Upon successful settlement, the following events are published to the ledger for downstream integrators:

- **`pay_rec`** (`emit_payment_recorded`): Fired for the final payment applied.
- **`inv_stlf`** (`emit_invoice_settled_final`): Fired upon finalization, indicating the total amount settled and timestamp.

## Additional References
- [Fee Configuration & Platform Treasury Operations](contracts/platform-fee-ops.md)
- [Dispute Interaction & Escrow Routing](DISPUTE.md)
