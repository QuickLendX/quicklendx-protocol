#!/usr/bin/env bash
# Regenerate the profit/fee golden-vector corpus from the live implementation.
#
# Usage:
#   ./scripts/refresh-profit-fee-corpus.sh [OPTIONS]
#
# Options:
#   -h, --help    Show this help message and exit
#
# Environment:
#   ALLOW_PROFIT_FEE_CORPUS_REFRESH  Must be set to 1 to regenerate the corpus.
#   CORPUS_PATH                      Optional relative or absolute path to corpus JSON.
#
# Bless workflow: admin-reviewed PR only. Never auto-bless in CI.
# Requires ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 to prevent accidental overwrites.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/refresh-profit-fee-corpus.sh [OPTIONS]

Regenerates the profit/fee golden-vector corpus fixture.

Options:
  -h, --help    Show this help message and exit

Environment:
  ALLOW_PROFIT_FEE_CORPUS_REFRESH  Set to 1 to confirm corpus regeneration (required).
  CORPUS_PATH                      Path to target JSON fixture (optional).
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

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/quicklendx-contracts"

if [[ "${ALLOW_PROFIT_FEE_CORPUS_REFRESH:-}" != "1" ]]; then
  echo "error: set ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 to regenerate the corpus" >&2
  exit 1
fi

export ALLOW_PROFIT_FEE_CORPUS_REFRESH=1
cargo test --test profit_fee_golden refresh_profit_fee_corpus -- --ignored --nocapture

echo "Corpus refreshed at quicklendx-contracts/${CORPUS_PATH:-tests/fixtures/profit_fee_corpus.json}"
echo "Commit the updated JSON in a dedicated PR to bless the semantic change."
