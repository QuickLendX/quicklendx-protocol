#!/usr/bin/env bash
set -euo pipefail

EXPECTED_STELLAR_VERSION="23.0.0"
REQUIRED_FAILURES=0

required_ok() {
  printf '[required] %-18s %s\n' "$1" "$2"
}

required_fail() {
  printf '[required] %-18s MISSING — %s\n' "$1" "$2"
  REQUIRED_FAILURES=1
}

optional_ok() {
  printf '[optional] %-18s %s\n' "$1" "$2"
}

optional_missing() {
  printf '[optional] %-18s MISSING — %s\n' "$1" "$2"
}

if command -v rustc >/dev/null 2>&1; then
  rustc_version="$(rustc --version)"
  required_ok rustc "$rustc_version"
else
  required_fail rustc "Install Rust with https://rustup.rs/"
fi

if command -v cargo >/dev/null 2>&1; then
  required_ok cargo "$(cargo --version)"
else
  required_fail cargo "Install Rust with https://rustup.rs/"
fi

if command -v rustup >/dev/null 2>&1; then
  if rustup target list --installed | grep -Fxq wasm32v1-none; then
    required_ok wasm32v1-none "WASM target installed"
  else
    required_fail wasm32v1-none "Run: rustup target add wasm32v1-none"
  fi
else
  required_fail rustup "Install Rust with rustup: https://rustup.rs/"
fi

if command -v stellar >/dev/null 2>&1; then
  if stellar_version_output="$(stellar --version 2>&1)"; then
    stellar_version="$(printf '%s\n' "$stellar_version_output" | sed -n '1s/^stellar \([^ ]*\).*/\1/p')"
  else
    stellar_version=""
  fi
  if [[ "$stellar_version" == "$EXPECTED_STELLAR_VERSION" ]]; then
    required_ok stellar "$stellar_version_output"
  else
    required_fail stellar "Found ${stellar_version:-unknown}; install the pinned version: cargo install --locked stellar-cli@${EXPECTED_STELLAR_VERSION}"
  fi
else
  required_fail stellar "Install the pinned version: cargo install --locked stellar-cli@${EXPECTED_STELLAR_VERSION}"
fi

if command -v cargo >/dev/null 2>&1 && cargo_llvm_cov_output="$(cargo llvm-cov --version 2>&1)"; then
  optional_ok cargo-llvm-cov "$cargo_llvm_cov_output"
else
  optional_missing cargo-llvm-cov "Install with: cargo install cargo-llvm-cov"
fi

if command -v wasm-opt >/dev/null 2>&1; then
  if wasm_opt_output="$(wasm-opt --version 2>&1)"; then
    optional_ok wasm-opt "$wasm_opt_output"
  else
    optional_missing wasm-opt "Installed but --version failed; reinstall Binaryen if the size check needs it."
  fi
else
  optional_missing wasm-opt "Install Binaryen (for example: brew install binaryen); WASM size checks can use the fallback budget."
fi

if [[ "$REQUIRED_FAILURES" -ne 0 ]]; then
  printf 'One or more required contributor tools are missing or have the wrong version.\n' >&2
  exit 1
fi

printf 'Required contributor tools are ready.\n'
