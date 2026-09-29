#!/usr/bin/env bash
# Generate a coverage drift report Comparing PR coverage vs Main.
#
# Usage:
#   ./scripts/coverage-drift.sh [baseline.json] [pr.json]
#
# Environment:
#   PR_COMMENT_FILE  If set, write the markdown table to this file.
#
set -euo pipefail

BASELINE_JSON="${1:-}"
PR_JSON="${2:-}"

if [[ -z "$PR_JSON" ]]; then
  echo "Usage: $0 [baseline.json] [pr.json]"
  exit 1
fi

# Function to extract module coverage from JSON
extract_coverage() {
  local json_file="$1"
  if [[ ! -f "$json_file" ]]; then
    # Return empty if file doesn't exist
    return
  fi
  # We use jq to extract filename and line coverage percentage
  # filename is usually src/module.rs or quicklendx-contracts/src/module.rs
  # We will strip the prefix to get the module name
  jq -r '.data[0].files[] | "\(.filename) \(.summary.lines.percent)"' "$json_file" | sort
}

declare -A BASELINE_MAP
if [[ -n "$BASELINE_JSON" && -f "$BASELINE_JSON" ]]; then
  while read -r mod percent; do
    BASELINE_MAP["$mod"]="$percent"
  done < <(extract_coverage "$BASELINE_JSON")
fi

declare -A PR_MAP
while read -r mod percent; do
  PR_MAP["$mod"]="$percent"
done < <(extract_coverage "$PR_JSON")

# Build the set of all modules mentioned in either report
ALL_MODULES=$( (printf "%s\n" "${!BASELINE_MAP[@]}"; printf "%s\n" "${!PR_MAP[@]}") | sort -u )

# Header for the markdown table
TABLE="| Module | Main % | PR % | Delta |"
TABLE+=$'\n'
TABLE+="| :--- | :---: | :---: | :---: |"

HAS_REGRESSIONS=false

for mod in $ALL_MODULES; do
  # Skip non-source files if any
  if [[ ! "$mod" =~ \.rs$ ]]; then continue; fi
  
  # Clean up module name for display (strip src/ and .rs)
  display_name=$(echo "$mod" | sed -E 's|.*src/||; s|\.rs$||')
  
  main_cov="${BASELINE_MAP[$mod]:-0}"
  pr_cov="${PR_MAP[$mod]:-0.00}"
  
  # Handle "0" as 0.00
  if [[ "$main_cov" == "0" ]]; then main_cov="0.00"; fi
  if [[ "$pr_cov" == "0" ]]; then pr_cov="0.00"; fi

  # Calculate delta using awk for float math
  delta=$(awk -v pr="$pr_cov" -v main="$main_cov" 'BEGIN { printf "%.2f", pr - main }')
  
  # Formatting
  delta_display="$delta%"
  if (( $(awk -v d="$delta" 'BEGIN { print (d > 0) }') )); then
    delta_display="+${delta_display}"
  fi
  
  # Alert on regressions in security-critical modules
  warning=""
  if [[ "$mod" =~ (escrow|settlement|reentrancy) ]] && (( $(awk -v d="$delta" 'BEGIN { print (d < 0) }') )); then
    warning=" ⚠️"
    HAS_REGRESSIONS=true
  fi

  TABLE+=$'\n'
  TABLE+="| $display_name | $main_cov% | $pr_cov% | $delta_display$warning |"
done

# Add global gate note
TABLE+=$'\n\n'
TABLE+="> [!NOTE]"
TABLE+=$'\n'
TABLE+="> This report is informational for per-module coverage drift. The 95% global coverage gate remains the hard requirement for PR approval."

if [[ -n "${PR_COMMENT_FILE:-}" ]]; then
  echo "$TABLE" > "$PR_COMMENT_FILE"
fi

echo "$TABLE"

if [[ "$HAS_REGRESSIONS" == "true" ]]; then
  echo ""
  echo "Warning: Coverage regressions detected in security-critical modules."
fi
