param(
    [switch]$Offline,
    [string]$EvidenceDirectory
)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$shellName = if ($PSVersionTable.PSEdition -eq 'Desktop') { 'powershell.exe' } elseif ($env:OS -eq 'Windows_NT') { 'pwsh.exe' } else { 'pwsh' }
$powerShell = Join-Path $PSHOME $shellName
if (-not $EvidenceDirectory) { $EvidenceDirectory = Join-Path $root ('development-docs/ops/evidence/hr-r3-' + (Get-Date -Format 'yyyyMMdd-HHmmss')) }
$evidence = [IO.Path]::GetFullPath($EvidenceDirectory)
$target = [IO.Path]::GetFullPath((Join-Path $root 'target')) + [IO.Path]::DirectorySeparatorChar
if (($evidence + [IO.Path]::DirectorySeparatorChar).StartsWith($target, [StringComparison]::OrdinalIgnoreCase)) { throw 'Evidence must stay outside target.' }
if (Test-Path -LiteralPath (Join-Path $evidence 'results.json')) { throw 'Choose a new evidence directory; existing results are immutable.' }
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$results = [Collections.Generic.List[object]]::new()
$mode = if ($Offline) { @('--offline') } else { @() }
function Run-Check([string]$name, [string]$program, [string[]]$options, [switch]$RequireTests) {
    $log = Join-Path $evidence ($name + '.log')
    $command = @(Get-Command $program -CommandType Application -ErrorAction Stop)[0]
    Write-Host "HR/R3 check: $name"
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $previous = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        & $command.Source @options *> $log
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previous }
    $watch.Stop()
    if ($RequireTests -and (Get-Content -LiteralPath $log -Raw) -notmatch 'test result: ok\. [1-9][0-9]* passed;') { $code = 1 }
    $results.Add([pscustomobject]@{ name = $name; program = $command.Source; arguments = $options; exit_code = $code; elapsed_ms = $watch.ElapsedMilliseconds; log = $log; log_sha256 = (Get-FileHash -LiteralPath $log).Hash })
    $results | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $evidence 'results.json') -Encoding UTF8
    if ($code -ne 0) { Get-Content -LiteralPath $log -Tail 65; throw "Check failed: $name; evidence: $evidence" }
}
function Host-Binary([string]$name) {
    $text = Get-Content -LiteralPath (Join-Path $evidence ($name + '.log')) -Raw
    $binary = [regex]::Match($text, 'Running unittests src[\\/]lib\.rs \(([^)]+)\)').Groups[1].Value
    if (-not $binary) { throw "Test executable identity missing: $name" }
    $resolved = (Resolve-Path -LiteralPath $binary).Path
    [pscustomobject]@{ name = $name; executable = $resolved; sha256 = (Get-FileHash -LiteralPath $resolved).Hash } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence ($name + '-binary.json')) -Encoding UTF8
    return $resolved
}
Push-Location $root
try {
    Write-Host "HR0-HR5/R3 evidence: $evidence"
    [pscustomobject]@{
        timestamp = (Get-Date -Format o)
        os = [Environment]::OSVersion.VersionString
        architecture = $env:PROCESSOR_ARCHITECTURE
        powershell = $PSVersionTable.PSVersion.ToString()
        edition = $PSVersionTable.PSEdition
        host_executable = $powerShell
        cargo_build_jobs = $env:CARGO_BUILD_JOBS
        cargo_incremental = $env:CARGO_INCREMENTAL
        acceptance_script_sha256 = (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'check-hr-r3-acceptance.ps1')).Hash
        closure_auditor_sha256 = (Get-FileHash -LiteralPath (Join-Path $PSScriptRoot '../release/check-r3-closure.py')).Hash
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $evidence 'environment.json') -Encoding UTF8
    Run-Check 'rustc' 'rustc' @('-Vv')
    Run-Check 'source' 'git' @('rev-parse', 'HEAD')
    Run-Check 'working-tree' 'git' @('status', '--porcelain=v1', '--untracked-files=normal')
    Run-Check 'diff' 'git' @('diff', '--stat', 'HEAD')
    Run-Check 'protocol-macros-cli' 'cargo' (@('test', '-p', 'jadix_hot_reload', '-p', 'jadix_macros', '--all-features', '--locked') + $mode) -RequireTests
    Run-Check 'framework-hot-reload' 'cargo' (@('test', '-p', 'jadix', '--features', 'hot-reload,devtools,gallery-tests', '--lib', 'hot_reload', '--locked') + $mode + @('--', '--test-threads=1')) -RequireTests
    $enhanced = Host-Binary 'framework-hot-reload'
    foreach ($filter in @('app::devtools::', 'keyframes', 'style::animation::tests::', 'app::pending_images::', 'reconcile', 'stylesheet')) {
        $name = 'enhanced-' + ($filter -replace '::', '-')
        Run-Check $name $enhanced @($filter, '--test-threads=1') -RequireTests
    }
    Run-Check 'native-frames' $powerShell @('-NoProfile', '-File', 'scripts/acceptance/check-r2-native.ps1', '-Executable', $enhanced) -RequireTests
    Run-Check 'framework-no-macros' 'cargo' (@('test', '-p', 'jadix', '--no-default-features', '--features', 'devtools,hot-reload', '--lib', 'app::devtools::', '--locked') + $mode + @('--', '--test-threads=1')) -RequireTests
    $plain = Host-Binary 'framework-no-macros'
    Run-Check 'explicit-builder-registration' $plain @('explicit_builder_description', '--test-threads=1') -RequireTests
    foreach ($selection in @('devtools', 'hot-reload')) {
        Run-Check ('framework-feature-' + $selection) 'cargo' (@('check', '-p', 'jadix', '--no-default-features', '--features', $selection, '--locked') + $mode)
    }
    Run-Check 'release-isolation' 'cargo' (@('check', '-p', 'jadix', '--release', '--no-default-features', '--features', 'hot-reload,devtools', '--locked') + $mode)
    foreach ($script in @('check-runtime-boundaries', 'check-window-deps', 'check-optional-macros', 'check-product-distributions')) {
        $options = @('-NoProfile', '-File', "scripts/checks/$script.ps1")
        if ($Offline) { $options += '-Offline' }
        Run-Check $script $powerShell $options
    }
    Run-Check 'construction-baseline' 'cargo' (@('test', '-p', 'jadix', '--test', 'hr_baseline', '--locked') + $mode + @('--', '--ignored', '--exact', 'hot_reload_three_entry_construction_baseline', '--test-threads=1', '--nocapture')) -RequireTests
    Run-Check 'gallery-tests' 'cargo' (@('test', '-p', 'gallery', '--locked') + $mode + @('--', '--test-threads=1')) -RequireTests
    Run-Check 'gallery-build' 'cargo' (@('build', '-p', 'gallery', '--locked') + $mode)
    $audit = @('scripts/release/check-r3-closure.py', '--package-lists', '--output', (Join-Path $evidence 'r3-closure.json'), '--markdown', (Join-Path $evidence 'r3-closure.md'))
    if ($Offline) { $audit += '--offline' }
    Run-Check 'r3-closure' 'python' $audit
    Write-Host "HR0-HR5/R3 automated checks passed: $evidence"
    Write-Host 'User sign-off, HR6 file-to-native-UI/performance comparison and R4/R5 publication gates remain separate.'
} finally { Pop-Location }
