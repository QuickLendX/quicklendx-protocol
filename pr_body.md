# PR Summary: Add BID_OVERBID_POLICY.md documentation

**Closes #1936**

## Changes

1. **Created `docs/BID_OVERBID_POLICY.md`** - Documents what happens when a bid exceeds the invoice amount (the "overbid" case)

2. **Updated `README.md`** - Added cross-link to the new document in the Documentation section

## Document Content

The new document covers:
- **Summary**: Overbids are rejected at placement time with error `InvoiceAmountInvalid` (code 1003, ABI `INV_AI`)
- **Code location**: `quicklendx-contracts/src/verification.rs::validate_bid()` line 804-806
- **Call chain**: `place_bid` → `validate_bid` → returns error before any state mutation
- **Error details**: Full error code table entry
- **Concrete example**: Rejected `try_place_bid` call with assertion
- **What integrators see**: RPC error 1003, empty event log, unchanged contract state
- **Why no refund path**: Validation runs before any storage write or token transfer
- **Related validation rules**: Full ordered list of all checks in `validate_bid`
- **Test boundary**: Reference to `test_bid_validation_edge_cases` test
- **Cross-references**: ERROR_CODES.md, BID_RANKING.md, source files
- **Maintenance checklist**: Steps to update when policy changes

## Verification

- `cargo build` ✅
- `cargo clippy --lib` ✅ (no warnings)
- Bid-related unit tests pass ✅ (13/13)
- Document is linked from top-level README.md ✅
- Follows existing doc style (BID_RANKING.md, ERROR_CODES.md patterns) ✅

## Audience

Written for **contributors and downstream integrators** who need to understand the rejection boundary and error code — not for operators or end users.