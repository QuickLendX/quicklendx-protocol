#!/usr/bin/env bash
# WASM build and size budget check for QuickLendX contracts.
# Builds the contract for Soroban (wasm32v1-none or wasm32-unknown-unknown),
# then asserts the WASM size is within the budget (256 KB).
# Ensures no test-only code in release build (test code is behind #[cfg(test)]).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$CONTRACTS_DIR"

# Size budget: 256 KB (network deployment limit)
MAX_BYTES="$((256 * 1024))"
WASM_NAME="quicklendx_contracts.wasm"

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

if [[ ! -f "$WASM_PATH" ]]; then
  echo "::error::WASM file not found: $WASM_PATH"
  exit 1
fi

SIZE=$(wc -c < "$WASM_PATH" | tr -d ' ')
echo "WASM path: $WASM_PATH"
echo "WASM size: $SIZE bytes (max $MAX_BYTES bytes)"

if [[ "$SIZE" -gt "$MAX_BYTES" ]]; then
  echo "::error::WASM size $SIZE exceeds budget $MAX_BYTES bytes (256 KB)"
  exit 1
fi

echo "==> WASM size within budget (256 KB). OK."
