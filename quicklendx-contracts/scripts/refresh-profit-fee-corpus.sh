#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: refresh-profit-fee-corpus.sh [--help]

With ALLOW_PROFIT_FEE_CORPUS_REFRESH=1, regenerate
tests/fixtures/profit_fee_corpus.json from the live profit and fee APIs.
Without the allow flag, print the blessing requirement and exit successfully.

Example:
  ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 ./scripts/refresh-profit-fee-corpus.sh
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if [[ "${1:-}" != "" ]]; then
  usage >&2
  exit 2
fi

cd "$(dirname "$0")/.."

if [[ "${ALLOW_PROFIT_FEE_CORPUS_REFRESH:-}" != "1" ]]; then
  echo "Corpus refresh blocked. Set ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 to regenerate." >&2
  exit 1
fi

cargo test --test profit_fee_golden refresh_profit_fee_corpus -- --ignored
