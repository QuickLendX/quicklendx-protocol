#!/usr/bin/env bash
# Update Soroban gas regression baselines.
#
# Usage:
#   ./quicklendx-contracts/scripts/update-gas-baseline.sh [OPTIONS]
#
# Options:
#   -h, --help    Show this help message and exit

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./quicklendx-contracts/scripts/update-gas-baseline.sh [OPTIONS]

Runs the gas regression test suite with UPDATE_GAS_BASELINE=1 to update baselines.

Options:
  -h, --help    Show this help message and exit
EOF
}

for arg in "$@"; do
  case "$arg" in
    -h|--help)
      usage
      exit 0
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: 'cargo' is required but not installed or not in PATH" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Updating Soroban gas regression baselines..."
cd "$CONTRACTS_DIR"
UPDATE_GAS_BASELINE=1 cargo test --test gas_regression -- --nocapture
echo "Baselines updated successfully in scripts/gas-baseline.toml!"
