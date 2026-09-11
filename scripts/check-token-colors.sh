#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
allow="$root/scripts/color-audit-allowlist.txt"
patterns=(
  'shadow\(\s*\[\s*0(\.0+)?\s*,\s*0(\.0+)?\s*,\s*0(\.0+)?\s*,'
  'box_shadow_color\(\s*\[\s*0(\.0+)?\s*,\s*0(\.0+)?\s*,\s*0(\.0+)?\s*,'
  '\.bg\(\s*\[\s*0(\.0+)?\s*,\s*0(\.0+)?\s*,\s*0(\.0+)?\s*,\s*0\.[1-9]'
)
dirs=(
  "$root/crates/rui/src/components"
  "$root/crates/rui/src/primitives"
  "$root/crates/rui/src/app/navigator"
)
hits=""
for pat in "${patterns[@]}"; do
  for dir in "${dirs[@]}"; do
    hits+=$(grep -REn --include='*.rs' "$pat" "$dir" 2>/dev/null || true)
    hits+=$'\n'
  done
done
filtered=""
while IFS= read -r line; do
  [[ -z "$line" ]] && continue
  echo "$line" | grep -Eq 'devtools|#\[cfg\(test\)\]|//|TRANSPARENT|overlay_scrim|shadow_rgba|shadow_ink' && continue
  key="${line//\\//}"
  if [[ -f "$allow" ]] && grep -Fqx "$key" "$allow"; then
    continue
  fi
  filtered+="$line"$'\n'
done <<< "$hits"
if [[ -n "${filtered//[$'\n']/}" ]]; then
  echo "Raw shadow/scrim RGBA in product paths:"
  echo "$filtered"
  exit 1
fi
echo "check-token-colors: OK"
