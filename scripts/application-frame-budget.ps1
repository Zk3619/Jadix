param(
    [Parameter(Mandatory)][string]$LogPath,
    [Parameter(Mandatory)][string]$Scenario,
    [Parameter(Mandatory)][string]$ExecutablePath,
    [double]$BudgetMs = 16.667,
    [ValidateRange(0, 10000)][int]$WarmupFrames = 30,
    [ValidateRange(1, 1000000)][int]$MinSamples = 120,
    [ValidateRange(0, 100)][double]$MaxMissPercent = 1,
    [switch]$RequireInput,
    [switch]$RequireIme,
    [switch]$RequireSemantics,
    [string]$OutputPath = '.tmp/application-frame-budget.json'
)

$ErrorActionPreference = 'Stop'
if ([double]::IsNaN($BudgetMs) -or [double]::IsInfinity($BudgetMs) -or $BudgetMs -le 0) {
    throw 'BudgetMs must be finite and positive'
}
$records = @(foreach ($line in [IO.File]::ReadAllLines((Resolve-Path -LiteralPath $LogPath).Path)) {
    if ($line.StartsWith('JADIX_FRAME_BUDGET ')) {
        $record = $line.Substring('JADIX_FRAME_BUDGET '.Length) | ConvertFrom-Json
        foreach ($field in @('work_ms', 'redraw_ms', 'event_ms', 'shared_ms', 'ime_ms', 'semantics_ms')) {
            $property = $record.PSObject.Properties[$field]
            if ($null -eq $property -or $null -eq $property.Value) { throw "Missing $field" }
            $value = [double]$property.Value
            if ([double]::IsNaN($value) -or [double]::IsInfinity($value) -or $value -lt 0) {
                throw "Invalid $field"
            }
        }
        if ($null -eq $record.window -or $null -eq $record.input_events -or $null -eq $record.ime_events) {
            throw 'Missing window/input coverage'
        }
        if ([math]::Abs($record.work_ms - $record.redraw_ms - $record.event_ms - $record.shared_ms) -gt 0.001) {
            throw 'Frame work components do not add up'
        }
        $record
    }
})
if ($records.Count -eq 0) { throw 'No application frame samples; render-only logs are not accepted' }
$reports = @(foreach ($window in ($records | Group-Object window)) {
    $samples = @($window.Group | Select-Object -Skip $WarmupFrames)
    $sorted = @($samples.work_ms | Sort-Object)
    $count = $samples.Count
    $missed = @($samples | Where-Object { $_.work_ms -gt $BudgetMs }).Count
    $inputEvents = [long](($samples | Measure-Object input_events -Sum).Sum)
    $imeEvents = [long](($samples | Measure-Object ime_events -Sum).Sum)
    $semanticsFrames = @($samples | Where-Object { $_.semantics_ms -gt 0 }).Count
    $p50 = if ($count) { $sorted[[math]::Ceiling($count * 0.50) - 1] } else { 0 }
    $p95 = if ($count) { $sorted[[math]::Ceiling($count * 0.95) - 1] } else { 0 }
    $p99 = if ($count) { $sorted[[math]::Ceiling($count * 0.99) - 1] } else { 0 }
    $missPercent = if ($count) { 100.0 * $missed / $count } else { 100.0 }
    [ordered]@{
        window = $window.Name; samples = $count; warmup_frames = $WarmupFrames
        budget_ms = $BudgetMs; p50_ms = $p50; p95_ms = $p95; p99_ms = $p99
        over_budget = $missed; miss_percent = $missPercent
        input_events = $inputEvents; ime_events = $imeEvents; semantics_frames = $semanticsFrames
        passed = ($count -ge $MinSamples -and $p95 -le $BudgetMs -and $p99 -le 2 * $BudgetMs `
            -and $missPercent -le $MaxMissPercent -and (!$RequireInput -or $inputEvents -gt 0) `
            -and (!$RequireIme -or $imeEvents -gt 0) -and (!$RequireSemantics -or $semanticsFrames -gt 0))
    }
})
$output = [IO.Path]::GetFullPath($OutputPath)
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($output)) | Out-Null
[ordered]@{
    scenario = $Scenario; captured_at = (Get-Date -Format o)
    log_sha256 = (Get-FileHash -LiteralPath $LogPath -Algorithm SHA256).Hash
    executable_sha256 = (Get-FileHash -LiteralPath $ExecutablePath -Algorithm SHA256).Hash
    executable = (Resolve-Path -LiteralPath $ExecutablePath).Path
    gpu = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion)
    cpu = (Get-CimInstance Win32_Processor | Select-Object -ExpandProperty Name)
    os = [Environment]::OSVersion.VersionString
    max_miss_percent = $MaxMissPercent
    requirements = @{ input = [bool]$RequireInput; ime = [bool]$RequireIme; semantics = [bool]$RequireSemantics }
    windows = $reports
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $output -Encoding utf8
$reports | Format-Table window, samples, p50_ms, p95_ms, p99_ms, miss_percent, passed
if (@($reports | Where-Object { !$_.passed }).Count) { throw "Application frame budget failed; evidence: $output" }
