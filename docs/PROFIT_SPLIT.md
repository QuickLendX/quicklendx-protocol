# Profit and Fee Split

This document explains how QuickLendX calculates the platform fee and the investor's return when an invoice is settled. It is intended for **Contributors** looking to understand or modify the core fee arithmetic.

## Overview

When an invoice is paid back, the protocol routes funds based on whether the `payment_amount` exceeds the `investment_amount` (the principal).
The entire calculation is contained in `quicklendx-contracts/src/profits.rs`.

The guiding principles are:
1. No floating-point math; we use integer division and basis points (bps).
2. All rounding truncates toward zero (i.e. floor division).
3. Any remainder (dust) from the platform fee division is absorbed by the platform, guaranteeing `investor_return + platform_fee == payment_amount`.

## The Formula

### 1. No-Profit Scenario (Underpayment or Exact Payment)

If the `payment_amount <= investment_amount`, there is no profit.
- The investor absorbs any loss up to the total investment.
- The platform takes 0 fees.

```rust
investor_return = payment_amount;
platform_fee = 0;
```

### 2. Profit Scenario

If the `payment_amount > investment_amount`, the platform assesses a fee on the *profit* only, not the principal.

```rust
// 1. Calculate the gross profit
let gross_profit = payment_amount - investment_amount;

// 2. Calculate the platform fee using basis points (10,000 bps = 100%)
// Assuming a 2% fee, fee_bps = 200
let platform_fee = (gross_profit * fee_bps) / 10_000;

// 3. The investor gets the principal plus the remainder of the profit
let investor_return = payment_amount - platform_fee;
```

*Note: The actual implementation in `profits.rs` uses `checked_mul`, `checked_div`, and `saturating_sub` to protect against overflows.*

## Concrete Example

Let's look at a settlement handled by `PlatformFee::calculate_with_fee_bps` with a `2%` platform fee (`200 bps`).

- **Principal (`investment_amount`)**: `1,000` stroops
- **Total Payment (`payment_amount`)**: `1,100` stroops
- **Fee Configuration**: `200 bps`

1. **Gross Profit**: `1,100 - 1,000 = 100` stroops
2. **Platform Fee**: `(100 * 200) / 10,000 = 2` stroops
3. **Investor Return**: `1,100 - 2 = 1,098` stroops

The platform takes `2` stroops, and the investor receives `1,098` stroops. 

### Rounding Example

If the payment was `1,001` stroops, the gross profit is `1`.
- **Platform Fee**: `(1 * 200) / 10,000 = 0` (due to integer truncation)
- **Investor Return**: `1,001 - 0 = 1,001` stroops

## Entrypoints

This math is primarily invoked during invoice settlement in `quicklendx-contracts/src/settlement.rs`. When a payment is processed, the settlement handler calls `PlatformFee::calculate` to determine exactly how many stroops to route to the investor and how many to the treasury.

For further testing and verification, see `test_profit_fee_formula.rs` and the tests at the bottom of `src/profits.rs`.

## Related Docs
- [Platform Fees Overview](PLATFORM_FEES.md)
- [Fee Configuration Guide](FEES.md)
