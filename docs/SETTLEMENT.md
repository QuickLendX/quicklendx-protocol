# Invoice Settlement & Fund Distribution

> **Audience:** Contributors, auditors, and integrators who need to understand
> how funds are distributed when a QuickLendX invoice is settled.
>
> This document explains the profit and fee calculation formulas, the "no dust"
> accounting invariant, and the on-chain settlement flow.

## 1. Overview

Settlement is the final stage of a successful invoice lifecycle. It occurs when the business repays the full invoice amount. The `settlement.rs` module orchestrates this process, which involves:

1.  **Receiving payment** from the business.
2.  **Calculating profit and fees** based on the difference between the payment and the original investment.
3.  **Distributing funds** to the investor (principal + profit share) and the platform (fee).
4.  **Transitioning** the invoice and investment to a terminal `Paid` / `Completed` state.

The core accounting logic is centralized in `profits.rs` to ensure consistency and auditability.

## 2. The Settlement Formula

The distribution of funds depends on the relationship between the `payment_amount` (what the business paid) and the `investment_amount` (what the investor funded).

The source of truth for this logic is `profits::PlatformFee::calculate`.

### Case 1: No Profit (Payment ≤ Investment)

When the business pays back an amount less than or equal to what the investor funded, there is no profit.

-   `gross_profit = 0`
-   `platform_fee = 0`
-   `investor_return = payment_amount`

The investor receives the entire payment, absorbing any loss if `payment_amount < investment_amount`. The platform takes no fee.

### Case 2: Profit (Payment > Investment)

When the payment exceeds the investment, a profit is generated. The platform takes a fee from this profit.

1.  **Gross Profit**:
    ```rust
    let gross_profit = payment_amount - investment_amount;
    ```

2.  **Platform Fee**: The fee is a percentage of the *gross profit*, not the total payment. The rate is configured by the admin (see `fees.rs`).
    ```rust
    // fee_bps is in basis points (e.g., 200 for 2%)
    let platform_fee = floor(gross_profit * fee_bps / 10_000);
    ```

3.  **Investor Return**: The investor receives the full payment minus the platform's fee.
    ```rust
    let investor_return = payment_amount - platform_fee;
    ```

## 3. The "No Dust" Invariant

The protocol guarantees that **no funds are lost or created** during settlement. This is the "no dust" invariant:

```
investor_return + platform_fee == payment_amount
```

This identity is preserved through the rounding strategy. Because `platform_fee` is calculated with integer floor division (rounding down), any fractional "dust" from the fee calculation is implicitly retained by the investor.

### Worked Example

-   **Investment Amount**: 10,000 USDC
-   **Payment Amount**: 11,000 USDC
-   **Platform Fee Rate**: 200 bps (2%)

1.  **Gross Profit**:
    `11,000 - 10,000 = 1,000`

2.  **Platform Fee**:
    `floor(1,000 * 200 / 10,000) = floor(20) = 20`

3.  **Investor Return**:
    `11,000 - 20 = 10,980`

4.  **Verification**:
    `10,980 (investor) + 20 (platform) = 11,000 (payment)`. The invariant holds.

## 4. On-Chain Settlement Flow

The settlement process is managed by the `settlement.rs` module.

-   **Partial Payments**: `process_partial_payment` allows businesses to make multiple payments. It records each payment, updates `total_paid`, and checks if the invoice is fully paid.
-   **Finalization**: Once `total_paid >= invoice.amount`, `settle_invoice_internal` is triggered. This function is the single point of entry for final fund distribution.
-   **Idempotency**: A finalization guard (`is_finalized`) prevents an invoice from being settled more than once, even if `settle_invoice` is called again.
-   **Dispute Interaction**: Settlement is **blocked** if an invoice has an active dispute (`dispute_status != None`). This is a critical safety feature to prevent funds from being released while their ownership is contested. See `docs/settlement-dispute-interaction.md` for details.

## 5. Cross-References

-   **Profit & Fee Logic**: `quicklendx-contracts/src/profits.rs`
-   **Settlement Flow**: `quicklendx-contracts/src/settlement.rs`
-   **Fee Configuration**: `quicklendx-contracts/src/fees.rs`
-   **Dispute Interaction**: `docs/settlement-dispute-interaction.md`