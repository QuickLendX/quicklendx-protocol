# Escrow Acceptance Hardening

## Overview

The escrow funding flow now enforces a single set of preconditions before a bid can be accepted.
This applies to both public acceptance entrypoints:

- `accept_bid`
- `accept_bid_and_fund`

## Security Goals

- Ensure the caller is authorizing the exact invoice that will be funded.
- Ensure only a valid `invoice_id` and `bid_id` pair can progress.
- Prevent funding when escrow or investment state already exists for the invoice.
- Reject inconsistent invoice funding metadata before any token transfer occurs.

## Acceptance Preconditions

Before the contract creates escrow, it now checks:

- The invoice exists.
- The caller is the invoice business owner and passes business KYC state checks.
- The invoice is still available for funding.
- The invoice has no stale funding metadata:
  - `funded_amount == 0`
  - `funded_at == None`
  - `investor == None`
- The invoice does not already have:
  - an escrow record
  - an investment record
- The bid exists.
- The bid belongs to the provided invoice.
- The bid is still `Placed`.
- The bid has not expired.
- The bid amount is positive.

## Issue Addressed

Previously, `accept_bid` reloaded the invoice ID from the bid after authorizing against the caller-supplied invoice. That allowed a mismatched invoice/bid pair to drift into the funding path and risk:

- escrow being created under the wrong invoice key
- status index corruption
- unauthorized cross-invoice funding side effects

Both acceptance paths now share the same validator in [`escrow.rs`](/Users/mac/Documents/github/wave/quicklendx-protocol/quicklendx-contracts/src/escrow.rs).

## Tests Added

The escrow hardening is covered with targeted regression tests in:

- [`test_escrow.rs`](/Users/mac/Documents/github/wave/quicklendx-protocol/quicklendx-contracts/src/test_escrow.rs)
- [`test_bid.rs`](/Users/mac/Documents/github/wave/quicklendx-protocol/quicklendx-contracts/src/test_bid.rs)

New scenarios include:

- rejecting mismatched invoice/bid pairs with no balance or status side effects
- rejecting acceptance when escrow already exists for the invoice

## Security Notes

- Validation runs before any funds are transferred into escrow.
- Existing escrow or investment state is treated as a hard stop to preserve one-to-one funding invariants.
- The contract still relies on the payment reentrancy guard in [`lib.rs`](/Users/mac/Documents/github/wave/quicklendx-protocol/quicklendx-contracts/src/lib.rs).
