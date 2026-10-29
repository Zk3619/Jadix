param([Parameter(Mandatory = $true)][string]$Executable)
$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $Executable).Path
$listing = & $binary --list
if ($LASTEXITCODE -ne 0) { throw 'Enhanced host test inventory failed' }
foreach ($name in @(
    'checkbox_checked_native_frames_match_full_repaint',
    'translated_scaled_native_frames_match_full_repaint',
    'theme_switch_updates_primary_secondary_and_retained_pixels'
)) {
    $matches = @($listing | Where-Object { $_ -match ('::' + [regex]::Escape($name) + ': test$') })
    if ($matches.Count -ne 1) { throw "Native test is missing or ambiguous: $name" }
    $identity = $matches[0] -replace ': test$', ''
    Write-Host "Exact native contract: $identity"
    $previousErrorAction = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & $binary $identity --ignored --exact --test-threads=1 --nocapture 2>&1
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousErrorAction }
    $output | ForEach-Object { Write-Output $_ }
    if ($code -ne 0 -or ($output -join "`n") -notmatch 'test result: ok\. 1 passed;') {
        throw "Native contract did not execute exactly once: $identity"
    }
}
Write-Host "Native test executable SHA-256: $((Get-FileHash -LiteralPath $binary).Hash)"
