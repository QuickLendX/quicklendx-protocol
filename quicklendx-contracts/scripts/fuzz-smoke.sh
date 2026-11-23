#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/fuzz-smoke.sh [SECONDS]

Run each libFuzzer target for the requested duration (default: 30 seconds).

Options:
  -h, --help  Show this help message
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

duration="${1:-30}"
if [[ ! "$duration" =~ ^[1-9][0-9]*$ ]]; then
  echo "error: duration must be a positive integer number of seconds" >&2
  usage >&2
  exit 2
fi

cd "$(dirname "$0")/../fuzz"
for target in accept_bid_and_fund settlement paged_queries; do
  echo "==> Fuzzing $target for ${duration}s"
  cargo +nightly fuzz run "$target" -- -max_total_time="$duration"
done
