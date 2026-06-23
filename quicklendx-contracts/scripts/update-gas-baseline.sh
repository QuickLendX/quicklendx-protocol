#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
echo "This helper documents how to refresh gas baselines."
echo ""
echo "Run the measurement harness (requires the bench measurement tests to be implemented):"
echo "  pushd $CONTRACTS_DIR"
echo "  cargo test --test gas_regression -- --nocapture"
echo "  popd"
echo ""
echo "Update scripts/gas-baseline.toml with recorded values and commit the change."
exit 0
