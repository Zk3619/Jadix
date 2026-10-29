# Fail if product components introduce raw black-alpha shadow / scrim literals.
# Exemptions: tests, comments, TRANSPARENT, allowlist.
$ErrorActionPreference = "Stop"
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$allow = Join-Path $root "scripts\checks\data\color-audit-allowlist.txt"
$allowSet = @{}
if (Test-Path $allow) {
    Get-Content $allow | ForEach-Object {
        $line = $_.Trim()
        if ($line -and -not $line.StartsWith("#")) { $allowSet[$line] = $true }
    }
}
$patterns = @(
    'shadow\(\s*\[\s*0(?:\.0+)?\s*,\s*0(?:\.0+)?\s*,\s*0(?:\.0+)?\s*,',
    'box_shadow_color\(\s*\[\s*0(?:\.0+)?\s*,\s*0(?:\.0+)?\s*,\s*0(?:\.0+)?\s*,',
    '\.bg\(\s*\[\s*0(?:\.0+)?\s*,\s*0(?:\.0+)?\s*,\s*0(?:\.0+)?\s*,\s*0\.[1-9]'
)
$dirs = @(
    (Join-Path $root "crates\jadix\src\components"),
    (Join-Path $root "crates\jadix\src\primitives"),
    (Join-Path $root "crates\jadix\src\app\navigator")
)
$hits = @()
foreach ($pat in $patterns) {
    foreach ($dir in $dirs) {
        $hits += @(
            Get-ChildItem -Path $dir -Recurse -Filter *.rs |
                Select-String -Pattern $pat |
                ForEach-Object { "{0}:{1}:{2}" -f $_.Path, $_.LineNumber, $_.Line.Trim() }
        )
    }
}
$hits = $hits | Where-Object {
    $_ -and
    $_ -notmatch 'devtools|#\[cfg\(test\)\]|//|TRANSPARENT|overlay_scrim|shadow_rgba|shadow_ink' -and
    -not $allowSet.ContainsKey(($_ -replace '\\','/'))
}
if ($hits.Count -gt 0) {
    Write-Error ("Raw shadow/scrim RGBA in product paths:`n" + ($hits -join "`n"))
}
Write-Host "check-token-colors: OK"
