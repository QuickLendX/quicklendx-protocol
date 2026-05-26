#!/usr/bin/env bash
# Installs a pre-commit hook that blocks accidental secret commits.
# Run once after cloning: bash scripts/install-hooks.sh

set -euo pipefail

HOOK=".git/hooks/pre-commit"

cat > "$HOOK" << 'EOF'
#!/usr/bin/env bash
# Pre-commit: reject files that look like they contain real secrets.

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
EOF

chmod +x "$HOOK"
echo "Pre-commit hook installed at $HOOK"
