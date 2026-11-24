#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir/.."

output_dir="${MUTANTS_OUTPUT:-target}"
mkdir -p "$(dirname "$output_dir")"

exec cargo mutants \
  --config .cargo/mutants.toml \
  --timeout "${MUTANTS_TIMEOUT:-300}" \
  --build-timeout "${MUTANTS_BUILD_TIMEOUT:-900}" \
  --jobs "${MUTANTS_JOBS:-2}" \
  --output "$output_dir" \
  "$@"
