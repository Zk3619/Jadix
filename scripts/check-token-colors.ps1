# Fail if product components introduce raw RGBA literals for chrome colors.
# Exemptions: color_space math, tests, devtools, comments.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$pattern = '\[\s*0(?:\.\d+)?\s*,\s*0(?:\.\d+)?\s*,\s*0(?:\.\d+)?\s*,\s*0\.\d+\s*\]'
$hits = @(
    rg $pattern (Join-Path $root "crates\rui\src\components") --glob "*.rs" -n 2>$null
    rg $pattern (Join-Path $root "crates\rui\src\primitives") --glob "*.rs" -n 2>$null
) | Where-Object { $_ -and $_ -notmatch 'devtools|#\[cfg\(test\)\]|//|TRANSPARENT' }

if ($hits.Count -gt 0) {
    Write-Error ("Raw shadow/scrim RGBA in product paths:`n" + ($hits -join "`n"))
}
Write-Host "check-token-colors: OK (no banned raw RGBA in components/primitives)"
