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
bash scripts/reproducible-build.sh --verify
```

### What the script does

1. Creates an isolated temporary directory (`mktemp -d`) with an `EXIT` trap so build artefacts are never left in the repository or committed.
2. Exports deterministic build environment variables:
   - `SOURCE_DATE_EPOCH=1700000000` (overridable via environment)
   - `CARGO_INCREMENTAL=0`
   - `RUSTC_WRAPPER=""`
   - `TZ=UTC` and `LC_ALL=C`
   - `RUSTFLAGS` with `--remap-path-prefix` mapping the repository root (`/workspace`), `CARGO_HOME` (`/cargo`), `RUSTUP_HOME` (`/rustup`), and each build's `CARGO_TARGET_DIR` (`/target`).
3. Verifies the WASM target is installed (`rustup target add wasm32v1-none`).
4. Runs `cargo build --target wasm32v1-none --release --locked --no-default-features` into `<tmp>/target-1` **(Build 1)** and records its SHA-256.
5. Runs the exact same build command into a completely separate clean directory `<tmp>/target-2` **(Build 2)** and records its SHA-256.
6. **Asserts both SHA-256 hashes and raw bytes match** (`cmp -s`). Exits `1` if they diverge.
7. Optionally asserts the verified hash equals `--expected-hash <sha256>` (or `EXPECTED_WASM_SHA256`).

### Expected output (passing)

```
╔══════════════════════════════════════════════════════════════╗
║  QuickLendX — WASM Reproducible-Build Verification          ║
╚══════════════════════════════════════════════════════════════╝

==> Mode:              verify (dual clean build)
==> SOURCE_DATE_EPOCH: 1700000000
==> Temp build root:   /tmp/qlx-reproducible-build.XXXXXX
==> Toolchain version: stable-x86_64-unknown-linux-gnu (…)
==> Checking WASM target…
==> Cargo.lock status: Cargo.lock present — using pinned dependencies.

============================================================
  Build 1 (CARGO_TARGET_DIR=/tmp/qlx-reproducible-build.XXXXXX/target-1)
============================================================
  SHA-256 (build 1): <64-hex-sha256>

============================================================
  Build 2 (CARGO_TARGET_DIR=/tmp/qlx-reproducible-build.XXXXXX/target-2)
============================================================
  SHA-256 (build 2): <64-hex-sha256>

============================================================
  Reproducibility verdict
============================================================
  Build 1: <64-hex-sha256>
  Build 2: <64-hex-sha256>

  PASS — hashes match. WASM build is reproducible.

  SHA-256: <64-hex-sha256>
```

### Verbose mode

```bash
VERBOSE=1 bash scripts/reproducible-build.sh --verify
```

---

## Expected Hash and Update Procedure

Record the verified release WASM SHA-256 hash here for each tagged release or audited commit:

| Target | `SOURCE_DATE_EPOCH` | Expected WASM SHA-256 |
|--------|---------------------|-----------------------|
| `wasm32v1-none` | `1700000000` | *(run `bash scripts/reproducible-build.sh --verify` on a clean release commit to populate)* |

To verify against a recorded hash:

```bash
bash scripts/reproducible-build.sh --verify --expected-hash <RECORDED_SHA256>
```

### Updating the recorded hash

Whenever contract source (`quicklendx-contracts/src/`), `Cargo.lock`, `Cargo.toml`, or `rust-toolchain.toml` changes for a release:

1. Ensure the working tree is clean and on the release commit.
2. Run `bash scripts/reproducible-build.sh --verify` and confirm it prints identical Build 1 and Build 2 SHA-256 hashes and exits `0`.
3. Update the **Expected WASM SHA-256** cell in the table above with the printed hash.
4. Commit the updated `docs/reproducible-builds.md` alongside the release notes / tag.

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
