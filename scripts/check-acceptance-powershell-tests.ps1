$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'This compatibility check requires Windows PowerShell 5.1.' }
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$evidence = Join-Path $root ('target/powershell-compat/' + [guid]::NewGuid().ToString('N'))
$fixtures = Join-Path $evidence 'command fixtures'
New-Item -ItemType Directory -Force -Path $fixtures | Out-Null
$shells = @((Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'))
if ($PSVersionTable.PSEdition -eq 'Core') { $shells += Join-Path $PSHOME 'pwsh.exe' }
$previousPath = $env:Path
$previousBuildExit = $env:JADIX_COMPAT_BUILD_EXIT
$previousNativeExit = $env:JADIX_COMPAT_NATIVE_EXIT
function Invoke-Acceptance([string]$Shell, [string]$Label, [string[]]$Options) {
    $previousErrorAction = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & $Shell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'scripts/check-r2-acceptance.ps1') -Offline @Options 2>&1
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousErrorAction }
    $text = $output -join "`n"
    $text | Set-Content -LiteralPath (Join-Path $evidence ($Label + '.log')) -Encoding UTF8
    $resultDirectory = [regex]::Match($text, 'R2 evidence: ([^\r\n]+)').Groups[1].Value.Trim()
    if (-not $resultDirectory) { throw "Missing runner evidence: $Label" }
    $results = Get-Content -LiteralPath (Join-Path $resultDirectory 'results.json') -Raw | ConvertFrom-Json
    $results = @($results)
    $caseDirectory = Join-Path $evidence $Label
    New-Item -ItemType Directory -Force -Path $caseDirectory | Out-Null
    Copy-Item -LiteralPath (Join-Path $resultDirectory 'results.json') -Destination $caseDirectory
    foreach ($result in $results) { Copy-Item -LiteralPath $result.log -Destination $caseDirectory }
    [pscustomobject]@{ ExitCode = $code; Results = $results; Directory = $caseDirectory }
}
Push-Location $root
try {
    $metadata = & cargo metadata --offline --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Offline workspace metadata failed.' }
    $metadata | Set-Content -LiteralPath (Join-Path $fixtures 'metadata.json') -Encoding UTF8
    @(
        '@echo off',
        'if "%~1"=="metadata" (',
        '  type "%~dp0metadata.json"',
        '  exit /b 0',
        ')',
        'if "%~1"=="tree" exit /b 0',
        'if "%~1"=="check" exit /b 0',
        'if "%~1"=="clean" exit /b 0',
        'echo compatibility fixture stdout',
        'echo compatibility fixture stderr 1>&2',
        'if "%~1"=="test" exit /b 23',
        'if "%~1"=="build" exit /b %JADIX_COMPAT_BUILD_EXIT%',
        'exit /b 97'
    ) | Set-Content -LiteralPath (Join-Path $fixtures 'cargo.cmd') -Encoding ASCII
    @(
        '@echo off',
        'if "%~1"=="--list" (',
        '  echo fixture::checkbox_checked_native_frames_match_full_repaint: test',
        '  echo fixture::translated_scaled_native_frames_match_full_repaint: test',
        '  echo fixture::theme_switch_updates_primary_secondary_and_retained_pixels: test',
        '  exit /b 0',
        ')',
        'echo test result: ok. 1 passed; 0 failed;',
        'echo native fixture stderr 1>&2',
        'exit /b %JADIX_COMPAT_NATIVE_EXIT%'
    ) | Set-Content -LiteralPath (Join-Path $fixtures 'native-tests.cmd') -Encoding ASCII
    $env:Path = $fixtures + ';' + (Join-Path $env:SystemRoot 'System32') + ';' + $env:SystemRoot
    if (Get-Command pwsh -ErrorAction SilentlyContinue) { throw 'Fixture unexpectedly has pwsh on PATH.' }
    foreach ($shell in $shells) {
        $label = [IO.Path]::GetFileNameWithoutExtension($shell)
        $run = Invoke-Acceptance $shell ($label + '-children') @()
        if ($run.ExitCode -eq 0 -or $run.Results.Count -ne 3 -or
            $run.Results[0].name -ne 'runtime-boundaries' -or $run.Results[0].exit_code -ne 0 -or
            $run.Results[1].name -ne 'feature-matrix' -or $run.Results[1].exit_code -ne 0 -or
            $run.Results[2].name -ne 'framework-minimal' -or $run.Results[2].exit_code -ne 23 -or
            $run.Results[0].program -ne $shell) { throw "Child-shell or exit-code regression: $label" }
        $env:JADIX_COMPAT_BUILD_EXIT = '0'
        $run = Invoke-Acceptance $shell ($label + '-success') @('-StartAt', 'gallery-build')
        $log = Get-Content -LiteralPath (Join-Path $run.Directory 'gallery-build.log') -Raw
        if ($run.ExitCode -ne 0 -or $run.Results.Count -ne 1 -or $run.Results[0].exit_code -ne 0 -or
            $log -notmatch 'compatibility fixture stdout' -or $log -notmatch 'compatibility fixture stderr') {
            throw "Successful native command or stderr logging regression: $label"
        }
        $env:JADIX_COMPAT_BUILD_EXIT = '31'
        $run = Invoke-Acceptance $shell ($label + '-failure') @('-StartAt', 'gallery-build')
        if ($run.ExitCode -eq 0 -or $run.Results.Count -ne 1 -or $run.Results[0].exit_code -ne 31) {
            throw "Failing native command was not preserved: $label"
        }
        foreach ($nativeExit in @('0', '47')) {
            $env:JADIX_COMPAT_NATIVE_EXIT = $nativeExit
            $previousErrorAction = $ErrorActionPreference
            try {
                $ErrorActionPreference = 'Continue'
                $output = & $shell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $root 'scripts/check-r2-native.ps1') -Executable (Join-Path $fixtures 'native-tests.cmd') 2>&1
                $code = $LASTEXITCODE
            } finally { $ErrorActionPreference = $previousErrorAction }
            $text = $output -join "`n"
            $text | Set-Content -LiteralPath (Join-Path $evidence ($label + '-native-' + $nativeExit + '.log')) -Encoding UTF8
            $contracts = [regex]::Matches($text, 'Exact native contract:').Count
            if (($nativeExit -eq '0' -and ($code -ne 0 -or $contracts -ne 3)) -or
                ($nativeExit -ne '0' -and ($code -eq 0 -or $contracts -ne 1)) -or
                $text -notmatch 'native fixture stderr') { throw "Native-contract stderr or failure regression: $label/$nativeExit" }
        }
        Write-Host "PASS: $label child scripts, native contracts, stderr logging and nonzero exits without pwsh on PATH."
    }
    Write-Host "Command fixtures only; no Rust build or acceptance result is claimed. Evidence: $evidence"
} finally {
    $env:Path = $previousPath
    $env:JADIX_COMPAT_BUILD_EXIT = $previousBuildExit
    $env:JADIX_COMPAT_NATIVE_EXIT = $previousNativeExit
    Pop-Location
}
