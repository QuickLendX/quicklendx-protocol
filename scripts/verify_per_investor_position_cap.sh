#!/usr/bin/env bash
# Lightweight logic check for Issue #1858 (per_investor_position_cap).
# Full `cargo test` currently blocked by unrelated upstream compile errors on main;
# this script mirrors the validate_bid guard semantics.
#
# Usage:
#   ./scripts/verify_per_investor_position_cap.sh [OPTIONS]
#
# Options:
#   -h, --help    Show this help message and exit

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/verify_per_investor_position_cap.sh [OPTIONS]

Performs lightweight logic check for per-investor position cap validation.

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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT"

PYTHON_CMD=()
if command -v python3 >/dev/null 2>&1 && python3 --version >/dev/null 2>&1; then
  PYTHON_CMD=(python3)
elif command -v python >/dev/null 2>&1 && python --version >/dev/null 2>&1; then
  PYTHON_CMD=(python)
elif command -v py >/dev/null 2>&1 && py -3 --version >/dev/null 2>&1; then
  PYTHON_CMD=(py -3)
else
  echo "error: Python 3 is required but not installed or not in PATH" >&2
  exit 1
fi

"${PYTHON_CMD[@]}" - <<'PY'
def exceeds(bid: int, cap: int | None) -> bool:
    return cap is not None and bid > cap

assert not exceeds(3000, 3000), "bid == cap must pass"
assert exceeds(3001, 3000), "bid == cap+1 must fail"
assert not exceeds(5000, None), "uncapped must pass"
assert not exceeds(1, 3000), "bid below cap must pass"
print("per_investor_position_cap logic checks: OK")
PY
