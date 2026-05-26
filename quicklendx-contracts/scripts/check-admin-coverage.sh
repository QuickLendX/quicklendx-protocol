#!/usr/bin/env bash
# Enforce minimum line coverage for quicklendx-contracts/src/admin.rs from an LCOV report.
#
# Usage:
#   ./scripts/check-admin-coverage.sh [path/to/lcov.info]
#
# Environment:
#   ADMIN_COVERAGE_MIN  Minimum coverage percentage (default: 95)
#
# Exit codes:
#   0  coverage threshold met
#   1  coverage threshold not met or invalid input
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LCOV_PATH="${1:-$CONTRACTS_DIR/coverage/lcov.info}"
MIN_COVERAGE="${ADMIN_COVERAGE_MIN:-95}"

if [[ ! "$MIN_COVERAGE" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
  echo "::error::ADMIN_COVERAGE_MIN must be numeric, got '$MIN_COVERAGE'"
  exit 1
fi

if [[ ! -f "$LCOV_PATH" ]]; then
  echo "::error::LCOV report not found at '$LCOV_PATH'"
  exit 1
fi

read -r HIT_LINES TOTAL_LINES < <(
  awk '
    /^SF:/ {
      file = substr($0, 4)
      in_admin = (file ~ /(^|[\/\\])src[\/\\]admin\.rs$/)
      next
    }
    in_admin && /^LH:/ { hit += substr($0, 4); next }
    in_admin && /^LF:/ { total += substr($0, 4); next }
    END {
      printf "%d %d\n", hit, total
    }
  ' "$LCOV_PATH"
)

if [[ "$TOTAL_LINES" -eq 0 ]]; then
  echo "::error::No admin.rs coverage entry found in '$LCOV_PATH'"
  exit 1
fi

ADMIN_COVERAGE="$(awk -v hit="$HIT_LINES" -v total="$TOTAL_LINES" 'BEGIN { printf "%.2f", (hit / total) * 100 }')"
MEETS_THRESHOLD="$(awk -v cov="$ADMIN_COVERAGE" -v min="$MIN_COVERAGE" 'BEGIN { print (cov + 0 >= min + 0) ? "yes" : "no" }')"

echo "Admin line coverage: ${ADMIN_COVERAGE}% (${HIT_LINES}/${TOTAL_LINES})"
echo "Required minimum   : ${MIN_COVERAGE}%"

if [[ "$MEETS_THRESHOLD" != "yes" ]]; then
  echo "::error::Admin coverage gate failed: ${ADMIN_COVERAGE}% < ${MIN_COVERAGE}%"
  exit 1
fi

echo "Admin coverage gate passed."
