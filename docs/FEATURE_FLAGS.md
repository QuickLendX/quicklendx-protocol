# Feature Flags

Audience: **contributors** adding a gated capability to the QuickLendX
contract.

QuickLendX has no general-purpose dynamic feature-flag service. On-chain code
favours explicit, auditable gates over runtime configuration. In practice a
"feature flag" in this codebase is one of two things:

1. A **pause gate** — toggle an existing entrypoint on/off at runtime via the
   admin pause controls (`pause.rs`).
2. A **compile-time gate** — a Cargo feature that includes/excludes code from
   the build.

This document shows how to add each, how to roll it out, and how to clean it up.

## 1. Runtime gate (pause control)

The contract ships an admin-controlled pause switch. While paused, protected
state-changing entrypoints reject calls; read-only entrypoints stay available.

### How to add a runtime gate

Add the gated entrypoint to the pause check in `is_entrypoint_paused`:

```rust
// quicklendx-contracts/src/pause.rs
pub fn is_entrypoint_paused(env: &Env, entrypoint: String) -> bool {
    if !Self::is_paused(env) {
        return false;
    }
    entrypoint == String::from_str(env, "upload_invoice")
        || entrypoint == String::from_str(env, "place_bid")
        || entrypoint == String::from_str(env, "accept_bid")
        // your new gated entrypoint:
        || entrypoint == String::from_str(env, "my_new_action")
}
```

Then guard the entrypoint body with `require_not_paused`:

```rust
pub fn my_new_action(env: Env, caller: Address) -> Result<(), QuickLendXError> {
    PauseControl::require_not_paused(&env)?;
    // ... business logic ...
    Ok(())
}
```

### How to roll out

```text
pause(admin)     # admin halts protected entrypoints
unpause(admin)   # admin resumes normal operation
```

- Ship the new entrypoint behind the pause gate.
- Keep the deployment paused (or pause it) until the rollout is verified.
- `unpause(admin)` once monitoring confirms healthy behaviour.
- Inspect state any time with `is_paused()` / `is_entrypoint_paused(name)`.

### How to clean up

A runtime gate is permanent infrastructure, not temporary scaffolding. The
only "cleanup" is removing an entrypoint from the pause list if it should no
longer be pausable — do this in `is_entrypoint_paused` and drop the
`require_not_paused` call from the body in the same PR.

## 2. Compile-time gate (Cargo feature)

For code that should not ship at all in some builds (e.g. test-only helpers,
experimental entrypoints), use a Cargo feature rather than a runtime flag — it
keeps the WASM binary smaller and the surface auditable.

### How to add a compile-time gate

Declare the feature in `quicklendx-contracts/Cargo.toml`:

```toml
[features]
# off by default; opt in with `--features experimental_payouts`
experimental_payouts = []
```

Gate the code with `#[cfg(feature = ...)]`:

```rust
#[cfg(feature = "experimental_payouts")]
pub fn experimental_payout(env: Env /* ... */) -> Result<(), QuickLendXError> {
    // ...
}
```

### How to roll out

- Default the feature **off** so the standard
  `cargo build --target wasm32-unknown-unknown --release` excludes it.
- Build the gated variant explicitly:
  `cargo build --features experimental_payouts`.
- Add a CI matrix entry that builds and tests with the feature enabled so the
  gated path does not bit-rot.

### How to clean up

When the feature graduates (or is abandoned):

1. Delete the `[features]` entry in `Cargo.toml`.
2. Remove every `#[cfg(feature = "...")]` attribute, keeping the code
   unconditionally (graduated) or deleting it (abandoned).
3. Remove the dedicated CI matrix entry.
4. `cargo build` and `cargo clippy --workspace --all-targets -- -D warnings`
   to confirm nothing referenced the removed feature.

## Discipline

- Keep `#![no_std]`: gates use `soroban_sdk` primitives, never `std::`.
- A flag is debt. Every flag you add should have a removal plan recorded in its
  introducing PR.

## See also

- [`docs/GOVERNANCE.md`](GOVERNANCE.md) — admin authority behind the pause
  controls.
- [`docs/RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) — using
  pause during an incident.
