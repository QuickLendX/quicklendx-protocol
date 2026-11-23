# Settlement Rounding Policy

This document outlines the rounding and precision policies used by the QuickLendX settlement engine across different currencies.

## Core Principle: Integer-Only On-Chain Math

The QuickLendX Soroban contracts use \i128\ integer arithmetic exclusively. There are no floating-point operations on-chain. All invoice amounts, payments, and fees are represented in the smallest indivisible unit of the underlying currency (e.g., stroops for XLM, 6 decimal places for USDC).

## The Settlement Accounting Invariant

To prevent value leakage and rounding drift, the settlement engine enforces a strict accounting invariant during finalization:

\\\ust
investor_return + platform_fee == total_paid
\\\

If this exact equality does not hold, the settlement transaction will unconditionally revert with \QuickLendXError::InvalidAmount\. 

## Fee Calculation and Rounding

Because the platform fee and investor return are calculated as proportions of the \	otal_paid\ amount, division is required.

1. **Flooring**: Division operations in the fee engine floor the result (truncating any fractional remainder).
2. **Remainder Allocation**: Any fractional remainder lost during the calculation of the \platform_fee\ is implicitly kept in the \investor_return\. The investor return is generally calculated as \	otal_paid - platform_fee\ to ensure the invariant is met perfectly.

## Currency-Specific Handling

Since the contract relies entirely on the integer representation provided by the caller and the token contract:

- **XLM (Stellar Lumens)**: 7 decimal places of precision. Represented in stroops.
- **USDC (Stellar)**: 7 decimal places of precision.
- **Other Tokens**: The settlement engine is agnostic to the token's decimal places. It operates purely on the integer amount provided.

Off-chain clients and indexers must read the token's metadata to format the \i128\ values for display.
