#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Updating Soroban gas regression baselines..."
cd "$CONTRACTS_DIR"
UPDATE_GAS_BASELINE=1 cargo test --test gas_regression -- --nocapture
echo "Baselines updated successfully in scripts/gas-baseline.toml!"
