#!/usr/bin/env bash
# Self-test for scripts/check-protocol-version-bump.sh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="${SCRIPT_DIR}/check-protocol-version-bump.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

make_repo() {
  local name="$1"
  local package_version="$2"
  local protocol_version="$3"
  local dir="${TMP_DIR}/${name}"

  mkdir -p "${dir}/quicklendx-contracts/src"
  git -C "${dir}" init --quiet
  git -C "${dir}" config user.email "test@example.invalid"
  git -C "${dir}" config user.name "Protocol Version Gate Test"

  write_contract_files "${dir}" "${package_version}" "${protocol_version}"
  git -C "${dir}" add quicklendx-contracts/Cargo.toml quicklendx-contracts/src/init.rs
  git -C "${dir}" commit --quiet -m "base"

  printf '%s\n' "${dir}"
}

write_contract_files() {
  local dir="$1"
  local package_version="$2"
  local protocol_version="$3"

  cat > "${dir}/quicklendx-contracts/Cargo.toml" <<TOML
[package]
name = "quicklendx-contracts"
version = "${package_version}"
edition = "2021"
TOML

  cat > "${dir}/quicklendx-contracts/src/init.rs" <<RS
pub const PROTOCOL_VERSION: u32 = ${protocol_version};
RS
}

commit_release_change() {
  local dir="$1"
  local package_version="$2"
  local protocol_version="$3"

  write_contract_files "${dir}" "${package_version}" "${protocol_version}"
  git -C "${dir}" add quicklendx-contracts/Cargo.toml quicklendx-contracts/src/init.rs
  git -C "${dir}" commit --allow-empty --quiet -m "release change"
}

expect_success() {
  local dir="$1"
  local label="$2"

  (
    cd "${dir}"
    BASE_REF=HEAD~1 HEAD_REF=HEAD bash "${CHECK_SCRIPT}" >/tmp/protocol-version-success.log 2>&1
  ) || {
    echo "FAIL: expected success for ${label}" >&2
    cat /tmp/protocol-version-success.log >&2
    exit 1
  }
}

expect_failure() {
  local dir="$1"
  local label="$2"

  if (
    cd "${dir}"
    BASE_REF=HEAD~1 HEAD_REF=HEAD bash "${CHECK_SCRIPT}" >/tmp/protocol-version-failure.log 2>&1
  ); then
    echo "FAIL: expected failure for ${label}" >&2
    cat /tmp/protocol-version-failure.log >&2
    exit 1
  fi
}

no_version_change_repo="$(make_repo no-version-change 0.1.0 1)"
commit_release_change "${no_version_change_repo}" 0.1.0 1
expect_success "${no_version_change_repo}" "unchanged package version"

missing_bump_repo="$(make_repo missing-bump 0.1.0 1)"
commit_release_change "${missing_bump_repo}" 0.1.1 1
expect_failure "${missing_bump_repo}" "package version change without protocol bump"

valid_bump_repo="$(make_repo valid-bump 0.1.0 1)"
commit_release_change "${valid_bump_repo}" 0.1.1 2
expect_success "${valid_bump_repo}" "package version change with protocol bump"

decrease_repo="$(make_repo decrease 0.1.0 2)"
commit_release_change "${decrease_repo}" 0.1.1 1
expect_failure "${decrease_repo}" "package version change with protocol decrease"

echo "OK: protocol version bump gate self-test passed."
