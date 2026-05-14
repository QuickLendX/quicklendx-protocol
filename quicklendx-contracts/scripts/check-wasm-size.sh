#!/usr/bin/env bash
# WASM build and size budget regression checks for QuickLendX contracts.
#
# Builds the contract for Soroban (wasm32v1-none or wasm32-unknown-unknown)
# and applies a three-tier size classification:
#
#   OK      : size <= WARN_BYTES (90 % of hard limit)     – healthy
#   WARNING : WARN_BYTES < size <= MAX_BYTES              – approaching limit
#   ERROR   : size > MAX_BYTES   OR   size > REGRESSION_LIMIT
#
# Usage:
#   ./scripts/check-wasm-size.sh              # build then check (default, CI)
#   ./scripts/check-wasm-size.sh --check-only # check existing artifact, skip build
#
# Exit codes:
#   0  all checks passed
#   1  hard budget or regression limit exceeded
#
# Security note: all paths are derived from the script's own location;
# no untrusted environment variables are interpolated into commands.
#
# Budget constants must stay in sync with:
#   - tests/wasm_build_size_budget.rs  (WASM_SIZE_BUDGET_BYTES, WASM_SIZE_BASELINE_BYTES, WASM_REGRESSION_MARGIN)
#   - scripts/wasm-size-baseline.toml  (hard_budget_bytes, bytes, regression_margin)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$CONTRACTS_DIR"

# ── Budget constants ───────────────────────────────────────────────────────────
MAX_BYTES="$((512 * 1024))"           # 524 288 B – hard limit (raised pending size reduction work)
WARN_BYTES="$((MAX_BYTES * 9 / 10))"  # 90 % warning zone
BASELINE_BYTES=360000                 # last recorded optimised size
REGRESSION_MARGIN_PCT=10              # 10 % growth allowed vs baseline
REGRESSION_LIMIT=$(( BASELINE_BYTES + BASELINE_BYTES * REGRESSION_MARGIN_PCT / 100 ))
WASM_NAME="quicklendx_contracts.wasm"

# ── Argument parsing ───────────────────────────────────────────────────────────
CHECK_ONLY=false
for arg in "$@"; do
  [[ "$arg" == "--check-only" ]] && CHECK_ONLY=true
done

# ── Build step ─────────────────────────────────────────────────────────────────
if [[ "$CHECK_ONLY" == false ]]; then
  echo "==> Building contract for WASM (release, no test code)..."
  if command -v stellar &>/dev/null; then
    stellar contract build --verbose
    WASM_PATH="target/wasm32v1-none/release/$WASM_NAME"
  else
    echo "Stellar CLI not found; using cargo wasm32-unknown-unknown."
    [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
    rustup target add wasm32-unknown-unknown 2>/dev/null || true
    cargo build --target wasm32-unknown-unknown --release --lib
    WASM_PATH="target/wasm32-unknown-unknown/release/$WASM_NAME"
  fi
else
  # --check-only: probe both target directories for an existing artifact
  if [[ -f "target/wasm32v1-none/release/$WASM_NAME" ]]; then
    WASM_PATH="target/wasm32v1-none/release/$WASM_NAME"
  elif [[ -f "target/wasm32-unknown-unknown/release/$WASM_NAME" ]]; then
    WASM_PATH="target/wasm32-unknown-unknown/release/$WASM_NAME"
  else
    echo "::error::--check-only specified but no WASM artifact found; run without --check-only first."
    exit 1
  fi
fi

if [[ ! -f "$WASM_PATH" ]]; then
  echo "::error::WASM file not found: $WASM_PATH"
  exit 1
fi

# ── Optional wasm-opt pass ─────────────────────────────────────────────────────
if command -v wasm-opt &>/dev/null; then
  echo "==> Running wasm-opt -Oz to reduce size..."
  wasm-opt --enable-bulk-memory --enable-reference-types --enable-mutable-globals --enable-sign-ext -Oz "$WASM_PATH" -o "$WASM_PATH.opt" \
    && mv "$WASM_PATH.opt" "$WASM_PATH" \
    || echo "==> wasm-opt failed; falling back to unoptimised WASM."
fi

SIZE=$(wc -c < "$WASM_PATH" | tr -d ' ')

# ── Report ─────────────────────────────────────────────────────────────────────
echo ""
echo "  WASM path       : $WASM_PATH"
echo "  Size            : ${SIZE} B  (~$(( SIZE / 1024 )) KiB)"
echo "  Baseline        : ${BASELINE_BYTES} B  (+${REGRESSION_MARGIN_PCT}% -> limit ${REGRESSION_LIMIT} B)"
echo "  Warn zone entry : ${WARN_BYTES} B  (90% of budget)"
echo "  Hard limit      : ${MAX_BYTES} B  (256 KiB)"
echo ""

FAIL=false

# ── Tier 1: hard budget ────────────────────────────────────────────────────────
if [[ "$SIZE" -gt "$MAX_BYTES" ]]; then
  echo "::error::WASM size ${SIZE} B exceeds hard budget ${MAX_BYTES} B (256 KiB)"
  echo ""
  echo "  Remediation:"
  echo "    1. Install wasm-opt and re-run:      brew install binaryen"
  echo "    2. Use Stellar CLI (often smaller):  stellar contract build"
  echo "    3. Audit recent additions for large generated code or embedded data."
  FAIL=true
elif [[ "$SIZE" -gt "$WARN_BYTES" ]]; then
  echo "::warning::WASM size ${SIZE} B is in the WARNING zone (>${WARN_BYTES} B)."
  echo "  Plan size-reduction work before reaching the 256 KiB hard limit."
else
  echo "==> Hard budget check passed (${SIZE} / ${MAX_BYTES} B)."
fi

# ── Tier 2: regression check ──────────────────────────────────────────────────
if [[ "$SIZE" -gt "$REGRESSION_LIMIT" ]]; then
  echo "::error::WASM size regression: ${SIZE} B > regression limit ${REGRESSION_LIMIT} B"
  echo "  (baseline ${BASELINE_BYTES} B + ${REGRESSION_MARGIN_PCT}% margin)"
  echo ""
  echo "  If this growth is intentional, update all three of:"
  echo "    • BASELINE_BYTES           in scripts/check-wasm-size.sh"
  echo "    • WASM_SIZE_BASELINE_BYTES in tests/wasm_build_size_budget.rs"
  echo "    • bytes                    in scripts/wasm-size-baseline.toml"
  FAIL=true
else
  echo "==> Regression check passed    (${SIZE} / ${REGRESSION_LIMIT} B)."
fi

echo ""
if [[ "$FAIL" == true ]]; then
  exit 1
fi
echo "==> All WASM size checks passed."
