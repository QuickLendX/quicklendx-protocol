#!/usr/bin/env bash
# scripts/reproducible-build.sh
#
# Asserts byte-identical WASM output across two clean builds in separate
# temporary target directories with deterministic environment settings.
#
# Stellar mainnet auditors require reproducible WASM artifacts. This script:
#   1. Creates an isolated temporary directory (cleaned up automatically on exit).
#   2. Sets deterministic build variables (SOURCE_DATE_EPOCH, CARGO_INCREMENTAL=0,
#      RUSTC_WRAPPER="", TZ=UTC, LC_ALL=C, and RUSTFLAGS --remap-path-prefix).
#   3. Performs Build 1 into <tmp>/target-1 and records its SHA-256.
#   4. Performs Build 2 into <tmp>/target-2 and records its SHA-256.
#   5. Compares both SHA-256 hashes (and raw bytes); exits nonzero on mismatch.
#
# Prerequisites
# -------------
#   • rustup + cargo (channel pinned by rust-toolchain.toml)
#   • wasm32v1-none target installed (requires Rust 1.84+)
#       rustup target add wasm32v1-none
#   • sha256sum (coreutils) or shasum (macOS fallback)
#
# Usage
# -----
#   # From the repository root (both forms run the dual-build verification)
#   bash scripts/reproducible-build.sh --verify
#   bash scripts/reproducible-build.sh
#
#   # Verbose mode (prints every cargo step)
#   VERBOSE=1 bash scripts/reproducible-build.sh --verify
#
#   # Optional: also assert the resulting hash matches an expected SHA-256
#   bash scripts/reproducible-build.sh --verify --expected-hash <sha256>
#
# Exit codes
# ----------
#   0 — both hashes match (build is reproducible)
#   1 — hashes diverge, expected hash mismatch, or build failed

set -euo pipefail

# ── Argument parsing ──────────────────────────────────────────────────────────

VERIFY_MODE=1
EXPECTED_HASH="${EXPECTED_WASM_SHA256:-}"

usage() {
    cat <<'EOF'
Usage: bash scripts/reproducible-build.sh [OPTIONS]

Options:
  --verify                  Build twice in separate clean temp directories and compare SHA-256 (default)
  --expected-hash <sha256>  Additionally assert that the verified hash equals <sha256>
  -h, --help                Show this help message

Environment variables:
  SOURCE_DATE_EPOCH         Fixed UNIX timestamp for deterministic builds (default: 1700000000)
  VERBOSE=1                 Pass --verbose to cargo build
  EXPECTED_WASM_SHA256      Optional expected SHA-256 (overridden by --expected-hash)
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --verify)
            VERIFY_MODE=1
            shift
            ;;
        --expected-hash)
            if [[ $# -lt 2 ]]; then
                echo "ERROR: --expected-hash requires a SHA-256 value" >&2
                exit 1
            fi
            EXPECTED_HASH="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "ERROR: Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

# ── Configuration ─────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

WASM_TARGET="wasm32v1-none"
WASM_FILE="quicklendx_contracts.wasm"

# Fixed deterministic environment
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1700000000}"
export CARGO_INCREMENTAL=0
export RUSTC_WRAPPER=""
export TZ="UTC"
export LC_ALL="C"

CARGO_FLAGS=(
    "--manifest-path" "${REPO_ROOT}/quicklendx-contracts/Cargo.toml"
    "--target"        "${WASM_TARGET}"
    "--release"
    "--locked"
    "--no-default-features"
)

VERBOSE="${VERBOSE:-0}"
if [[ "${VERBOSE}" == "1" ]]; then
    CARGO_FLAGS+=("--verbose")
fi

# Temporary workspace for both isolated builds — never committed
TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/qlx-reproducible-build.XXXXXX")"
cleanup() {
    rm -rf "${TMP_ROOT}"
}
trap cleanup EXIT

# ── Helpers ───────────────────────────────────────────────────────────────────

native_path() {
    local p="$1"
    if command -v cygpath &>/dev/null; then
        cygpath -w "$p"
    else
        printf '%s' "$p"
    fi
}

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

build_and_hash() {
    local label="$1"
    local target_dir="$2"
    local wasm_path="${target_dir}/${WASM_TARGET}/release/${WASM_FILE}"

    mkdir -p "${target_dir}"

    local repo_native cargo_home_native rustup_home_native target_native
    repo_native="$(native_path "${REPO_ROOT}")"
    cargo_home_native="$(native_path "${CARGO_HOME:-${HOME}/.cargo}")"
    rustup_home_native="$(native_path "${RUSTUP_HOME:-${HOME}/.rustup}")"
    target_native="$(native_path "${target_dir}")"

    local remap_flags=""
    remap_flags+="--remap-path-prefix=${repo_native}=/workspace "
    remap_flags+="--remap-path-prefix=${cargo_home_native}=/cargo "
    remap_flags+="--remap-path-prefix=${rustup_home_native}=/rustup "
    remap_flags+="--remap-path-prefix=${target_native}=/target"

    local effective_rustflags="${RUSTFLAGS:+${RUSTFLAGS} }${remap_flags}"

    echo "" >&2
    echo "============================================================" >&2
    echo "  Build ${label} (CARGO_TARGET_DIR=${target_dir})" >&2
    echo "============================================================" >&2

    CARGO_TARGET_DIR="${target_dir}" \
    RUSTFLAGS="${effective_rustflags}" \
        cargo build "${CARGO_FLAGS[@]}" >&2

    if [[ ! -f "${wasm_path}" ]]; then
        echo "ERROR: Expected WASM artefact not found at ${wasm_path}" >&2
        exit 1
    fi

    local hash
    hash="$(sha256_of "${wasm_path}")"
    echo "  SHA-256 (build ${label}): ${hash}" >&2
    echo "${hash}"
}

# ── Main ──────────────────────────────────────────────────────────────────────

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  QuickLendX — WASM Reproducible-Build Verification          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

echo "==> Mode:              $( [[ "${VERIFY_MODE}" == "1" ]] && echo "verify (dual clean build)" )"
echo "==> SOURCE_DATE_EPOCH: ${SOURCE_DATE_EPOCH}"
echo "==> Temp build root:   ${TMP_ROOT}"

# Verify toolchain pin
echo "==> Toolchain version:"
rustup show active-toolchain 2>/dev/null || rustc --version

# Verify wasm target is installed
echo "==> Checking WASM target…"
if ! rustup target list --installed | grep -qx "${WASM_TARGET}"; then
    echo "Installing ${WASM_TARGET}…"
    rustup target add "${WASM_TARGET}"
fi

echo ""
echo "==> Cargo.lock status:"
if [[ -f "${REPO_ROOT}/Cargo.lock" || -f "${REPO_ROOT}/quicklendx-contracts/Cargo.lock" ]]; then
    echo "    Cargo.lock present — using pinned dependencies."
else
    echo "ERROR: Cargo.lock is missing. Reproducible builds require a committed lockfile." >&2
    exit 1
fi

# ── Build 1 & Build 2 in separate clean target directories ─────────────────────

TARGET_DIR_1="${TMP_ROOT}/target-1"
TARGET_DIR_2="${TMP_ROOT}/target-2"

HASH_1="$(build_and_hash "1" "${TARGET_DIR_1}")"
HASH_2="$(build_and_hash "2" "${TARGET_DIR_2}")"

WASM_1="${TARGET_DIR_1}/${WASM_TARGET}/release/${WASM_FILE}"
WASM_2="${TARGET_DIR_2}/${WASM_TARGET}/release/${WASM_FILE}"

# ── Compare ───────────────────────────────────────────────────────────────────

echo ""
echo "============================================================"
echo "  Reproducibility verdict"
echo "============================================================"
echo "  Build 1: ${HASH_1}"
echo "  Build 2: ${HASH_2}"
echo ""

if [[ "${HASH_1}" != "${HASH_2}" ]] || ! cmp -s "${WASM_1}" "${WASM_2}"; then
    echo "  FAIL — hashes DIVERGE. Build is non-deterministic." >&2
    echo "" >&2
    echo "  This blocks release and audit sign-off." >&2
    echo "  Common causes:" >&2
    echo "    • codegen-units != 1 in Cargo.toml [profile.release]" >&2
    echo "    • lto not set to 'fat' or 'true'" >&2
    echo "    • build.rs injecting host timestamp or env vars" >&2
    echo "    • Unremapped host/target paths in RUSTFLAGS" >&2
    echo "    • Cargo.lock not committed (non-reproducible deps)" >&2
    echo "    • Different rustc version between runs (check rust-toolchain.toml)" >&2
    exit 1
fi

if [[ -n "${EXPECTED_HASH}" && "${HASH_1}" != "${EXPECTED_HASH}" ]]; then
    echo "  FAIL — verified build hash does not match expected hash." >&2
    echo "    Expected: ${EXPECTED_HASH}" >&2
    echo "    Actual:   ${HASH_1}" >&2
    exit 1
fi

echo "  PASS — hashes match. WASM build is reproducible."
echo ""
echo "  SHA-256: ${HASH_1}"
exit 0
