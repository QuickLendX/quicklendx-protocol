# Bid Overbid Policy

> **Audience:** contributors and integrators who need to understand what happens when a bid exceeds the invoice amount.
>
> This document explains the validation boundary and the rejection path — there is no "refund" because the overbid never enters the system.

## 1. Summary

When an investor places a bid whose `bid_amount` exceeds the `invoice.amount`, the contract **rejects the bid at placement time** with error `InvoiceAmountInvalid` (code **1003**, ABI symbol `INV_AI`). The bid is not stored, no escrow is created, and no funds move. There is no on-chain refund flow for overbids because the transaction reverts before any state change occurs.

## 2. Where the check lives

The check is in `quicklendx-contracts/src/verification.rs` inside `validate_bid`:

```rust
if bid_amount > invoice.amount {
    return Err(QuickLendXError::InvoiceAmountInvalid);
}
```

This runs **before** any storage write, before escrow creation, and before any token transfer.

## 3. Call chain

```
place_bid (lib.rs)
  └─► validate_bid (verification.rs)  ← returns InvoiceAmountInvalid here
        ├─► bid_amount ≤ 0            → stored
        ├─► escrow           → created
        └─► token transfer   → executed
```

If `validate_bid` returns an error, none of the three downstream steps happen.

## 4. Error details

| Property | Value |
|----------|-------|
| **Error variant** | `QuickLendXError::InvoiceAmountInvalid` |
| **Numeric code** | `1003` |
| **ABI symbol** | `INV_AI` |
| **Category** | Invoice lifecycle (1000–1007) |
| **Meaning** | Amount violates invoice-specific constraints (e.g., bid-to-invoice bounds) |

See [`docs/ERROR_CODES.md`](ERROR_CODES.md) for the full table.

## 5. Example: rejected overbid

```rust
// Setup: invoice.amount = 1_000 (XLM)
let invoice_amount = 1_000i128;
let overbid_amount = 1_001i128;  // exceeds invoice amount
let expected_return = 1_200i128;

let result = client.try_place_bid(&investor, &invoice_id, &overbid_amount, &expected_return);

assert_eq!(result, Err(Ok(QuickLendXError::InvoiceAmountInvalid)));
// No bid record created, no escrow, no funds transferred
```

## 6. What an integrator sees

- **RPC response**: `ContractError(ErrorContractResult::Err(1003))`
- **Event log**: empty — no `bid_placed`, no `escrow_created`
- **Contract state**: unchanged — `BidStorage::get_bids_for_invoice` returns the same list

## 7. Why no refund path?

The check runs in a pure validation function that **does not mutate storage**. The Soroban host reverts the entire transaction when the entry point returns an `Err`, so:

- No bid ID is allocated
- No escrow record is written
- No token allowance is consumed
- No gas is spent on token transfers

This is cheaper and safer than accepting the bid and then refunding.

## 8. Related validation rules (same function)

`validate_bid` also enforces, in order:

| Check | Error |
|-------|-------|
| `bid_amount ≤ 0` | `InvalidAmount` (1200) |
| Invoice not `Verified` | `InvalidStatus` (1401) |
| Invoice past due date | `InvalidStatus` (1401) |
| Business bidding on own invoice | `Unauthorized` (1100) |
| Bid below protocol minimums | `InvalidAmount` (1200) |
| **Bid exceeds invoice amount** | **`InvoiceAmountInvalid` (1003)** |
| `expected_return ≤ bid_amount` | `InvalidAmount` (1200) |
| Investor capacity / KYC | `InvestorNotVerified` (1605) / `OperationNotAllowed` (1402) |
| Duplicate active bid on same invoice | `OperationNotAllowed` (1402) |

## 9. Testing the boundary

Run the concrete test in the contract crate:

```bash
cd quicklendx-contracts
cargo test test_bid_validation_basic -- --nocapture
```

The test `test_bid_validation_basic` in `src/test_bid_validation.rs` asserts:

```rust
// Bid amount exceeding invoice amount should fail
let result = validate_bid(&env, &invoice, invoice_amount + 1, expected_return, &investor);
assert_eq!(result.unwrap_err(), QuickLendXError::InvoiceAmountInvalid);
```

## 10. Cross-references

- [`docs/ERROR_CODES.md`](ERROR_CODES.md) — complete error code catalog
- [`docs/BID_RANKING.md`](BID_RANKING.md) — how valid bids are ordered once placed
- `quicklendx-contracts/src/verification.rs` — `validate_bid` implementation
- `quicklendx-contracts/src/test_bid_validation.rs` — validation test suite

## 11. Maintenance checklist

When changing the overbid rule:

1. Update `validate_bid` in `quicklendx-contracts/src/verification.rs`
2. Add a test case in `src/test_bid_validation.rs`
3. Update the error code table in `docs/ERROR_CODES.md` if a new code is introduced
4. Update this document if the policy changes (e.g., if overbids become partial fills)
5. Run `cargo test` and `cargo clippy --workspace --all-targets -- -D warnings`