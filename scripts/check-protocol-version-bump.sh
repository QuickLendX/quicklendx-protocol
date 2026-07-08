#!/usr/bin/env bash
# Enforce that contract package releases ratchet the public protocol version.

set -euo pipefail

MANIFEST_PATH="quicklendx-contracts/Cargo.toml"
INIT_PATH="quicklendx-contracts/src/init.rs"

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

head_ref="${HEAD_REF:-HEAD}"
base_ref="${BASE_REF:-}"

if [[ -z "${base_ref}" ]]; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    base_ref="origin/${GITHUB_BASE_REF}"
  elif git rev-parse --verify --quiet origin/main >/dev/null; then
    base_ref="origin/main"
  elif git rev-parse --verify --quiet HEAD~1 >/dev/null; then
    base_ref="HEAD~1"
  else
    base_ref="HEAD"
  fi
fi

if [[ "${base_ref}" =~ ^0+$ ]]; then
  echo "OK: no previous base commit for this event; protocol version ratchet skipped."
  exit 0
fi

if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    git fetch --no-tags --depth=1 origin "${GITHUB_BASE_REF}" >/dev/null 2>&1 || true
    base_ref="FETCH_HEAD"
  fi
fi

if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "ERROR: unable to resolve base ref '${base_ref}' for protocol version check." >&2
  exit 1
fi

if ! git rev-parse --verify --quiet "${head_ref}^{commit}" >/dev/null; then
  echo "ERROR: unable to resolve head ref '${head_ref}' for protocol version check." >&2
  exit 1
fi

extract_package_version() {
  local ref="$1"
  git show "${ref}:${MANIFEST_PATH}" 2>/dev/null | awk '
    /^\[package\]/ { in_package = 1; next }
    /^\[/ { in_package = 0 }
    in_package && $1 == "version" {
      value = $0
      sub(/^[^=]*=[[:space:]]*"/, "", value)
      sub(/".*$/, "", value)
      print value
      exit
    }
  '
}

extract_protocol_version() {
  local ref="$1"
  git show "${ref}:${INIT_PATH}" 2>/dev/null | awk '
    /^[[:space:]]*pub[[:space:]]+const[[:space:]]+PROTOCOL_VERSION[[:space:]]*:/ {
      value = $0
      sub(/^.*=[[:space:]]*/, "", value)
      sub(/;.*/, "", value)
      gsub(/[[:space:]]/, "", value)
      print value
      exit
    }
  '
}

base_package_version="$(extract_package_version "${base_ref}")"
head_package_version="$(extract_package_version "${head_ref}")"

if [[ -z "${base_package_version}" || -z "${head_package_version}" ]]; then
  echo "ERROR: unable to read [package].version from ${MANIFEST_PATH}." >&2
  exit 1
fi

if [[ "${base_package_version}" == "${head_package_version}" ]]; then
  echo "OK: ${MANIFEST_PATH} package version unchanged (${head_package_version}); no protocol ratchet required."
  exit 0
fi

base_protocol_version="$(extract_protocol_version "${base_ref}")"
head_protocol_version="$(extract_protocol_version "${head_ref}")"

if [[ -z "${base_protocol_version}" || -z "${head_protocol_version}" ]]; then
  echo "ERROR: unable to read PROTOCOL_VERSION from ${INIT_PATH}." >&2
  exit 1
fi

if ! [[ "${base_protocol_version}" =~ ^[0-9]+$ && "${head_protocol_version}" =~ ^[0-9]+$ ]]; then
  echo "ERROR: PROTOCOL_VERSION must be an unsigned integer." >&2
  echo "       base=${base_protocol_version}, head=${head_protocol_version}" >&2
  exit 1
fi

if (( head_protocol_version <= base_protocol_version )); then
  cat >&2 <<MSG
ERROR: ${MANIFEST_PATH} package version changed (${base_package_version} -> ${head_package_version})
       but ${INIT_PATH} PROTOCOL_VERSION did not increase (${base_protocol_version} -> ${head_protocol_version}).

Every contract package release must ratchet PROTOCOL_VERSION so operators and
downstream consumers can detect the release via get_version().
MSG
  exit 1
fi

echo "OK: contract package version changed (${base_package_version} -> ${head_package_version}) and PROTOCOL_VERSION increased (${base_protocol_version} -> ${head_protocol_version})."
