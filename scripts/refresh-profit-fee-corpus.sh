#!/usr/bin/env bash
# Regenerate the profit/fee golden-vector corpus from the live implementation.
#
# Bless workflow: admin-reviewed PR only. Never auto-bless in CI.
# Requires ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 to prevent accidental overwrites.

set -euo pipefail

if [[ "${ALLOW_PROFIT_FEE_CORPUS_REFRESH:-}" != "1" ]]; then
  echo "error: set ALLOW_PROFIT_FEE_CORPUS_REFRESH=1 to regenerate the corpus" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/quicklendx-contracts"

export ALLOW_PROFIT_FEE_CORPUS_REFRESH=1
cargo test --test profit_fee_golden refresh_profit_fee_corpus -- --ignored --nocapture

echo "Corpus refreshed at quicklendx-contracts/${CORPUS_PATH:-tests/fixtures/profit_fee_corpus.json}"
echo "Commit the updated JSON in a dedicated PR to bless the semantic change."
