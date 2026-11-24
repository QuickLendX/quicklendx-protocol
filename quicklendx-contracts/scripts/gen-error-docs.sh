#!/usr/bin/env bash
# Generate or check the repository-root QuickLendX contract error catalog.
#
# Usage:
#   quicklendx-contracts/scripts/gen-error-docs.sh [--check]
#
# The script can be invoked from any working directory. It uses only Bash and
# the Python 3 standard library.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

if [[ "$#" -gt 1 || ( "$#" -eq 1 && "$1" != "--check" ) ]]; then
  echo "Usage: $0 [--check]" >&2
  exit 2
fi

CHECK_MODE=0
if [[ "$#" -eq 1 ]]; then
  CHECK_MODE=1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "error: Python 3 is required to generate or check error documentation; install Python 3 and try again." >&2
  exit 1
fi

python3 - "$REPO_ROOT" "$CHECK_MODE" <<'PY'
import difflib
import pathlib
import re
import sys


def fail(message: str) -> "NoReturn":
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


root = pathlib.Path(sys.argv[1])
check_mode = sys.argv[2] == "1"
source_path = root / "quicklendx-contracts/src/errors.rs"
output_path = root / "docs/ERROR_CODES.md"

try:
    source = source_path.read_text(encoding="utf-8")
except OSError as exc:
    fail(f"cannot read {source_path}: {exc}")

enum_match = re.search(r"(?m)^pub\s+enum\s+QuickLendXError\s*\{", source)
if enum_match is None:
    fail(f"could not locate `pub enum QuickLendXError` in {source_path}")

body_start = enum_match.end()
body_end = source.find("\n}", body_start)
if body_end < 0:
    fail("could not locate the end of QuickLendXError")
enum_body = source[body_start:body_end]

variants = []
pending_docs = []
variant_pattern = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(\d+)\s*,\s*(?://.*)?$")
for line_number, line in enumerate(enum_body.splitlines(), start=source[:body_start].count("\n") + 1):
    stripped = line.strip()
    if not stripped:
        continue
    if stripped.startswith("///"):
        pending_docs.append(stripped[3:].lstrip())
        continue
    if stripped.startswith("//"):
        continue
    match = variant_pattern.fullmatch(line)
    if match is None:
        fail(
            f"cannot safely parse QuickLendXError at {source_path}:{line_number}: {stripped!r}; "
            "expected a documented variant with an explicit numeric discriminant"
        )
    name, number_text = match.groups()
    variants.append({"name": name, "code": int(number_text), "docs": pending_docs})
    pending_docs = []

if pending_docs:
    fail("documentation comments at the end of QuickLendXError are not attached to a variant")
if not variants:
    fail("QuickLendXError parsed with no variants; refusing to generate incomplete documentation")
names = [item["name"] for item in variants]
codes = [item["code"] for item in variants]
if len(names) != len(set(names)):
    fail("QuickLendXError contains duplicate variant names")
if len(codes) != len(set(codes)):
    fail("QuickLendXError contains duplicate numeric discriminants")

try:
    symbol_body_start = source.index("impl From<QuickLendXError> for Symbol")
except ValueError:
    fail("could not locate `impl From<QuickLendXError> for Symbol`")
symbol_matches = re.findall(
    r'QuickLendXError::([A-Za-z_][A-Za-z0-9_]*)\s*=>\s*symbol_short!\("([A-Za-z0-9_]+)"\)',
    source[symbol_body_start:],
)
symbols = {}
for name, symbol in symbol_matches:
    if name in symbols:
        fail(f"duplicate ABI symbol mapping for {name}")
    symbols[name] = symbol
if set(symbols) != set(names):
    missing = sorted(set(names) - set(symbols))
    extra = sorted(set(symbols) - set(names))
    fail(
        "could not verify a complete ABI symbol mapping from errors.rs "
        f"(missing: {', '.join(missing) or 'none'}; extra: {', '.join(extra) or 'none'})"
    )

def markdown_cell(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", "<br>")


rows = [
    "| Code | Variant | ABI symbol | Rust documentation |",
    "| ---: | --- | --- | --- |",
]
for item in sorted(variants, key=lambda variant: variant["code"]):
    docs = "<br>".join(markdown_cell(line) for line in item["docs"] if line)
    rows.append(
        f"| {item['code']} | `{item['name']}` | `{symbols[item['name']]}` | {docs} |"
    )

# The FreshnessError table is intentionally maintained separately: this issue
# generates QuickLendXError from errors.rs, while preserving the existing
# freshness catalog and explanatory documentation.
freshness_table = """## FreshnessError (data freshness sub-error)

Defined in `quicklendx-contracts/src/freshness.rs`.

| Code | Variant | Meaning |
| ---: | --- | --- |
| 1 | `NotAuthorized` | The caller lacks authorization for the freshness operation. |
| 2 | `StaleDataRejected` | Off-chain data exceeds the configured freshness drift threshold. |
| 3 | `InvalidConfigValue` | The freshness configuration value is invalid. |
"""

document = f"""# Contract Error Codes

**Audience:** Smart-contract contributors and reviewers.

Every `QuickLendXError` variant and its explicit numeric discriminant, ABI symbol, and Rust documentation are listed below. The source of truth is `quicklendx-contracts/src/errors.rs`. Soroban surfaces these as `ContractError(ErrorContractResult::Err(u32))`; the `u32` is the discriminant.

## Using error codes in tests

```rust
use crate::errors::QuickLendXError;

// Match a returned error
let result = contract.try_create_invoice(&env, &inv);
assert_eq!(result, Err(Ok(QuickLendXError::InvoiceAmountInvalid)));

// Extract the numeric discriminant
assert_eq!(QuickLendXError::InvoiceNotFound as u32, 1000);
```

Off-chain integrators can inspect the `u32` value in transaction result metadata. Numeric values are public ABI; **do not renumber existing variants**.

## QuickLendXError

The table is generated from the enum in numeric order. Variants whose source order differs from their numeric order are still included at their declared code.

{chr(10).join(rows)}

{freshness_table}
## Code example: matching errors in Rust

```rust
use crate::errors::QuickLendXError;

/// Returns the user-facing message for common error codes.
pub fn describe_error_code(code: u32) -> &'static str {{
    match code {{
        1000 => "Invoice not found",
        1100 => "Caller not authorized",
        1401 => "Operation not allowed in current state",
        2100 => "Contract is paused",
        _ => "Unknown error",
    }}
}}
```

## Maintaining this document

Run `quicklendx-contracts/scripts/gen-error-docs.sh` after changing `QuickLendXError`. Use `--check` to verify the committed documentation without modifying files. The generator reads the enum and its ABI symbol mapping directly and fails if it cannot parse every variant and mapping.

Any change to the public error surface requires review. **Never renumber an existing variant** — numeric values are public ABI.
"""

try:
    current = output_path.read_text(encoding="utf-8")
except FileNotFoundError:
    current = None
except OSError as exc:
    fail(f"cannot read {output_path}: {exc}")

if current == document:
    if check_mode:
        print(f"OK: {output_path.relative_to(root)} is up to date")
    else:
        print(f"Up to date: {output_path.relative_to(root)}")
    raise SystemExit(0)

if check_mode:
    print(f"ERROR: generated documentation is out of date: {output_path.relative_to(root)}", file=sys.stderr)
    if current is None:
        print("The documentation file does not exist.", file=sys.stderr)
    else:
        diff = difflib.unified_diff(
            current.splitlines(),
            document.splitlines(),
            fromfile="docs/ERROR_CODES.md (current)",
            tofile="docs/ERROR_CODES.md (generated)",
            lineterm="",
        )
        print("\n".join(diff), file=sys.stderr)
    raise SystemExit(1)

try:
    output_path.write_text(document, encoding="utf-8")
except OSError as exc:
    fail(f"cannot write {output_path}: {exc}")
print(f"Generated {output_path.relative_to(root)} ({len(variants)} error variants)")
PY
