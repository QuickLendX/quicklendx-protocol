# Pull Request: Reject Generated Address Lookalikes as Admin Transfer Destinations

## Description

This PR hardens admin transfer destination validation by rejecting syntactically valid Soroban addresses that do not exist on-ledger. In practice, this blocks `Address::generate(&env)`-style lookalikes from being used as direct or pending admin transfer destinations.

Closes #<issue-number>

## Threat Mitigated

Without this check, an attacker or compromised admin workflow could transfer protocol administration to a lookalike address that is valid as an `Address` value but has no backing ledger entry. That can strand admin authority at an unowned/nonexistent destination, preventing legitimate operators from rotating configuration, pausing, or performing incident response. The fix fails closed before writing `ADMIN_KEY` or `ADMIN_PENDING_KEY`.

## Changes

- Added an admin-transfer destination existence check using Soroban `Address::exists()` in `admin.rs`.
- The check returns the typed Soroban contract error `QuickLendXError::InvalidAddress` for nonexistent transfer destinations.
- Added a new test file `tests/test_admin_lookalike.rs` with negative tests covering generated address lookalikes on both direct and two-step admin transfer initiation.
- Updated `lib.rs` to expose `transfer_admin`, `initiate_admin_transfer`, and `set_two_step_enabled` with consistent, testable signatures.

## Performance

Admin transfer is not a hot path; it is an operator/governance action. No instruction-budget benchmark was added.

## Verification

- `cargo test -p quicklendx-contracts --test test_admin_lookalike`
- `cargo build --target wasm32-unknown-unknown --release`
- `cargo clippy --workspace --all-targets -- -D warnings`