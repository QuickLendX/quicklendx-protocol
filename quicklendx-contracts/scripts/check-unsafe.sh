#!/usr/bin/env bash
# check-unsafe.sh – cargo-geiger unsafe-code gate for QuickLendX Soroban contracts.
#
# THREAT MODEL
# ============
# An `unsafe` block that reaches the WASM binary can bypass Rust's memory-safety
# guarantees inside the Soroban runtime.  If an attacker can trigger a memory
# corruption bug (use-after-free, out-of-bounds write, type confusion) via a
# crafted contract call, they may be able to:
#   • Read or overwrite ledger state that does not belong to them.
#   • Subvert authorization checks (require_auth) by corrupting Address objects.
#   • Crash the host VM, enabling a denial-of-service attack against the network.
#
# None of the QuickLendX first-party sources require `unsafe`.  The soroban-sdk
# itself uses a small number of carefully reviewed `unsafe` blocks for FFI and
# memory layout.  Any `unsafe` that appears *outside* the allow-listed crates is
# therefore unexpected and must be reviewed before merging.
#
# HOW IT WORKS
# ============
# 1. `cargo geiger --all-features` scans the dependency graph and counts
#    unsafe blocks / functions per crate.
# 2. This script parses that output and fails if any crate that is NOT on the
#    ALLOWED_CRATES list contains unsafe usage.
# 3. The check runs against the native host target (not wasm32) so that
#    cargo-geiger can instrument and analyse the code.
#
# WHITELISTED CRATES
# ==================
# Only explicitly listed crates may contain unsafe code.  When a new transitive
# dependency legitimately requires unsafe, a contributor must:
#   1. Add the crate name below with a justification comment.
#   2. Get a reviewer to confirm the unsafe usage is sound.
#   3. Update this file in the same PR as the dependency addition.
#
# Justifications for current entries:
#   soroban-sdk          – Soroban runtime FFI; maintained by Stellar Development Foundation.
#   soroban-env-common   – Shared environment types with platform-level unsafe (SDF).
#   soroban-env-guest    – Guest-side host-function bindings via extern "C" (SDF).
#   getrandom            – OS entropy via syscalls; required for cryptographic keys.
#   libsecp256k1         – C-FFI bindings to the secp256k1 library.
#   sha2                 – SIMD acceleration via portable_simd/intrinsics.
#   subtle               – Constant-time operations that must avoid compiler opts.
#   num-bigint           – Big-integer arithmetic with unsafe digit manipulation.
#   zeroize              – Secure memory zeroing via volatile writes.
#   ppv-lite86           – SIMD/AVX2 acceleration for ChaCha20 used by getrandom.
#   proc-macro2          – Procedural macro plumbing; no runtime unsafe.
#   byteorder            – Byte-order conversions using transmute.
#   serde                – Serialization framework with unsafe for performance.
#   serde_json           – JSON impl with unsafe string operations.
#
# NOTE: This list uses substring matching (grep -F), so entries should be as
# specific as possible to avoid accidental allowances.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# ---------------------------------------------------------------------------
# Crates that are explicitly allowed to contain unsafe code.
# Each entry is matched as a fixed substring against the crate name column.
# Keep entries sorted alphabetically for easy auditing.
# ---------------------------------------------------------------------------
ALLOWED_CRATES=(
  "byteorder"
  "getrandom"
  "libsecp256k1"
  "num-bigint"
  "ppv-lite86"
  "proc-macro2"
  "serde"
  "serde_json"
  "sha2"
  "soroban-env-common"
  "soroban-env-guest"
  "soroban-sdk"
  "subtle"
  "zeroize"
)

echo "=== cargo-geiger unsafe-code gate ==="
echo "Working directory: ${CONTRACTS_DIR}"
echo ""
echo "Allowed crates:"
for crate in "${ALLOWED_CRATES[@]}"; do
  echo "  - ${crate}"
done
echo ""

# Run cargo geiger and capture output.
# --all-features ensures fuzz/testutils code is also scanned.
# --include-tests is NOT passed: we only gate production code.
# --quiet suppresses the compile progress spam; only the table is emitted.
GEIGER_OUTPUT=$(
  cd "${CONTRACTS_DIR}"
  cargo geiger --all-features --quiet 2>&1
) || {
  echo "ERROR: cargo geiger failed to run."
  echo "Install it with: cargo install cargo-geiger"
  exit 1
}

echo "--- geiger output ---"
echo "${GEIGER_OUTPUT}"
echo "--- end geiger output ---"
echo ""

# ---------------------------------------------------------------------------
# Parse violations.
#
# cargo-geiger table format (columns separated by whitespace):
#
#   !   crate-name version  unsafe_fns  unsafe_exprs  unsafe_impls  unsafe_traits  unsafe_methods
#
# Lines starting with '!' indicate the crate HAS unsafe usage.
# Lines starting with  ' ' (space / no bang) are clean.
#
# We extract every crate name on a '!'-prefixed line, then reject any that
# are not in ALLOWED_CRATES.
# ---------------------------------------------------------------------------

VIOLATIONS=0
VIOLATION_LIST=""

while IFS= read -r line; do
  # Lines with unsafe usage start with '!'
  if [[ "${line}" == \!* ]]; then
    # Extract the crate name (second whitespace-delimited field after '!')
    crate_name=$(echo "${line}" | awk '{print $2}')
    [[ -z "${crate_name}" ]] && continue

    # Check if this crate is in the allow-list
    allowed=0
    for allowed_crate in "${ALLOWED_CRATES[@]}"; do
      if echo "${crate_name}" | grep -qF "${allowed_crate}"; then
        allowed=1
        break
      fi
    done

    if [[ "${allowed}" -eq 0 ]]; then
      VIOLATIONS=$((VIOLATIONS + 1))
      VIOLATION_LIST="${VIOLATION_LIST}  - ${crate_name} (from line: ${line})\n"
    fi
  fi
done <<< "${GEIGER_OUTPUT}"

if [[ "${VIOLATIONS}" -gt 0 ]]; then
  echo ""
  echo "SECURITY GATE FAILED: ${VIOLATIONS} crate(s) contain unsafe code outside the allow-list."
  echo ""
  echo "Violating crates:"
  printf "%b" "${VIOLATION_LIST}"
  echo ""
  echo "To fix:"
  echo "  1. Remove the unsafe block from the crate if possible."
  echo "  2. If the unsafe usage is justified, add the crate name to the"
  echo "     ALLOWED_CRATES list in scripts/check-unsafe.sh with a comment"
  echo "     explaining why the unsafe code is sound and necessary."
  echo "  3. Get a security reviewer to approve the addition before merging."
  exit 1
fi

echo "OK: No unexpected unsafe code detected."
echo "    All unsafe usage is confined to allow-listed crates."
exit 0
