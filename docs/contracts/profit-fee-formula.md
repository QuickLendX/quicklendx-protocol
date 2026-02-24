# Profit and Fee Calculation Formula

This document describes the centralized profit and fee calculation formula used in the QuickLendX protocol for invoice settlement.

## Overview

When an invoice is settled, funds flow from the business (debtor) to the investor who funded the invoice. The protocol calculates:

1. **Investor Return** - The total amount returned to the investor
2. **Platform Fee** - The fee collected by the platform from any profit

## Formula

### Core Calculation

```
Given:
  - investment_amount: The original amount invested (principal)
  - payment_amount: The total payment received from the business
  - fee_bps: Platform fee in basis points (1 bps = 0.01%)

Calculate:
  1. gross_profit = max(0, payment_amount - investment_amount)
  2. platform_fee = floor(gross_profit * fee_bps / 10,000)
  3. investor_return = payment_amount - platform_fee
```

### Key Properties

| Property               | Description                                                 |
| ---------------------- | ----------------------------------------------------------- |
| **No Dust**            | `investor_return + platform_fee == payment_amount` (always) |
| **Fee on Profit Only** | Platform fee is only charged on profit, never on principal  |
| **Rounding**           | All divisions round DOWN (truncate toward zero)             |
| **Loss Protection**    | No fee is charged when payment <= investment                |

## Scenarios

### 1. Profitable Settlement (Normal Case)

```
Investment: 1,000 tokens
Payment:    1,100 tokens (10% return)
Fee Rate:   2% (200 bps)

Calculation:
  gross_profit = 1,100 - 1,000 = 100
  platform_fee = floor(100 * 200 / 10,000) = floor(2.0) = 2
  investor_return = 1,100 - 2 = 1,098

Result:
  Investor receives: 1,098 tokens (109.8% of investment)
  Platform receives: 2 tokens
  Total distributed: 1,100 tokens (no dust)
```

### 2. Exact Payment (Break-Even)

```
Investment: 1,000 tokens
Payment:    1,000 tokens (0% return)
Fee Rate:   2% (200 bps)

Calculation:
  gross_profit = 1,000 - 1,000 = 0
  platform_fee = 0 (no profit to charge fee on)
  investor_return = 1,000

Result:
  Investor receives: 1,000 tokens (100% of investment)
  Platform receives: 0 tokens
```

### 3. Underpayment (Loss Scenario)

```
Investment: 1,000 tokens
Payment:    900 tokens (10% loss)
Fee Rate:   2% (200 bps)

Calculation:
  gross_profit = max(0, 900 - 1,000) = 0
  platform_fee = 0 (no profit)
  investor_return = 900

Result:
  Investor receives: 900 tokens (90% of investment - a loss)
  Platform receives: 0 tokens (no fee on losses)
```

### 4. Overpayment (High Profit)

```
Investment: 1,000 tokens
Payment:    2,000 tokens (100% return)
Fee Rate:   2% (200 bps)

Calculation:
  gross_profit = 2,000 - 1,000 = 1,000
  platform_fee = floor(1,000 * 200 / 10,000) = 20
  investor_return = 2,000 - 20 = 1,980

Result:
  Investor receives: 1,980 tokens (198% of investment)
  Platform receives: 20 tokens
```

## Rounding Behavior

### Strategy: Round Down (Floor Division)

All fee calculations use integer floor division, which always rounds down. This has two important effects:

1. **Favors Investors**: Any fractional fees are absorbed by the platform, not the investor
2. **No Dust**: Since we compute `investor_return = payment - platform_fee`, there's never any leftover amount

### Rounding Examples

| Profit | Fee Rate | Raw Fee | Rounded Fee | Notes                        |
| ------ | -------- | ------- | ----------- | ---------------------------- |
| 1      | 2%       | 0.02    | 0           | Investor keeps full profit   |
| 49     | 2%       | 0.98    | 0           | Just under 1 token threshold |
| 50     | 2%       | 1.00    | 1           | Exact boundary               |
| 51     | 2%       | 1.02    | 1           | Rounds down to 1             |
| 99     | 2%       | 1.98    | 1           | Just under 2 token threshold |
| 100    | 2%       | 2.00    | 2           | Exact                        |

### Dust-Free Invariant

The calculation guarantees:

```
investor_return + platform_fee == payment_amount
```

This is achieved by computing:

1. First, calculate `platform_fee` using floor division
2. Then, compute `investor_return = payment_amount - platform_fee`

This subtraction-based approach ensures exact equality with no rounding errors.

## Overflow Safety

### Integer Arithmetic

All calculations use `i128` integers with saturating arithmetic:

```rust
// Saturating multiplication prevents overflow
let fee = gross_profit.saturating_mul(fee_bps);

// Checked division with fallback
let platform_fee = fee.checked_div(BPS_DENOMINATOR).unwrap_or(0);
```

### Maximum Supported Values

| Type         | Maximum Value | Notes                                |
| ------------ | ------------- | ------------------------------------ |
| Amount       | ~1.7 x 10^38  | i128::MAX                            |
| Fee BPS      | 1,000 (10%)   | Protocol limit                       |
| Safe Product | 10^37         | For fee calculation without overflow |

For practical purposes, amounts up to 10^30 (a nonillion) are safely supported.

## Configuration

### Default Settings

| Parameter                  | Value  | Description          |
| -------------------------- | ------ | -------------------- |
| `DEFAULT_PLATFORM_FEE_BPS` | 200    | 2% default fee       |
| `MAX_PLATFORM_FEE_BPS`     | 1,000  | 10% maximum fee      |
| `BPS_DENOMINATOR`          | 10,000 | 100% in basis points |

### Updating Fee Configuration

Only the contract admin can update the platform fee:

```rust
// Admin-only function
PlatformFee::set_config(env, admin, new_fee_bps)?;

// Validation:
// - new_fee_bps >= 0
// - new_fee_bps <= 1000 (10%)
```

## API Reference

### Core Functions

#### `PlatformFee::calculate(env, investment_amount, payment_amount)`

Primary calculation function. Returns `(investor_return, platform_fee)`.

```rust
let (investor_return, platform_fee) = PlatformFee::calculate(&env, 1000, 1100);
// investor_return = 1098
// platform_fee = 2
```

#### `PlatformFee::calculate_breakdown(env, investment_amount, payment_amount)`

Returns detailed breakdown for transparency and event emission.

```rust
let breakdown = PlatformFee::calculate_breakdown(&env, 1000, 1100);
// breakdown.investment_amount = 1000
// breakdown.payment_amount = 1100
// breakdown.gross_profit = 100
// breakdown.platform_fee = 2
// breakdown.investor_profit = 98
// breakdown.investor_return = 1098
// breakdown.fee_bps_applied = 200
```

#### `calculate_investor_profit(env, investment_amount, payment_amount)`

Returns only the investor's net profit (after fees).

```rust
let profit = calculate_investor_profit(&env, 1000, 1100);
// profit = 98 (gross profit 100 minus 2 fee)
```

#### `calculate_platform_fee(env, investment_amount, payment_amount)`

Returns only the platform fee amount.

```rust
let fee = calculate_platform_fee(&env, 1000, 1100);
// fee = 2
```

### Pure Functions (No Storage Access)

For frontend calculations or testing, use the `_with_fee_bps` variants:

```rust
// No environment needed - pure calculation
let (investor_return, platform_fee) =
    PlatformFee::calculate_with_fee_bps(1000, 1100, 200);

let breakdown =
    PlatformFee::calculate_breakdown_with_fee_bps(1000, 1100, 200);
```

### Treasury Split

For revenue distribution:

```rust
// Split 100 fee tokens: 50% to treasury, 50% remaining
let (treasury_amount, remaining) = calculate_treasury_split(100, 5000);
// treasury_amount = 50
// remaining = 50
```

### Validation Functions

```rust
// Verify no dust in a calculation
let is_valid = verify_no_dust(investor_return, platform_fee, payment_amount);

// Validate input amounts
validate_calculation_inputs(investment_amount, payment_amount)?;
```

## Data Types

### PlatformFeeConfig

```rust
pub struct PlatformFeeConfig {
    pub fee_bps: i128,        // Fee in basis points
    pub updated_at: u64,      // Last update timestamp
    pub updated_by: Address,  // Admin who updated
}
```

### ProfitFeeBreakdown

```rust
pub struct ProfitFeeBreakdown {
    pub investment_amount: i128,  // Original investment
    pub payment_amount: i128,     // Total payment
    pub gross_profit: i128,       // Profit before fees
    pub platform_fee: i128,       // Fee amount
    pub investor_profit: i128,    // Profit after fees
    pub investor_return: i128,    // Total to investor
    pub fee_bps_applied: i128,    // Fee rate used
}
```

## Events

### Platform Fee Updated

Emitted when the fee configuration changes:

```rust
emit_platform_fee_updated(env, &config);
// Event: (fee_bps, updated_at, updated_by)
```

### Invoice Settled

Includes fee breakdown:

```rust
emit_invoice_settled(env, &invoice, investor_return, platform_fee);
```

### Platform Fee Routed

Emitted when fees are sent to treasury:

```rust
emit_platform_fee_routed(env, invoice_id, &recipient, fee_amount);
```

## Security Considerations

### Deterministic Results

- No floating-point arithmetic
- Integer-only calculations
- Consistent rounding (always floor)
- Results are reproducible across all nodes

### Access Control

- Fee configuration changes require admin authorization
- `require_auth()` enforced on all configuration updates

### Bounds Checking

- Fee basis points validated: 0 <= fee_bps <= 1000
- Negative amounts rejected
- Overflow protected via saturating arithmetic

### Audit Trail

- All fee calculations logged via events
- Configuration changes include timestamp and admin address
- Settlement events include full fee breakdown

## Testing

### Required Test Coverage

The formula must be tested for:

1. **Basic calculations** - Normal profit scenarios
2. **Exact payment** - Zero profit case
3. **Underpayment** - Loss scenarios
4. **Overpayment** - High profit scenarios
5. **Rounding** - Edge cases near boundaries
6. **Large amounts** - Overflow safety
7. **Zero values** - Edge cases with zero investment/payment
8. **Fee rates** - Various fee percentages (0%, 2%, 5%, 10%)

### Test Invariants

Every test should verify:

```rust
// No dust invariant
assert_eq!(investor_return + platform_fee, payment_amount);

// Non-negative fee
assert!(platform_fee >= 0);

// Fee only on profit
if payment_amount <= investment_amount {
    assert_eq!(platform_fee, 0);
}
```

## Integration with Settlement

The profit/fee calculation is integrated into the settlement flow:

```rust
// In settle_invoice_internal()
let (investor_return, platform_fee) =
    crate::fees::FeeManager::calculate_platform_fee(
        env,
        investment.amount,
        total_payment,
    )?;

// Transfer to investor
transfer_funds(env, &currency, &business, &investor, investor_return)?;

// Route platform fee
if platform_fee > 0 {
    FeeManager::route_platform_fee(env, &currency, &business, platform_fee)?;
}
```

## Frontend Integration

For displaying fee calculations to users before investment:

```javascript
// JavaScript equivalent (frontend)
function calculateFees(investmentAmount, expectedPayment, feeBps = 200) {
  const grossProfit = Math.max(0, expectedPayment - investmentAmount);
  const platformFee = Math.floor((grossProfit * feeBps) / 10000);
  const investorReturn = expectedPayment - platformFee;

  return {
    investmentAmount,
    expectedPayment,
    grossProfit,
    platformFee,
    investorProfit: grossProfit - platformFee,
    investorReturn,
    effectiveReturn: (
      ((investorReturn - investmentAmount) / investmentAmount) *
      100
    ).toFixed(2),
  };
}

// Example usage
const calc = calculateFees(10000, 11000, 200);
// {
//   investmentAmount: 10000,
//   expectedPayment: 11000,
//   grossProfit: 1000,
//   platformFee: 20,
//   investorProfit: 980,
//   investorReturn: 10980,
//   effectiveReturn: "9.80"
// }
```

## Version History

| Version | Date    | Changes                                         |
| ------- | ------- | ----------------------------------------------- |
| 1.0.0   | 2025-01 | Initial implementation with centralized formula |

## References

- [Settlement Module](./settlement.md) - Settlement flow integration
- [Fees Module](./fees.md) - Complete fee system documentation
- [Events](./events.md) - Event emission details
