#!/usr/bin/env bash
# check-entrypoint-docs.sh – Verify that README.md entrypoint table matches
# exported functions in quicklendx-contracts/src/lib.rs.
#
# Usage:
#   ./scripts/check-entrypoint-docs.sh [OPTIONS]
#   ./scripts/check-entrypoint-docs.sh [--lib-path <path>] [--readme-path <path>]
#
# Options:
#   -h, --help           Show this help message and exit
#   --lib-path <path>    Path to src/lib.rs (default: <contracts_dir>/src/lib.rs)
#   --readme-path <path> Path to README.md (default: <contracts_dir>/README.md)
#
# Exit codes:
#   0  Docs match code (no entrypoint drift)
#   1  Mismatches found between README.md and src/lib.rs or invalid arguments
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTRACTS_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

LIB_PATH="$CONTRACTS_DIR/src/lib.rs"
README_PATH="$CONTRACTS_DIR/README.md"

show_help() {
  cat << 'EOF'
Usage: ./scripts/check-entrypoint-docs.sh [OPTIONS]

Diffs the README entrypoint tables against the contract's exported functions
in src/lib.rs without compiling the contract.

Options:
  -h, --help             Show this help message and exit
  --lib-path <path>      Path to src/lib.rs (default: quicklendx-contracts/src/lib.rs)
  --readme-path <path>   Path to README.md (default: quicklendx-contracts/README.md)

Exit codes:
  0  All contract entrypoints in src/lib.rs match the README.md table
  1  Mismatches found (missing or extra entrypoints), or file not found
EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      show_help
      exit 0
      ;;
    --lib-path)
      if [[ -z "${2:-}" ]]; then
        echo "::error::Missing value for --lib-path" >&2
        exit 1
      fi
      LIB_PATH="$2"
      shift 2
      ;;
    --readme-path)
      if [[ -z "${2:-}" ]]; then
        echo "::error::Missing value for --readme-path" >&2
        exit 1
      fi
      README_PATH="$2"
      shift 2
      ;;
    *)
      # Allow positional arguments: $1 as lib_path, $2 as readme_path
      if [[ -z "${LIB_PATH_SET:-}" ]]; then
        LIB_PATH="$1"
        LIB_PATH_SET=1
        shift
      elif [[ -z "${README_PATH_SET:-}" ]]; then
        README_PATH="$1"
        README_PATH_SET=1
        shift
      else
        echo "::error::Unknown argument: $1" >&2
        show_help
        exit 1
      fi
      ;;
  esac
done

if [[ ! -f "$LIB_PATH" ]]; then
  echo "::error::Contract source file not found at '$LIB_PATH'" >&2
  exit 1
fi

if [[ ! -f "$README_PATH" ]]; then
  echo "::error::Documentation file not found at '$README_PATH'" >&2
  exit 1
fi

PYTHON_BIN=""
if command -v python3 >/dev/null 2>&1; then
  PYTHON_BIN="python3"
elif command -v python >/dev/null 2>&1; then
  PYTHON_BIN="python"
else
  echo "::error::python3 or python is required to run the entrypoint doc check" >&2
  exit 1
fi

exec "$PYTHON_BIN" -I -c '
import os
import re
import sys

lib_path = sys.argv[1]
readme_path = sys.argv[2]

# 1. Extract exported functions from #[contractimpl] block in lib.rs
try:
    with open(lib_path, "r", encoding="utf-8") as f:
        lib_content = f.read()
except Exception as e:
    print(f"::error::Failed to read {lib_path}: {e}", file=sys.stderr)
    sys.exit(1)

code_fns = set()
in_contractimpl = False
depth = 0

for line in lib_content.splitlines():
    if "#[contractimpl]" in line:
        in_contractimpl = True
        continue
    if in_contractimpl:
        # Match pub fn <identifier>
        m = re.match(r"^\s*pub\s+fn\s+([a-zA-Z0-9_]+)", line)
        if m:
            code_fns.add(m.group(1))
        for ch in line:
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    in_contractimpl = False
                    break

# 2. Extract backticked entrypoint names from README table
try:
    with open(readme_path, "r", encoding="utf-8") as f:
        readme_content = f.read()
except Exception as e:
    print(f"::error::Failed to read {readme_path}: {e}", file=sys.stderr)
    sys.exit(1)

readme_fns = set()
in_section = False
in_table = False

for line in readme_content.splitlines():
    # Detect entrypoint section header
    if re.search(r"#+\s*Contract entrypoints", line, re.IGNORECASE):
        in_section = True
        continue
    if in_section:
        # Stop if a new heading starts after having entered the table
        if in_table and line.startswith("#"):
            break
        # Table header
        if line.strip().startswith("|") and "Entrypoint" in line:
            in_table = True
            continue
        if in_table:
            # End of table
            if not line.strip().startswith("|"):
                if line.strip() != "":
                    break
                continue
            # Skip separator line like |---|---|
            if re.match(r"^\s*\|(?:\s*:?-+:?\s*\|)+\s*$", line):
                continue
            # Extract first column entrypoint name inside backticks
            cols = [col.strip() for col in line.split("|")]
            # cols[0] is before first pipe (empty), cols[1] is first column
            if len(cols) > 1 and cols[1]:
                m = re.search(r"`([a-zA-Z0-9_]+)", cols[1])
                if m:
                    readme_fns.add(m.group(1))

# 3. Compare sets
missing_in_readme = sorted(code_fns - readme_fns)
extra_in_readme = sorted(readme_fns - code_fns)

mismatches = 0
for fn in missing_in_readme:
    print(f"MISSING: {fn} (in {os.path.basename(lib_path)} #[contractimpl], missing from {os.path.basename(readme_path)})")
    mismatches += 1

for fn in extra_in_readme:
    print(f"EXTRA: {fn} (in {os.path.basename(readme_path)}, not in {os.path.basename(lib_path)} #[contractimpl])")
    mismatches += 1

if mismatches > 0:
    print(f"\nEntrypoint check failed: {mismatches} mismatch(es) found ({len(missing_in_readme)} missing, {len(extra_in_readme)} extra).", file=sys.stderr)
    sys.exit(1)

print(f"OK: All {len(code_fns)} exported contract entrypoints match {os.path.basename(readme_path)}.")
sys.exit(0)
' "$LIB_PATH" "$README_PATH"
