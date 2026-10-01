# Pull Request: #1536 Add cancel_treasury_rotation admin entrypoint

## Description

This PR introduces a `cancel_treasury_rotation` admin entrypoint, permitting an authorized admin to abort a pending treasury address rotation before its timelock expires and it can be executed.

This is a defense-in-depth hardening measure. An inability to cancel a pending rotation represents a potential security gap where a mistaken or malicious change could become permanent. This change closes that gap.

Closes #1536

## Threat Mitigated

Without this check, two risk scenarios exist:
1.  **Operational Error:** An admin accidentally initiates a rotation to a wrong address (e.g., a typo, or an address for the wrong network). Without a cancellation function, the protocol is locked into this incorrect state until the timelock expires, at which point the treasury function could be permanently broken if the destination is un-ownable.
2.  **Compromised Admin Key:** An attacker gains control of an admin key and initiates a rotation to an address they control. A cancellation function provides a critical safety valve, allowing the legitimate operators to regain control and abort the malicious transfer before it executes.

This entrypoint mitigates these threats by allowing a swift correction, preventing irreversible errors or fund redirection.

## Changes

- Added a new `cancel_treasury_rotation` function to the `QuickLendXContract` implementation, gated to the current admin via `require_auth()`.
- The new function removes the pending treasury address and its execution timestamp from persistent storage, effectively aborting the rotation.
- Added a `NoPendingTreasuryRotation` error to the `QuickLendXError` enum, which is returned if the function is called when no rotation is pending.
- Added a `treasury_rotation_cancelled` event that is emitted upon successful cancellation.
- Created a new integration test file, `tests/test_treasury_rotation.rs`, with test cases for:
    - Successful cancellation by an admin.
    - Failure when no rotation is pending.
    - Auth failure when called by a non-admin.

## Verification

- `cargo fmt --all`
- `cargo test -p quicklendx-contracts --test test_treasury_rotation`
- `cargo build --target wasm32-unknown-unknown --release`
- `cargo clippy --workspace --all-targets -- -D warnings`