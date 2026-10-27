param([switch]$Offline, [switch]$InventoryOnly, [string]$StartAtFeature = "")

$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $inventory = Get-Content (Join-Path $PSScriptRoot '../docs/FEATURES.md') -Raw
    $featureSection = $false
    $featureNames = @()
    foreach ($line in Get-Content (Join-Path $PSScriptRoot '../crates/jadix/Cargo.toml')) {
        if ($line -eq '[features]') { $featureSection = $true; continue }
        if ($featureSection -and $line -match '^\[') { break }
        if ($featureSection -and $line -match '^([a-z0-9-]+)\s*=') {
            $featureNames += $Matches[1]
        }
    }
    foreach ($feature in $featureNames) {
        $row = '(?m)^\| \x60' + [regex]::Escape($feature) + '\x60 \|'
        if ($inventory -notmatch $row) { throw "Feature inventory is missing a row: $feature" }
    }
    foreach ($row in [regex]::Matches($inventory, '(?m)^\| \x60([a-z0-9-]+)\x60 \|')) {
        if ($row.Groups[1].Value -notin $featureNames) {
            throw "Feature inventory contains an unknown feature: $($row.Groups[1].Value)"
        }
    }
    Write-Host "Feature inventory covers $($featureNames.Count) declared features."
    if ($InventoryOnly) { return }
    $cargoOptions = @()
    if ($Offline) { $cargoOptions += '--offline' }
    foreach ($selection in @('', 'theme-shadcn', 'code', 'code-lsp', 'scale-preview', 'backend-dx12', 'windows-dwm')) {
        $treeOptions = @('tree', '-p', 'jadix', '--no-default-features', '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}')
        if ($selection) { $treeOptions += @('--features', $selection) }
        $tree = & cargo @treeOptions @cargoOptions
        if ($LASTEXITCODE -ne 0) { throw 'Dependency tree failed' }
        $unexpected = $tree | Select-String '^(jadix_macros|jadix_clipboard|jadix_lucide|gif|image-webp|zune-jpeg|pulldown-cmark|similar|rfd|latex-rust|resvg|syntect|tokio|moka) v'
        if ($unexpected) { throw "Minimal dependency leak: $unexpected" }
    }
    $selections = @(
        '', 'macros', 'drag', 'product-tests', 'theme-shadcn', 'theme-fluent', 'theme-material3', 'theme-liquid-glass',
        'animation-base', 'animation-ex', 'animation', 'animation-path', 'svg',
        'document', 'document-editing', 'document-diff',
        'code', 'code-lsp', 'markdown', 'scale-preview', 'devtools',
        'windows-dwm', 'window-materials', 'windows-composition', 'windows-dcomp', 'native-materials',
        'document-science', 'document-math', 'syntax-highlighting', 'file-dialog',
        'backend-dx12', 'backend-gles', 'backend-metal', 'backend-vulkan',
        'backend-dx11-native', 'backend-dx11-angle', 'backend-dx11',
        'file-persistence', 'native-clipboard', 'image-jpeg', 'image-gif', 'image-webp', 'image-ico', 'images',
        'runtime-tokio', 'remote-asset', 'remote-asset-cache'
    )
    if ($StartAtFeature) {
        $start = [array]::IndexOf($selections, $StartAtFeature)
        if ($start -lt 0) { throw "Unknown restart feature: $StartAtFeature" }
        Write-Host "Resuming isolated matrix at '$StartAtFeature'; earlier checks require a retained log."
        $selections = $selections[$start..($selections.Count - 1)]
    }
    foreach ($selection in $selections) {
        Write-Host "Checking isolated feature: '$selection'"
        $checkOptions = @('check', '-p', 'jadix', '--no-default-features', '--all-targets')
        if ($selection) { $checkOptions += @('--features', $selection) }
        & cargo @checkOptions @cargoOptions
        if ($LASTEXITCODE -ne 0) { throw "Feature check failed: '$selection'" }
    }
    foreach ($selection in @('', 'paths', 'font', 'ui', 'macros', 'animated', 'svg', 'animation-path', 'animated,svg,macros', 'animated,animation-path,macros')) {
        Write-Host "Checking icon product feature: '$selection'"
        $options = @('check', '-p', 'jadix_lucide', '--no-default-features', '--all-targets')
        if ($selection) { $options += @('--features', $selection) }
        & cargo @options @cargoOptions
        if ($LASTEXITCODE -ne 0) { throw "Icon product feature failed: '$selection'" }
    }
} finally {
    Pop-Location
}
