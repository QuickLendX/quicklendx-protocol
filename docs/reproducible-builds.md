# Reproducible WASM Builds

Stellar mainnet auditors require that the WASM artefact deployed on-chain can be
independently rebuilt from the committed source and verified to be byte-identical.
This document explains how QuickLendX achieves and tests that property.

---

## Why Reproducibility Matters

| Without reproducibility | With reproducibility |
|-------------------------|----------------------|
| Auditors must trust the deployer's build environment | Anyone can verify the on-chain WASM matches the source |
| Non-determinism can hide build-time secrets or tampering | Diverging hashes immediately signal a problem |
| Re-audit required after any toolchain update | Toolchain changes are caught by CI before they reach mainnet |

A build is reproducible when two independent, clean compilations of the same
source commit produce **byte-identical** output (matching SHA-256 hashes).

---

## What Guarantees Determinism

The following settings in `quicklendx-contracts/Cargo.toml` are **required** for
reproducible output:

```toml
[profile.release]
codegen-units = 1        # single codegen unit — no parallel ordering races
lto           = true     # link-time optimisation collapses nondeterminism
opt-level     = "z"      # fixed optimisation level
overflow-checks = true   # does not affect determinism, kept for safety
debug         = 0
strip         = true
```

Additionally:

| Requirement | File | Why |
|-------------|------|-----|
| Pinned toolchain channel | `rust-toolchain.toml` | Prevents silent rustc upgrades |
| Committed lockfile | `quicklendx-contracts/Cargo.lock` | Pins every transitive dependency version |
| `--locked` flag | CI / build script | Forces cargo to honour the lockfile exactly |
| `--no-default-features` | CI / build script | Excludes dev/test features from WASM output |
| No build-time env injection | `build.rs` (none present) | `build.rs` scripts can embed host timestamps |

---

## Running Locally

```bash
# From the repository root
bash scripts/reproducible-build.sh
```

### What the script does

1. Verifies the WASM target is installed (`rustup target add wasm32-unknown-unknown`).
2. **Cleans** all build artefacts (`cargo clean` + `rm -rf target/.../release`).
3. Runs `cargo build --target wasm32-unknown-unknown --release --locked --no-default-features` **(Build 1)**.
4. Records the SHA-256 of the resulting `.wasm` file.
5. **Cleans again**.
6. Runs the same build command **(Build 2)**.
7. Records the SHA-256.
8. **Asserts both hashes match**. Exits `1` if they diverge.

### Expected output (passing)

```
╔══════════════════════════════════════════════════════════════╗
║  QuickLendX — WASM Reproducible-Build Verification          ║
╚══════════════════════════════════════════════════════════════╝

==> Toolchain version: stable-x86_64-unknown-linux-gnu (…)
==> Checking WASM target… wasm32-unknown-unknown (installed)
==> Cargo.lock status: Cargo.lock present — using pinned dependencies.

============================================================
  Build 1
============================================================
  SHA-256 (build 1): 3a7f9c…

============================================================
  Build 2
============================================================
  SHA-256 (build 2): 3a7f9c…

============================================================
  Reproducibility verdict
============================================================
  Build 1: 3a7f9c…
  Build 2: 3a7f9c…

  ✅  PASS — hashes match. WASM build is reproducible.
```

### Verbose mode

```bash
VERBOSE=1 bash scripts/reproducible-build.sh
```

---

## CI Workflow

The workflow at `.github/workflows/wasm-reproducible-build.yml` runs automatically on:

- Every push to `main` that touches `quicklendx-contracts/`, `rust-toolchain.toml`,
  or the build script/workflow itself.
- Every pull request targeting `main` with contract changes.
- Manual trigger via `workflow_dispatch` (for audit runs on any commit).

### Key design decisions

| Decision | Rationale |
|----------|-----------|
| **No Cargo cache** (actions/cache intentionally absent) | Cache reuse would defeat the purpose — we need two genuinely independent builds |
| `CARGO_INCREMENTAL=0` env var | Disables incremental compilation that survives across runs in some setups |
| `RUSTC_WRAPPER=""` | Disables sccache or similar transparent caching proxies that CI might inject |
| `--locked` flag | CI fails fast if `Cargo.lock` is stale rather than silently resolving new deps |
| 90-day artefact retention | Provides an audit trail for every reproducible build on `main` |

The workflow uploads the final `.wasm` and a `.sha256` manifest as a GitHub Actions
artefact, providing a timestamped, immutable record of each verified build.

---

## Verifying an On-Chain Deployment

To verify that a deployed contract matches a source commit:

```bash
# 1. Check out the exact commit used for deployment
git checkout <COMMIT_SHA>

# 2. Run the reproducible build
bash scripts/reproducible-build.sh

# 3. Compare the printed hash against the deployed contract's WASM hash
#    (available via Stellar's getContractWasm RPC method)
stellar contract inspect --wasm quicklendx-contracts/target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm
```

Or compute the hash manually:

```bash
sha256sum quicklendx-contracts/target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm
```

---

## Troubleshooting Non-Determinism

If the script or CI reports diverging hashes, investigate in this order:

### 1. Toolchain mismatch

```bash
rustc --version --verbose
# Confirm Host: and commit: match between runs
```

Check `rust-toolchain.toml` — the channel must be pinned to a specific version
(e.g. `channel = "1.78.0"`) rather than `"stable"` for maximum reproducibility.

### 2. `codegen-units` not set to 1

```toml
# quicklendx-contracts/Cargo.toml
[profile.release]
codegen-units = 1   # ← REQUIRED
```

Multiple codegen units allow the compiler to process functions in parallel, which
introduces ordering non-determinism.

### 3. Stale Cargo.lock

```bash
cargo generate-lockfile --manifest-path quicklendx-contracts/Cargo.toml
git diff quicklendx-contracts/Cargo.lock
# If there is a diff, a dependency was silently upgraded.
# Commit the updated lockfile and re-run the reproducibility check.
```

### 4. `build.rs` injecting timestamps

If a dependency's `build.rs` embeds `std::time::SystemTime::now()`, `env!("OUT_DIR")`,
or similar host-specific values, the WASM output will differ between runs.
Identify the culprit with:

```bash
cargo build --target wasm32-unknown-unknown --release -vv 2>&1 | grep "build\[" | head -20
```

### 5. LTO not enabled

Without `lto = true`, the linker may process input files in a different order
depending on OS scheduler timing.

---

## Future-Proofing

Any future hash divergence **blocks release**. The process is:

1. CI reports `❌ FAIL`.
2. The PR/commit is blocked from merging.
3. The root cause is identified using the troubleshooting steps above.
4. A fix is committed (toolchain pin, lockfile update, `build.rs` fix, etc.).
5. The script and CI must pass before the change can land on `main`.

This is intentional. A deploy to Stellar mainnet with a non-reproducible artefact
cannot be independently verified by auditors.

---

## Related Files

| File | Purpose |
|------|---------|
| `scripts/reproducible-build.sh` | Local reproducibility verification script |
| `.github/workflows/wasm-reproducible-build.yml` | CI workflow enforcing the invariant |
| `rust-toolchain.toml` | Pins the Rust toolchain channel |
| `quicklendx-contracts/Cargo.toml` | Contains `[profile.release]` determinism flags |
| `quicklendx-contracts/Cargo.lock` | Pins all transitive dependency versions |
| `scripts/check-wasm-size.sh` | Size-budget check (complementary to reproducibility) |
