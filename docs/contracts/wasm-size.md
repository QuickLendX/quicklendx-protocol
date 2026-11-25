# WASM Size Budget

#### Contract size budget

The release build is tuned for minimal WASM size (`opt-level = "z"`, LTO, strip,
`codegen-units = 1`). The contract **must** stay within the size budget to be
accepted by the Stellar network.

##### Three-tier size classification

| Tier        | Range                            | Behaviour                                                                   |
| ----------- | -------------------------------- | --------------------------------------------------------------------------- |
| **OK**      | 0 – 235 929 B (≤ 90 % of budget) | Green – no action needed.                                                   |
| **Warning** | 235 930 – 262 144 B (90 – 100 %) | Yellow – plan a reduction effort; CI prints a diagnostic but does not fail. |
| **Over**    | > 262 144 B (> 256 KiB)          | Red – CI **fails**; artifact is rejected by the Stellar network.            |

##### Regression detection

In addition to the hard budget, CI compares the current build against a recorded
**baseline** (last known good size). If the artifact grows by more than **5 %**
relative to the baseline, CI fails even if the hard budget is not yet broken.
This catches incremental drift before it becomes a problem.

| Constant            | Value                     | Location                                                                                           |
| ------------------- | ------------------------- | -------------------------------------------------------------------------------------------------- |
| Hard budget (opt.)  | 262 144 B (256 KiB)       | `quicklendx-contracts/tests/wasm_build_size_budget.rs`, `quicklendx-contracts/scripts/check-wasm-size.sh`, `quicklendx-contracts/scripts/wasm-size-baseline.toml` |
| Raw fallback budget | 327 680 B (320 KiB)       | `quicklendx-contracts/tests/wasm_build_size_budget.rs` – used when `wasm-opt` absent (e.g. Windows local builds)        |
| Warning zone        | 235 929 B (90 %)          | `quicklendx-contracts/tests/wasm_build_size_budget.rs`                                                                  |
| Regression baseline | 217 668 B (last recorded) | `quicklendx-contracts/tests/wasm_build_size_budget.rs`, `quicklendx-contracts/scripts/wasm-size-baseline.toml`                               |
| Regression margin   | 5 %                       | `quicklendx-contracts/tests/wasm_build_size_budget.rs`, `quicklendx-contracts/scripts/check-wasm-size.sh`                                    |

##### Update procedure after a legitimate size increase

When adding features that intentionally grow the binary, update all three
locations in the **same PR** that introduces the growth:

```bash
# 1. Build and note the optimised size
cd quicklendx-contracts
./scripts/check-wasm-size.sh   # prints "WASM size: <N> bytes"

# 2. Update the baseline (replace 217668 with the new value)
#    a. tests/wasm_build_size_budget.rs  → WASM_SIZE_BASELINE_BYTES
#    b. scripts/check-wasm-size.sh       → BASELINE_BYTES
#    c. scripts/wasm-size-baseline.toml  → [baseline].bytes and recorded
```

##### Local enforcement (recommended before every push)

```bash
# Shell script – also used by CI
./scripts/check-wasm-size.sh

# Rust integration tests – also run in CI
cargo test --test wasm_build_size_budget
```

Both enforce all three tiers. The shell script uses `stellar contract build`
when the Stellar CLI is available, otherwise falls back to
`cargo build --target wasm32-unknown-unknown --release`. Either path runs
`wasm-opt -Oz` if the `binaryen` package is installed
(`brew install binaryen` / `apt install binaryen`).

Test-only code is excluded from the release artifact via `#[cfg(test)]`; the
regression tests verify this by building without the `testutils` feature.

##### Size reduction tips

- Use `stellar contract build` (produces `wasm32v1-none` binaries, typically 5–15 % smaller than `wasm32-unknown-unknown`).
- Install `wasm-opt` (`brew install binaryen`) and run the script — it applies `-Oz` post-compilation.
- Keep test helpers behind `#[cfg(test)]`; avoid `#[cfg(not(test))]` guards on large data.
- Audit `soroban-sdk` feature flags: enabling only `alloc` (not `testutils`) keeps the footprint minimal in release.
- Prefer `#[contracttype]` enums over large `String`/`Bytes` payloads for on-chain data.
