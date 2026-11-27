#!/usr/bin/env bash
# Fails if a top-level `src/test_*.rs` unit-test file has no `mod` declaration.
#
# Unit tests live in `src/test_*.rs` and are declared in `src/tests/mod.rs`
# (plus `pub mod test_utils;` in `src/lib.rs`). A test file that is never
# declared is silently skipped by `cargo test`, so this script reports such
# orphans by name instead of leaving them hidden.
#
# Usage: bash scripts/check-test-modules.sh
set -euo pipefail

cd "$(dirname "$0")/.."

declaration_files=("src/lib.rs")
if [[ -f src/tests/mod.rs ]]; then
    declaration_files+=("src/tests/mod.rs")
fi

# Module names declared in the crate root or in the tests module.
declared="$(
    grep -hoE '^[[:space:]]*(pub[[:space:]]+)?mod[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*;' \
        "${declaration_files[@]}" \
        | sed -E 's/.*mod[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*;/\1/' \
        | sort -u
)"

shopt -s nullglob
orphans=()
for file in src/test_*.rs; do
    name="$(basename "$file" .rs)"
    if ! grep -qxF "$name" <<<"$declared"; then
        orphans+=("$file")
    fi
done

if (( ${#orphans[@]} > 0 )); then
    echo "error: ${#orphans[@]} test file(s) are not declared in src/tests/mod.rs:" >&2
    for file in "${orphans[@]}"; do
        echo "  - $file (missing \`mod $(basename "$file" .rs);\`)" >&2
    done
    echo "" >&2
    echo "Add a \`#[path = \"../<name>.rs\"] mod <name>;\` declaration to src/tests/mod.rs." >&2
    exit 1
fi

echo "ok: every src/test_*.rs file is declared"
