# Pull Request: Reject Generated Address Lookalikes as Admin Transfer Destinations

## Description

This PR hardens admin transfer destination validation by rejecting syntactically valid Soroban addresses that do not exist on-ledger. In practice, this blocks `Address::generate(&env)`-style lookalikes from being used as direct or pending admin transfer destinations.

Closes #<issue-number>

## Threat Mitigated

Without this check, an attacker or compromised admin workflow could transfer protocol administration to a lookalike address that is valid as an `Address` value but has no backing ledger entry. That can strand admin authority at an unowned/nonexistent destination, preventing legitimate operators from rotating configuration, pausing, or performing incident response. The fix fails closed before writing `ADMIN_KEY` or `ADMIN_PENDING_KEY`.

## Changes

- Added an admin-transfer destination existence check using Soroban `Address::exists()`.
- Returned the typed Soroban contract error `QuickLendXError::InvalidAddress` for nonexistent transfer destinations.
- Added a negative test covering generated address lookalikes on both direct and two-step admin transfer initiation.
- Updated the error catalog for the new `InvalidAddress` raising site.

## Performance

Admin transfer is not a hot path; it is an operator/governance action. No instruction-budget benchmark was added.

## Verification

- `cargo fmt --all` from `quicklendx-contracts/`
- `cargo test -p quicklendx-contracts generated_address_lookalike_destinations_are_rejected`
- `cargo build --target wasm32-unknown-unknown --release`
- `cargo clippy --workspace --all-targets -- -D warnings`

Current local test/WASM verification is blocked by pre-existing crate compile errors unrelated to this change, including duplicate `recompute_investor_tier` definitions in `src/verification.rs`/`src/lib.rs`, a private `PlatformFeeConfig` import, missing `Dispute::resolution_outcome` fields in existing tests/code, and an unavailable `enable_invocation_metering` method in `src/bench.rs`. Clippy could not start because `cargo-clippy` is not applicable to the active `stable-x86_64-unknown-linux-gnu` toolchain even though `rustup component add clippy` reports the component is up to date.
