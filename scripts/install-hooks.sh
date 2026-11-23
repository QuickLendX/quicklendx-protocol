#!/usr/bin/env bash
# Installs a pre-commit hook that blocks accidental secret commits
# and unformatted Rust code.
# Run once after cloning: bash scripts/install-hooks.sh

set -euo pipefail

HOOK=".git/hooks/pre-commit"

cat > "$HOOK" << 'EOF'
#!/usr/bin/env bash
# Pre-commit: reject files that look like they contain real secrets,
# and reject staged Rust files that are not rustfmt-formatted.
# Opt out of the rustfmt check with: SKIP_FMT=1 git commit ...

set -euo pipefail

PATTERNS=(
  'ADMIN_API_KEY\s*=\s*[A-Za-z0-9+/]{20,}'
  'WEBHOOK_SECRET\s*=\s*[A-Za-z0-9+/]{10,}'
  'SECRET_KEY\s*=\s*[A-Za-z0-9+/]{10,}'
  'PASSWORD\s*=\s*[A-Za-z0-9+/]{6,}'
  'PRIVATE_KEY\s*=\s*[A-Za-z0-9+/]{20,}'
)

STAGED=$(git diff --cached --name-only --diff-filter=ACM)
FOUND=0

for file in $STAGED; do
  # Skip .env.example and this script itself
  [[ "$file" == *".env.example"* ]] && continue
  [[ "$file" == *"install-hooks.sh"* ]] && continue
  [[ "$file" == *".test."* ]] && continue

  for pattern in "${PATTERNS[@]}"; do
    if git show ":$file" 2>/dev/null | grep -qE "$pattern"; then
      echo "  [secret-check] Possible secret in: $file (pattern: $pattern)"
      FOUND=1
    fi
  done
done

if [[ $FOUND -ne 0 ]]; then
  echo ""
  echo "Commit blocked: potential secrets detected."
  echo "Use environment variables or a secrets manager instead."
  echo "To bypass (only if you are certain): git commit --no-verify"
  exit 1
fi

# --- rustfmt check -----------------------------------------------------------
# Only runs when staged files include Rust sources. Skipped with SKIP_FMT=1.
if [[ "${SKIP_FMT:-0}" != "1" ]]; then
  HAS_RS=0
  for file in $STAGED; do
    if [[ "$file" == *.rs ]]; then
      HAS_RS=1
      break
    fi
  done

  if [[ $HAS_RS -eq 1 ]]; then
    if ! command -v cargo >/dev/null 2>&1; then
      echo "  [fmt-check] cargo not found; skipping rustfmt check."
      echo "  Install Rust (https://rustup.rs) or set SKIP_FMT=1 to silence this."
    elif ! cargo fmt --all -- --check >/dev/null 2>&1; then
      echo ""
      echo "Commit blocked: Rust files are not formatted with rustfmt."
      echo "Run 'cargo fmt --all' and re-stage your changes."
      echo "To bypass (not recommended): SKIP_FMT=1 git commit ..."
      exit 1
    fi
  fi
fi
EOF

chmod +x "$HOOK"
echo "Pre-commit hook installed at $HOOK"
