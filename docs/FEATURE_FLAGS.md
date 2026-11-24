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

## Cargo features in `quicklendx-contracts`

The crate declares four features in the `[features]` table of
`quicklendx-contracts/Cargo.toml`. **All four are off by default**, so the
standard release build (`cargo build --target wasm32v1-none --release` or
`stellar contract build`) contains none of them. Release artifacts must be
built with the default feature set only — never with `--all-features` or an
explicit `--features` flag.

| Feature | Purpose | Who enables it | Command |
|---------|---------|----------------|---------|
| `fuzz-tests` | Compiles the property/fuzz test modules gated behind `#[cfg(feature = "fuzz-tests")]` (e.g. `test_bid_compare_order_props`, `test_fuzz_tier_boundary`, `test_seed`, `test_investment_queries`) and pulls in the optional `proptest` dependency. | Contributors running the fuzz suite, via `run_fuzz_tests.sh` or directly. CI does not enable it. | `cargo test --features fuzz-tests fuzz_` (or `./run_fuzz_tests.sh quick` from the repo root) |
| `legacy-tests` | Compiles the test modules and integration targets written against the older full contract API. Eight integration targets in `Cargo.toml` declare `required-features = ["legacy-tests"]` (`auth_verification_test`, `investor_kyc_tier`, `invoice_lifecycle_e2e`, `issue_2954_bid_expiry_active_counts`, `pagination_match_over_under`, `profit_fee_golden`, `test_admin_lookalike`, `test_treasury_rotation`), and `src/test_notifications.rs` is gated the same way. | Contributors working on those suites; intentionally kept off in CI. | `cargo test --features legacy-tests` |
| `diagnostics` | Compiles `src/diagnostics.rs`: the feature-gated `qlx_log!` logging macro and the `ProtocolDiagnostics` state snapshot. Without the feature (and outside `#[cfg(test)]`), `qlx_log!` expands to a no-op at zero runtime cost, so release WASM carries no logging. | Developers debugging locally, and operators cutting an instrumented non-release build. Never release artifacts. | `cargo test --features diagnostics` or `cargo build --profile release-with-logs --features diagnostics` |
| `testutils` | Enables `soroban-sdk/testutils`, which provides the Soroban test host (`Env::default`, `mock_all_auths`, `env.register`). The crate's own dev-dependency enables it for every `cargo test` run, so it never needs to be passed by hand. | The test harness only; it is a dev-only feature. | `cargo test --features testutils` |

Notes:

- When the contract CI job is enabled, its test step is plain `cargo test`: it
  compiles no fuzz or legacy tests and leaves `diagnostics` off.
- `cargo test --all-features` builds everything at once and is fine for a local
  smoke check, but it is not the release configuration and can be slow.
- `testutils` must not be enabled for a WASM release build: it links the
  Soroban test host and inflates the artifact.

### Current status of these commands

The commands above describe the intended feature wiring. **None of them pass
end-to-end today**, because the crate root does not compile: `src/lib.rs` is
still a stub (see the temporary `if: false` on the `build` job in
`.github/workflows/ci.yml` and issue #2963), and `cargo check` currently fails
with `no variant, associated function, or constant named InvalidInput found for
enum QuickLendXError`. That failure is independent of which feature is selected.

`--features diagnostics` additionally fails with `E0428: the name qlx_log is
defined multiple times` (and the same for `assert_view_only`), because both
`src/lib.rs` and the feature-gated `src/diagnostics.rs` export those macros. The
feature builds are therefore documented as **pending** until the crate-root
stub is replaced; they should be re-verified in the PR that repairs it.

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
