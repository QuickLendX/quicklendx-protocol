# Decimal Handling in QuickLendX Contracts

## Overview

QuickLendX contracts do **not** perform decimal normalization. All monetary amounts are stored and processed in the token's native units (smallest denomination). Callers are responsible for providing amounts in the correct scale for each token type.

## Design Rationale

The contract avoids decimal normalization for three key reasons:

1. **Token Agnostic**: Different tokens use different decimal places (e.g., USDC uses 6 decimals, many ERC-20 tokens use 18 decimals, XLM uses 7 decimals). Normalizing to a single standard would require querying each token's decimals and performing scaling operations, increasing gas costs and complexity.

2. **Precision Preservation**: Scaling operations can introduce rounding errors. By working in native units, the contract preserves exact precision as defined by each token contract.

3. **Soroban Token Interface**: The Stellar Asset Contract (SAC) and Soroban token interface handle decimal conversions at the token level. The QuickLendX contract simply passes amounts through to these interfaces.

## Amount Representation

### In Contract Storage

All monetary values in contract storage use `i128` type and represent amounts in the token's smallest unit:

```rust
pub struct Invoice {
    pub amount: i128,           // Native token units
    pub funded_amount: i128,    // Native token units
    // ...
}

pub struct Bid {
    pub bid_amount: i128,       // Native token units
    pub expected_return: i128,  // Native token units
    // ...
}
```

### Default Minimum Amounts

The protocol uses different defaults for production vs. testing:

```rust
// src/protocol_limits.rs
#[cfg(not(test))]
const DEFAULT_MIN_AMOUNT: i128 = 1_000_000; // 1 token (6 decimals - USDC style)
#[cfg(test)]
const DEFAULT_MIN_AMOUNT: i128 = 10;       // Smaller for tests
```

**Note**: The comment "6 decimals" indicates this default is calibrated for USDC-style tokens. For tokens with different decimals, admins should configure appropriate minimum amounts via `set_protocol_limits`.

## Integration Guide

### For Frontend/Backend Integrators

When integrating with QuickLendX, you must convert human-readable amounts to native token units before calling contract functions.

#### Example: USDC (6 decimals)

```typescript
// Human-readable: $100.00 USDC
const humanAmount = 100.00;
const usdcDecimals = 6;
const nativeAmount = BigInt(humanAmount * 10 ** usdcDecimals); // 100_000_000

// Call contract with nativeAmount
contract.store_invoice(business, nativeAmount, currency, dueDate, description, category, tags);
```

#### Example: 18-decimal Token (e.g., DAI-style)

```typescript
// Human-readable: $100.00 of 18-decimal token
const humanAmount = 100.00;
const tokenDecimals = 18;
const nativeAmount = BigInt(humanAmount * 10 ** tokenDecimals); // 100_000_000_000_000_000_000

// Call contract with nativeAmount
contract.store_invoice(business, nativeAmount, currency, dueDate, description, category, tags);
```

#### Example: XLM (7 decimals)

```typescript
// Human-readable: 100 XLM
const humanAmount = 100.00;
const xlmDecimals = 7;
const nativeAmount = BigInt(humanAmount * 10 ** xlmDecimals); // 1_000_000_000

// Call contract with nativeAmount
contract.store_invoice(business, nativeAmount, currency, dueDate, description, category, tags);
```

### For Contract Contributors

When adding new functions that handle monetary amounts:

1. **Always use `i128`** for amount parameters
2. **Document the expected scale** in function docstrings
3. **Never perform scaling** inside the contract
4. **Validate amounts** against protocol limits (which are also in native units)

```rust
/// Store an invoice with the specified amount.
///
/// # Arguments
/// * `amount` - Invoice amount in the token's smallest unit (e.g., 1_000_000 for $1.00 USDC)
///
/// # Errors
/// * `InvalidAmount` if amount is less than the configured minimum
pub fn store_invoice(
    env: Env,
    business: Address,
    amount: i128,  // Native token units - no scaling performed
    currency: Address,
    due_date: u64,
    description: String,
    category: InvoiceCategory,
    tags: Vec<String>,
) -> Result<BytesN<32>, QuickLendXError>
```

## Token Transfer Behavior

The contract uses the standard Soroban token interface for transfers, which operates in native units:

```rust
// src/payments.rs
let token_client = token::Client::new(env, currency);
token_client.transfer(from, to, &amount);  // amount is in native units
token_client.transfer_from(spender, from, to, &amount);
```

The token contract itself handles any necessary decimal conversions for its internal accounting.

## Protocol Limits Configuration

When configuring protocol limits for tokens with non-standard decimals, adjust the minimum amounts accordingly:

```rust
// For a 18-decimal token, you might set:
let min_invoice_amount = 1_000_000_000_000_000_000; // 1 token in 18 decimals

// For a 6-decimal token (USDC):
let min_invoice_amount = 1_000_000; // 1 token in 6 decimals
```

Use the `set_protocol_limits` admin function to apply these values:

```rust
contract.set_protocol_limits(
    env,
    admin,
    min_invoice_amount,  // Must match token decimals
    min_bid_amount,
    min_bid_bps,
    max_due_date_days,
    grace_period_seconds,
    max_invoices_per_business,
)?;
```

## Testing Considerations

Test fixtures use simplified amounts for efficiency. When writing integration tests:

```rust
// Test setup often uses small amounts
let test_amount = 1_000i128; // Works for any token in test context

// For realistic testing with specific tokens, use appropriate scales
let usdc_amount = 100_000_000i128;   // $100 USDC (6 decimals)
let dai_amount = 100_000_000_000_000_000_000i128; // $100 DAI (18 decimals)
```

## Common Pitfalls

### 1. Mixing Decimal Scales

**Wrong**: Passing human-readable amounts directly
```rust
// ❌ Don't do this
contract.store_invoice(business, 100, currency, due_date, ...); // 100 what?
```

**Correct**: Convert to native units first
```rust
// ✅ Do this instead
let amount = 100 * 1_000_000; // $100 USDC in native units
contract.store_invoice(business, amount, currency, due_date, ...);
```

### 2. Assuming Fixed Decimals

**Wrong**: Hardcoding 6-decimal assumptions
```rust
// ❌ Don't assume all tokens are 6 decimals
let amount = human_amount * 1_000_000;
```

**Correct**: Query token decimals or use token-specific logic
```rust
// ✅ Handle different decimals appropriately
let amount = match token_type {
    TokenType::USDC => human_amount * 1_000_000,
    TokenType::DAI => human_amount * 1_000_000_000_000_000_000,
    TokenType::XLM => human_amount * 10_000_000,
};
```

### 3. Rounding Errors in Off-chain Calculations

When calculating fees or percentages off-chain, perform calculations in native units to avoid precision loss:

```rust
// ❌ Calculate in human units then convert (potential rounding)
let fee = (human_amount * fee_bps / 10000) * 1_000_000;

// ✅ Calculate in native units directly
let fee = native_amount * fee_bps / 10000;
```

## Security Considerations

1. **Amount Validation**: The contract validates amounts against configured minimums but cannot validate that callers used the correct decimal scaling. This is the caller's responsibility.

2. **Overflow Protection**: All arithmetic uses `checked_*` or `saturating_*` operations to prevent overflow, regardless of the decimal scale used.

3. **Precision Loss**: By avoiding scaling operations, the contract eliminates a class of potential precision-loss bugs.

## References

- Soroban Token Interface: https://developers.stellar.org/docs/build/smart-contracts/tokens
- Stellar Asset Contract (SAC): https://developers.stellar.org/docs/build/smart-contracts/token-contracts
- Protocol Limits Configuration: See `src/protocol_limits.rs` and `src/init.rs`
