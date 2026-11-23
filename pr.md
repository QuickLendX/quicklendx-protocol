## Summary

Add comprehensive test coverage for governance proposal lifecycle transitions in the `Governable` trait. Previously, `governance.rs` had no test coverage — this change locks in the full proposal state machine (Active → Passed/Rejected → Executed) with deterministic, CI-matrix-safe tests.

## Changes

### New file
- **`quicklendx-contracts/src/test_governance.rs`** — 17 unit tests exercising the `Governable` trait via a `TestGovernance` implementor (quorum=3, voting period=10 ledgers):
  - **Proposal submission**: field validation, duplicate rejection
  - **Vote casting**: for/against tally, double-vote guard, expired-window rejection, non-active proposal rejection, nonexistent proposal rejection
  - **Finalization**: quorum+majority → Passed, majority against → Rejected, quorum not met → Rejected, window-still-open rejection, non-active rejection
  - **Execution**: `run_proposal` auto-finalizes then executes, `run_proposal` rejects non-Passed proposals
  - **Query**: `get_proposal` returns correct state, nonexistent proposal returns error

### Modified files
- **`quicklendx-contracts/src/lib.rs`** — registered `mod test_governance` under `#[cfg(test)]` (no `legacy-tests` feature gate), so these tests run on every CI matrix entry
- **`quicklendx-contracts/src/profits.rs`** — removed duplicate `compute_yield(i128, i128, i128)` definition; renamed inner `compute_yield(i128, u32, u32)` to `compute_yield_u32` and updated sole caller
- **`quicklendx-contracts/src/errors.rs`** — added missing match arms for `NoPendingTreasuryRotation` and `InvalidLedgerSequence` in `QuickLendXError → Symbol` conversion
- **`quicklendx-contracts/src/storage.rs`** — removed pre-existing stray closing brace; shortened `pnd_trs_ts` symbol to `trs_ts` (max 9 chars for `symbol_short!`)
- **`quicklendx-contracts/src/events.rs`** — shortened `tr_rot_cncl` symbol to `tr_rot_c` (max 9 chars for `symbol_short!`)
- **`quicklendx-contracts/src/idempotency.rs`** — added missing `soroban_sdk` imports (`Symbol`, `BytesN`, `Address`, `Bytes`, `Env`)
- **`quicklendx-contracts/src/test_bid_capacity_stress.rs`** — fixed missing opening brace on `if` statement
- **`quicklendx-contracts/src/test_bid_ranking_determinism.rs`** — fixed duplicate `Vec` import conflict
- **`Cargo.lock`** — ethnum 1.5.0 → 1.5.3 (compatibility with rustc 1.97)

## Testing

- All 17 new tests are deterministic: no `Date.now()`/`Math.random()`, use `env.ledger().set_sequence_number()` for window control
- `#![no_std]` discipline maintained: only `soroban_sdk` primitives
- New tests gated with `#[cfg(test)]` only (no feature flag), ensuring they run on every CI matrix entry
- `cargo check --lib` passes; all 17 governance tests pass

Closes #
