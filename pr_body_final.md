Closes #1936

## Summary
Added `docs/BID_OVERBID_POLICY.md` documenting what happens when a bid exceeds the invoice amount.

## Changes
- Created `docs/BID_OVERBID_POLICY.md` explaining the overbid rejection policy
- Linked the new document from `README.md`

## Policy Details
When an investor places a bid whose `bid_amount` exceeds `invoice.amount`, the contract rejects the bid at placement time with error `InvoiceAmountInvalid` (code 1003, ABI symbol `INV_AI`). The bid is not stored, no escrow is created, and no funds move — the transaction reverts before any state change.

There is no on-chain refund flow for overbids because the validation runs in a pure function (`validate_bid`) that mutates no storage. The Soroban host reverts the entire transaction when the entrypoint returns an error.

## Acceptance Criteria
- [x] Change matches the summary (documents overbid rejection)
- [x] New document linked from top-level doc (README.md)
- [x] Examples compile (reuses existing test helper patterns from `test_bid_validation.rs`)
- [x] Lint, type-check, tests pass locally (`cargo clippy --lib`, `cargo test --lib test_bid`)