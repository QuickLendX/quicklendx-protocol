#!/usr/bin/env bash
# scripts/check-wasm-size.sh
#
# Builds the QuickLendX contract for Soroban (wasm32, release) and asserts
# the output is within the 256 KB network deployment size limit.
#
# Usage:
#   cd quicklendx-contracts
#   ./scripts/check-wasm-size.sh
#
# Exit codes:
#   0 — build succeeded and WASM is within budget
#   1 — build failed or WASM exceeds 256 KB

set -euo pipefail

WASM_TARGET="wasm32-unknown-unknown"
PACKAGE_NAME="quicklendx-contracts"
WASM_PATH="target/${WASM_TARGET}/release/${PACKAGE_NAME//-/_}.wasm"
SIZE_LIMIT_BYTES=$((256 * 1024))   # 256 KB

echo "==> Building ${PACKAGE_NAME} for Soroban (release)..."

cargo build \
    --target "${WASM_TARGET}" \
    --release \
    --no-default-features

if [[ ! -f "${WASM_PATH}" ]]; then
    echo "ERROR: Expected WASM output not found at ${WASM_PATH}" >&2
    exit 1
fi

WASM_SIZE=$(stat -c%s "${WASM_PATH}" 2>/dev/null || stat -f%z "${WASM_PATH}")
WASM_SIZE_KB=$(( WASM_SIZE / 1024 ))

echo "==> WASM size: ${WASM_SIZE} bytes (${WASM_SIZE_KB} KB) / limit: ${SIZE_LIMIT_BYTES} bytes (256 KB)"

if (( WASM_SIZE > SIZE_LIMIT_BYTES )); then
    echo "FAIL: WASM size ${WASM_SIZE} bytes exceeds 256 KB budget (${SIZE_LIMIT_BYTES} bytes)." >&2
    exit 1
fi

echo "PASS: WASM is within the 256 KB size budget."