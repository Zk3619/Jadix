param(
    [string]$TargetDir = "assets/twemoji/svg"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../..")
$targetPath = Join-Path $repoRoot $TargetDir
if (-not (Test-Path $targetPath)) {
    throw "Directory not found: $targetPath"
}

$regex = '^[0-9a-f]+(?:-[0-9a-f]+)*\.svg$'
$files = Get-ChildItem -Path $targetPath -File -Filter "*.svg"

$invalid = @()
foreach ($f in $files) {
    if ($f.Name -cnotmatch $regex) {
        $invalid += $f.Name
    }
}

$requiredGroups = @(
    @("1f468-200d-1f469-200d-1f467-200d-1f466.svg"), # family
    @("1f44d-1f3fd.svg"),                             # skin tone
    @("1f1e8-1f1f3.svg"),                             # CN
    @("1f1fa-1f1f8.svg"),                             # US
    @("31-fe0f-20e3.svg", "31-20e3.svg"),            # 1 keycap variants
    @("1f9d1-200d-1f4bb.svg")                        # technologist
)

$missing = @()
foreach ($group in $requiredGroups) {
    $found = $false
    foreach ($candidate in $group) {
        if (Test-Path (Join-Path $targetPath $candidate)) {
            $found = $true
            break
        }
    }
    if (-not $found) {
        $missing += ($group -join " | ")
    }
}

Write-Host "SVG files: $($files.Count)"
Write-Host "Invalid names: $($invalid.Count)"
if ($invalid.Count -gt 0) {
    Write-Host "Invalid filename samples:"
    $invalid | Select-Object -First 20 | ForEach-Object { Write-Host "  $_" }
}

Write-Host "Missing required samples: $($missing.Count)"
if ($missing.Count -gt 0) {
    $missing | ForEach-Object { Write-Host "  $_" }
}

if ($invalid.Count -gt 0 -or $missing.Count -gt 0) {
    exit 1
}

Write-Host "Twemoji SVG naming verification passed."
