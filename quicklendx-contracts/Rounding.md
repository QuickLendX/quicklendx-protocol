# Rounding Strategy Documentation

## Overview

QuickLendX contracts perform integer arithmetic for all financial calculations. When converting token amounts to human‑readable decimal values (e.g., cents to dollars), the contract must decide how to round fractional values.

### Rounding Modes

- **Half‑Up (Standard) Rounding** – If the fractional part is **0.5** or greater, round **up**; otherwise round **down**. This is the typical rounding method most developers expect.
- **Banker's (Half‑Even) Rounding** – If the fractional part is **0.5**, round to the **nearest even** integer. This reduces cumulative rounding bias in large data sets and is the default in many financial libraries.

## Why It Matters

Consistent rounding ensures:
- Predictable invoice amounts for both businesses and investors.
- Accurate fee calculations and revenue sharing.
- Compliance with financial standards expected by downstream integrators.

## Implementation in Soroban Contracts

The contracts use `soroban_sdk::Env` integer math. Below are concrete examples demonstrating each rounding mode using a helper function.

```rust
/// Rounds `value` (in smallest token units) to `decimals` decimal places.
/// `mode` determines the rounding strategy.
pub fn round_amount(value: i128, decimals: u32, mode: RoundingMode) -> i128 {
    let factor: i128 = 10i128.pow(decimals);
    let half = factor / 2;
    match mode {
        RoundingMode::HalfUp => {
            // Add half then truncate (standard half‑up)
            (value + half) / factor * factor
        }
        RoundingMode::Bankers => {
            // Determine even/odd adjustment when exactly half
            let remainder = value % factor;
            if remainder > half {
                // More than half – round up
                (value + factor - remainder) / factor * factor
            } else if remainder < half {
                // Less than half – round down
                (value - remainder) / factor * factor
            } else {
                // Exactly half – round to even
                let base = (value - remainder) / factor;
                if base % 2 == 0 {
                    base * factor // even – stay
                } else {
                    (base + 1) * factor // odd – round up to make even
                }
            }
        }
    }
}

pub enum RoundingMode {
    HalfUp,
    Bankers,
}
```

## Example Usage

Assume a token with 6 decimal places (e.g., USDC). We want to round `1234567` (1.234567 USDC) to two decimal places (cents).

```rust
let rounded_half_up = round_amount(1_234_567, 2, RoundingMode::HalfUp); // → 1_235_000 (1.235 USD)
let rounded_bankers = round_amount(1_234_567, 2, RoundingMode::Bankers); // → 1_234_000 (1.234 USD) because 0.5 rounds to even
```

## References

- Wikipedia: [Rounding](https://en.wikipedia.org/wiki/Rounding)
- Rust `Decimal` crate rounding behaviours (used as inspiration).
