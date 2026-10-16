# Sync the gallery / emoji-matrix Twemoji PNGs into assets/twemoji/72x72.
# Full asset packs stay gitignored; this pulls the minimum set for local visual QA.
param(
    [string]$Version = "16.0.1",
    [string]$Dest = ""
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
if (-not $Dest) {
    $Dest = Join-Path $root "assets\twemoji\72x72"
}
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

$stems = @(
    "1f600", "1f601", "1f602", "1f923", "1f605", "1f972", "1f9e0", "1f9ea",
    "1f680", "1f30d", "1f525", "2705", "274c", "1f4a1", "1f9e9",
    "1f468-200d-1f469-200d-1f467-200d-1f466",
    "1f9d1-200d-1f4bb",
    "1f469-200d-1f4bb",
    "1f3f3-fe0f-200d-1f308",
    "1f469-200d-1f680",
    "1f468-200d-1f52c",
    "1f44d-1f3fd",
    "1f1e8-1f1f3",
    "1f1fa-1f1f8",
    "31-20e3", "32-20e3", "39-20e3",
    "1f44d", "1f64b"
)

$base = "https://cdn.jsdelivr.net/gh/jdecked/twemoji@$Version/assets/72x72"
$ok = 0
$fail = @()
foreach ($stem in $stems) {
    $out = Join-Path $Dest "$stem.png"
    try {
        Invoke-WebRequest -Uri "$base/$stem.png" -OutFile $out -UseBasicParsing -TimeoutSec 60
        if ((Get-Item $out).Length -gt 100) {
            $ok++
        } else {
            Remove-Item $out -Force -ErrorAction SilentlyContinue
            $fail += $stem
        }
    } catch {
        $fail += $stem
    }
}

Write-Host "Synced $ok Twemoji PNGs -> $Dest"
if ($fail.Count -gt 0) {
    Write-Host "Failed: $($fail -join ', ')"
    exit 1
}
Write-Host "Twemoji matrix sync passed."
