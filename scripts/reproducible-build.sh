#!/usr/bin/env bash
# scripts/reproducible-build.sh
#
# Asserts byte-identical WASM output across two clean back-to-back builds.
#
# Stellar mainnet auditors require reproducible WASM artifacts.  This script:
#   1. Cleans ALL Cargo build artefacts to prevent cache contamination.
#   2. Performs build #1 and records the SHA-256 of the resulting .wasm file.
#   3. Cleans again.
#   4. Performs build #2 and records the SHA-256.
#   5. Asserts both hashes match; exits 1 if they diverge.
#
# A diverging hash signals non-determinism (build-time timestamps, non-stable
# codegen ordering, or host-toolchain mismatch) and BLOCKS release.
#
# Prerequisites
# -------------
#   • rustup + cargo (channel pinned by rust-toolchain.toml)
#   • wasm32-unknown-unknown target installed
#       rustup target add wasm32-unknown-unknown
#   • sha256sum (coreutils) or shasum (macOS fallback)
#
# Usage
# -----
#   # From the repository root
#   bash scripts/reproducible-build.sh
#
#   # Verbose mode (prints every cargo step)
#   VERBOSE=1 bash scripts/reproducible-build.sh
#
# Exit codes
# ----------
#   0 — both hashes match (build is reproducible)
#   1 — hashes diverge or build failed

set -euo pipefail

# ── Configuration ─────────────────────────────────────────────────────────────

WASM_TARGET="wasm32-unknown-unknown"
PACKAGE="quicklendx-contracts"
WASM_FILE="quicklendx_contracts.wasm"
WASM_PATH="quicklendx-contracts/target/${WASM_TARGET}/release/${WASM_FILE}"

# Extra flags forwarded to every `cargo build` call.
# --locked  : honour the committed Cargo.lock exactly
# --no-default-features : exclude dev features from WASM artefact
CARGO_FLAGS=(
    "--manifest-path" "quicklendx-contracts/Cargo.toml"
    "--target"        "${WASM_TARGET}"
    "--release"
    "--locked"
    "--no-default-features"
)

VERBOSE="${VERBOSE:-0}"
if [[ "${VERBOSE}" == "1" ]]; then
    CARGO_FLAGS+=("--verbose")
fi

# ── Helper: sha256 ────────────────────────────────────────────────────────────

sha256_of() {
    local file="$1"
    if command -v sha256sum &>/dev/null; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum &>/dev/null; then
        shasum -a 256 "$file" | awk '{print $1}'
    else
        echo "ERROR: neither sha256sum nor shasum found in PATH" >&2
        exit 1
    fi
}

# ── Helper: clean build artefacts ─────────────────────────────────────────────

clean_build() {
    echo "==> Cleaning build artefacts…"
    cargo clean --manifest-path quicklendx-contracts/Cargo.toml \
        --target "${WASM_TARGET}" \
        2>/dev/null || true
    # Belt-and-suspenders: remove the release dir directly if cargo clean
    # leaves anything behind (e.g. incremental caches under a different key).
    rm -rf "quicklendx-contracts/target/${WASM_TARGET}/release"
}

# ── Helper: build and capture hash ────────────────────────────────────────────

build_and_hash() {
    local label="$1"

    echo "" >&2
    echo "============================================================" >&2
    echo "  Build ${label}" >&2
    echo "============================================================" >&2

    cargo build "${CARGO_FLAGS[@]}" >&2

    if [[ ! -f "${WASM_PATH}" ]]; then
        echo "ERROR: Expected WASM artefact not found at ${WASM_PATH}" >&2
        exit 1
    fi

    local hash
    hash="$(sha256_of "${WASM_PATH}")"
    echo "  SHA-256 (build ${label}): ${hash}" >&2
    echo "${hash}"
}

# ── Main ──────────────────────────────────────────────────────────────────────

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  QuickLendX — WASM Reproducible-Build Verification          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Verify toolchain pin
echo "==> Toolchain version:"
rustup show active-toolchain 2>/dev/null || rustc --version

# Verify wasm target is installed
echo "==> Checking WASM target…"
rustup target list --installed | grep "${WASM_TARGET}" || {
    echo "Installing ${WASM_TARGET}…"
    rustup target add "${WASM_TARGET}"
}

echo ""
echo "==> Cargo.lock status:"
if [[ -f quicklendx-contracts/Cargo.lock ]]; then
    echo "    Cargo.lock present — using pinned dependencies."
else
    echo "ERROR: Cargo.lock is missing. Reproducible builds require a committed lockfile." >&2
    exit 1
fi

# ── Build 1 ───────────────────────────────────────────────────────────────────

clean_build
HASH_1="$(build_and_hash "1")"

# ── Build 2 ───────────────────────────────────────────────────────────────────

clean_build
HASH_2="$(build_and_hash "2")"

# ── Compare ───────────────────────────────────────────────────────────────────

echo ""
echo "============================================================"
echo "  Reproducibility verdict"
echo "============================================================"
echo "  Build 1: ${HASH_1}"
echo "  Build 2: ${HASH_2}"
echo ""

if [[ "${HASH_1}" == "${HASH_2}" ]]; then
    echo "  ✅  PASS — hashes match. WASM build is reproducible."
    echo ""
    echo "  Artefact: ${WASM_PATH}"
    echo "  SHA-256:  ${HASH_1}"
    exit 0
else
    echo "  ❌  FAIL — hashes DIVERGE. Build is non-deterministic." >&2
    echo "" >&2
    echo "  This blocks release and audit sign-off." >&2
    echo "  Common causes:" >&2
    echo "    • codegen-units != 1 in Cargo.toml [profile.release]" >&2
    echo "    • lto not set to 'fat' or 'true'" >&2
    echo "    • build.rs injecting host timestamp or env vars" >&2
    echo "    • Cargo.lock not committed (non-reproducible deps)" >&2
    echo "    • Different rustc version between runs (check rust-toolchain.toml)" >&2
    exit 1
fi
