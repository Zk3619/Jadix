param([switch]$Offline, [string]$FeatureStartAt = "", [string]$StartAt = "")
$ErrorActionPreference = 'Stop'
$shellName = if ($PSVersionTable.PSEdition -eq 'Desktop') { 'powershell.exe' } elseif ($env:OS -eq 'Windows_NT') { 'pwsh.exe' } else { 'pwsh' }
$powerShell = Join-Path $PSHOME $shellName
if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '1' }
if (-not $env:CARGO_INCREMENTAL) { $env:CARGO_INCREMENTAL = '0' }
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$evidence = Join-Path $root ('target/r2-acceptance/' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$results = [Collections.Generic.List[object]]::new()
$started = -not $StartAt
$mode = if ($Offline) { @('--offline') } else { @() }
function Run-Check([string]$name, [string]$program, [string[]]$options) {
    if (-not $script:started) {
        if ($name -ne $StartAt) { return }
        $script:started = $true
        Write-Host "Resuming at $StartAt; prior successful checks require retained results."
    }
    $log = Join-Path $evidence ($name + '.log')
    Write-Host "R2 check: $name"
    $command = Get-Command $program -CommandType Application -ErrorAction Stop
    $previousErrorAction = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & $command.Source @options *> $log
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousErrorAction }
    $results.Add([pscustomobject]@{ name = $name; program = $program; arguments = $options; exit_code = $code; log = $log })
    $results | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $evidence 'results.json')
    if ($code -ne 0) { Get-Content $log -Tail 60; throw "R2 check failed: $name; evidence: $evidence" }
}
Push-Location $root
try {
    Write-Host "R2 evidence: $evidence"
    Run-Check 'runtime-boundaries' $powerShell (@('-NoProfile', '-File', 'scripts/check-runtime-boundaries.ps1') + $(if ($Offline) { '-Offline' }))
    Run-Check 'feature-matrix' $powerShell (@('-NoProfile', '-File', 'scripts/check-features.ps1') + $(if ($Offline) { '-Offline' }) + $(if ($FeatureStartAt) { @('-StartAtFeature', $FeatureStartAt) }))
    Run-Check 'framework-minimal' 'cargo' (@('test', '-p', 'jadix', '--no-default-features', '--lib', '--tests') + $mode + @('--', '--test-threads=1'))
    Run-Check 'framework-enhanced' 'cargo' (@('test', '-p', 'jadix', '--features', 'gallery-tests', '--lib', '--tests') + $mode + @('--', '--test-threads=1'))
    if (-not $StartAt -or $StartAt -eq 'framework-enhanced') {
        $hostLog = Get-Content (Join-Path $evidence 'framework-enhanced.log') -Raw
        $binary = [regex]::Match($hostLog, 'Running unittests src\\lib\.rs \(([^)]+\.exe)\)').Groups[1].Value
        if (-not $binary) { throw 'Enhanced host binary identity is missing' }
        Run-Check 'native-frames' $powerShell @('-NoProfile', '-File', 'scripts/check-r2-native.ps1', '-Executable', $binary)
    }
    Run-Check 'components-builder' 'cargo' (@('test', '-p', 'jadix_components', '--no-default-features', '--tests') + $mode + @('--', '--test-threads=1'))
    Run-Check 'components-macros' 'cargo' (@('test', '-p', 'jadix_components', '--features', 'macros,dnd,themes,animation-path,document-editing,document-diff,document-science,scale-preview,svg,file-persistence', '--tests') + $mode + @('--', '--test-threads=1'))
    Run-Check 'dnd' 'cargo' (@('test', '-p', 'jadix_dnd', '--features', 'macros', '--all-targets') + $mode + @('--', '--test-threads=1'))
    Run-Check 'lucide-default' 'cargo' (@('test', '-p', 'jadix_lucide', '--all-targets') + $mode)
    Run-Check 'lucide-minimal' 'cargo' (@('test', '-p', 'jadix_lucide', '--no-default-features', '--all-targets') + $mode)
    Run-Check 'lucide-animated' 'cargo' (@('test', '-p', 'jadix_lucide', '--features', 'animated,animation-path,macros', '--all-targets') + $mode)
    Run-Check 'runtime-kernels-minimal' 'cargo' (@('test', '-p', 'jadix_renderer', '-p', 'jadix_animation', '--no-default-features') + $mode + @('--', '--test-threads=1'))
    Run-Check 'runtime-kernels' 'cargo' (@('test', '-p', 'jadix_core', '-p', 'jadix_layout', '-p', 'jadix_renderer', '-p', 'jadix_animation', '-p', 'jadix_theme', '-p', 'jadix_window_materials', '--all-features') + $mode + @('--', '--test-threads=1'))
    Run-Check 'kernels-macros-cli' 'cargo' (@('test', '-p', 'jadix_document', '-p', 'jadix_lsp', '-p', 'jadix_macros', '-p', 'jadix_hot_reload', '-p', 'jadix_hot_reload_cli', '--all-features', '--all-targets') + $mode)
    foreach ($script in @('check-product-distributions', 'check-extracted-kernels', 'check-optional-macros', 'check-authoring', 'check-dnd-source')) {
        Run-Check $script $powerShell (@('-NoProfile', '-File', "scripts/$script.ps1") + $(if ($Offline) { '-Offline' }))
    }
    Run-Check 'components-webview-assets' 'cargo' (@('test', '-p', 'jadix_components', '--features', 'macros,document-science,document-editing,webview,file-persistence', '--test', 'document_extensions') + $mode + @('--', '--test-threads=1'))
    Run-Check 'workspace-check' 'cargo' (@('check', '--workspace', '--all-targets') + $mode)
    Run-Check 'gallery-tests' 'cargo' (@('test', '-p', 'gallery') + $mode + @('--', '--test-threads=1'))
    Run-Check 'gallery-build' 'cargo' (@('build', '-p', 'gallery') + $mode)
    if (-not $started) { throw "Unknown restart check: $StartAt" }
    Write-Host "R2 selected automated checks passed: $evidence"
} finally { Pop-Location }
