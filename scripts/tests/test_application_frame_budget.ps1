$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ('jadix-budget-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
$gate = Join-Path $PSScriptRoot '../benchmarks/application-frame-budget.ps1'
$fixture = Join-Path $root 'fixture.log'
$report = Join-Path $root 'report.json'
$sample = @{
    window = 1; work_ms = 8.0; redraw_ms = 5.0; event_ms = 2.0; shared_ms = 1.0
    ime_ms = 1.0; semantics_ms = 0.5; input_events = 1; ime_events = 1
}
function Write-Fixture($samples) {
    @($samples | ForEach-Object { 'JADIX_FRAME_BUDGET ' + ($_ | ConvertTo-Json -Compress) }) |
        Set-Content -LiteralPath $fixture -Encoding utf8
}
function Invoke-Gate {
    & $gate -LogPath $fixture -Scenario 'gate-fixture-only-not-performance-evidence' `
        -ExecutablePath (Join-Path $PSHOME 'pwsh.exe') -WarmupFrames 1 -MinSamples 3 `
        -RequireInput -RequireIme -RequireSemantics -OutputPath $report | Out-Null
}
function Expect-Failure($name) {
    $failed = $false
    try { Invoke-Gate } catch { $failed = $true }
    if (!$failed) { throw "Gate incorrectly passed: $name" }
    Write-Output "PASS rejects $name"
}
Write-Fixture @($sample, $sample, $sample, $sample)
Invoke-Gate
Write-Output 'PASS valid samples and warmup exclusion'
$slow = $sample.Clone()
$slow.work_ms = 30.0
$slow.redraw_ms = 27.0
Write-Fixture @($slow, $slow, $slow, $slow)
Expect-Failure 'over-budget frames'
$noIme = $sample.Clone()
$noIme.ime_events = 0
Write-Fixture @($noIme, $noIme, $noIme, $noIme)
Expect-Failure 'missing IME coverage'
$otherWindow = $sample.Clone()
$otherWindow.window = 2
Write-Fixture @($sample, $sample, $sample, $otherWindow)
Expect-Failure 'undersampled secondary window'
$invalid = $sample.Clone()
$invalid.Remove('event_ms')
Write-Fixture @($invalid)
Expect-Failure 'missing stage field'
Set-Content -LiteralPath $fixture -Value 'render-only p95=1ms' -Encoding utf8
Expect-Failure 'render-only logs'
Write-Output "Fixtures: $root"
