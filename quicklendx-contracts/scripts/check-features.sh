#!/usr/bin/env bash
# Check the supported contract feature combinations for library and test targets.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${script_dir}/.." || exit 1

log_dir="$(mktemp -d "${TMPDIR:-/tmp}/quicklendx-features.XXXXXX")" || exit 1
trap 'rm -rf "$log_dir"' EXIT

features=("" "testutils" "diagnostics" "testutils,diagnostics" "fuzz-tests")
labels=("none" "testutils" "diagnostics" "testutils,diagnostics" "fuzz-tests")
lib_results=()
test_results=()
failures=0

check() {
  local index="$1"
  local target="$2"
  local log_file="${log_dir}/${index}-${target}.log"
  local args=(--lib)

  if [[ "$target" == tests ]]; then
    args+=(--tests)
  fi
  if [[ -n "${features[$index]}" ]]; then
    args+=(--features "${features[$index]}")
  fi

  printf 'Checking %-22s %s\n' "${labels[$index]}" "cargo check ${args[*]}"
  if cargo check "${args[@]}" >"$log_file" 2>&1; then
    printf -v "$3" '%s' 'PASS'
  else
    printf -v "$3" '%s' 'FAIL'
    failures=$((failures + 1))
    printf '\nFailure: cargo check %s\n' "${args[*]}"
    # Cargo often ends with warnings; show the first errors instead.
    grep -m 12 -E '^error(\[|:)' "$log_file" || tail -n 20 "$log_file"
    tail -n 3 "$log_file"
    printf '\n'
  fi
}

for index in "${!features[@]}"; do
  check "$index" lib result
  lib_results+=("$result")
  check "$index" tests result
  test_results+=("$result")
done

printf '\n%-23s | %-5s | %s\n' 'Features' 'lib' 'lib + tests'
printf '%s\n' '---------------------------------------------'
for index in "${!features[@]}"; do
  printf '%-23s | %-5s | %-5s\n' "${labels[$index]}" "${lib_results[$index]}" "${test_results[$index]}"
done

if (( failures > 0 )); then
  printf '\n%d feature check(s) failed.\n' "$failures"
  exit 1
fi

printf '\nAll feature checks passed.\n'
