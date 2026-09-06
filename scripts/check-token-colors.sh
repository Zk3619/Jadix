#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
pattern='\[\s*0(?:\.\d+)?\s*,\s*0(?:\.\d+)?\s*,\s*0(?:\.\d+)?\s*,\s*0\.\d+\s*\]'
hits=$(rg "$pattern" "$root/crates/rui/src/components" "$root/crates/rui/src/primitives" \
  --glob '*.rs' -n 2>/dev/null | rg -v 'devtools|#\[cfg\(test\)\]|//|TRANSPARENT' || true)
if [[ -n "$hits" ]]; then
  echo "Raw shadow/scrim RGBA in product paths:"
  echo "$hits"
  exit 1
fi
echo "check-token-colors: OK"
