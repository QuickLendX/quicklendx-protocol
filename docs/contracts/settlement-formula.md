# Settlement formula, inputs, and update timing

This guide is for contributors who need to understand the settlement math in QuickLendX. It documents the formula implemented in [quicklendx-contracts/src/profits.rs](../../quicklendx-contracts/src/profits.rs), the inputs it consumes, and the policy for when administrative fee updates take effect.

## Audience

This is written for contributors who are reviewing settlement behavior or tracing an invoice from funding to payout.

## Inputs

The settlement calculation consumes three values:

- `investment_amount`: the principal amount originally funded by the investor.
- `payment_amount`: the total amount the business pays toward the invoice during settlement.
- `fee_bps`: the active platform fee rate, expressed in basis points, from the current platform-fee configuration.

The important detail is that the fee rate is read at settlement time, not when the investment was first placed.

## Core formula

The contract uses the profit portion of the payment as the fee base:

```text
gross_profit = max(0, payment_amount - investment_amount)
platform_fee = floor(gross_profit * fee_bps / 10_000)
investor_return = payment_amount - platform_fee
```

If the payment does not exceed the investment, the formula falls back to a no-profit case:

```text
platform_fee = 0
investor_return = payment_amount
```

## Why this shape matters

The contract intentionally charges fees only on profit, never on principal:

- `payment_amount <= investment_amount` means there is no profit to charge a fee on.
- `payment_amount > investment_amount` means the fee is taken from the positive spread only.
- The use of floor division keeps the result deterministic and prevents rounding drift from creating dust.

## Worked example

Suppose the investor funded `1_000` units, the business pays `1_100`, and the active fee is `200` bps (2%):

```text
gross_profit = 1_100 - 1_000 = 100
platform_fee = floor(100 * 200 / 10_000) = 2
investor_return = 1_100 - 2 = 1_098
```

The same logic is expressed in the contract as a pure helper that can be called with explicit values:

```rust
let (investor_return, platform_fee) =
    PlatformFee::calculate_with_fee_bps(1_000, 1_100, 200);

assert_eq!(platform_fee, 2);
assert_eq!(investor_return, 1_098);
```

## Invariants to check when reviewing a change

The settlement path should preserve these invariants:

- `investor_return + platform_fee == payment_amount`
- `platform_fee <= gross_profit` when profit exists
- no fee is collected when there is no profit

Those invariants are covered by the settlement-accounting tests in [quicklendx-contracts/src/test_settlement_accounting_identity.rs](../../quicklendx-contracts/src/test_settlement_accounting_identity.rs).

## When fee updates are rolled into settlement

Administrative fee updates are applied when settlement runs. The contract reads the current platform-fee configuration inside `PlatformFee::calculate`, so a fee change becomes effective for the next settlement that uses that configuration.

In practical terms:

1. An admin updates the platform fee configuration.
2. The new value is written to storage.
3. The next settlement call that evaluates the formula uses the updated value.
4. Already-funded invoices are not retroactively re-priced; the fee change only affects later settlement calculations.

That means the update is rolled in at settlement time rather than at funding time.

## Review checklist

When you change settlement math or fee handling, verify:

- the formula still uses `payment_amount - investment_amount` for the profit base
- the fee still uses floor division with the basis-point denominator of `10_000`
- the no-dust invariant still holds
- fee updates are documented as taking effect on subsequent settlements, not retroactively
