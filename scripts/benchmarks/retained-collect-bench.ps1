param(
    [Parameter(Mandatory)][string]$BaselineExecutable,
    [Parameter(Mandatory)][string]$CandidateExecutable,
    [ValidateRange(1, 20)][int]$Rounds = 5,
    [ValidateRange(0, 100)][double]$MaxRegressionPercent = 10,
    [string]$OutputDirectory = 'development-docs/engine/perf-20261017-frame5'
)

$ErrorActionPreference = 'Stop'
$executables = @{
    baseline = (Resolve-Path -LiteralPath $BaselineExecutable).Path
    candidate = (Resolve-Path -LiteralPath $CandidateExecutable).Path
}
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
$measurements = @()
for ($round = 1; $round -le $Rounds; $round++) {
    # Reverse order on alternate rounds; run only after compilation has ended.
    $order = if ($round % 2) { @('baseline', 'candidate') } else { @('candidate', 'baseline') }
    foreach ($variant in $order) {
        $lines = @(& $executables[$variant] retained_collect_frame_budget --ignored --nocapture --test-threads=1 2>&1)
        $exitCode = $LASTEXITCODE
        $lines | Set-Content -Encoding utf8 (Join-Path $outputRoot "$round-$variant.log")
        if ($exitCode -ne 0) { throw "Benchmark failed: round $round $variant" }
        $count = 0
        foreach ($line in $lines) {
            if ("$line" -match '^retained-collect workload=(\S+) rows=256 depth=5 samples=200 p50_ms=([\d.]+) p95_ms=([\d.]+)') {
                $measurements += [ordered]@{
                    round = $round; variant = $variant; workload = $Matches[1]
                    p50_ms = [double]::Parse($Matches[2], [cultureinfo]::InvariantCulture)
                    p95_ms = [double]::Parse($Matches[3], [cultureinfo]::InvariantCulture)
                }
                $count++
            }
        }
        if ($count -ne 3) { throw "Incomplete benchmark: round $round $variant" }
    }
}
function Get-Median($values) {
    $sorted = @($values | Sort-Object)
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2) { return $sorted[$middle] }
    return ($sorted[$middle - 1] + $sorted[$middle]) / 2
}
$comparison = foreach ($workload in @('idle', 'one-leaf', 'all')) {
    $before = @($measurements | Where-Object { $_.variant -eq 'baseline' -and $_.workload -eq $workload })
    $after = @($measurements | Where-Object { $_.variant -eq 'candidate' -and $_.workload -eq $workload })
    $beforeP50 = Get-Median @($before | ForEach-Object { $_.p50_ms })
    $afterP50 = Get-Median @($after | ForEach-Object { $_.p50_ms })
    $beforeP95 = Get-Median @($before | ForEach-Object { $_.p95_ms })
    $afterP95 = Get-Median @($after | ForEach-Object { $_.p95_ms })
    [ordered]@{
        workload = $workload
        baseline_p50_ms = $beforeP50; candidate_p50_ms = $afterP50
        baseline_p95_ms = $beforeP95; candidate_p95_ms = $afterP95
        p50_reduction_percent = 100 * (1 - $afterP50 / $beforeP50)
        within_budget = $afterP50 -le $beforeP50 * (1 + $MaxRegressionPercent / 100) -and
            $afterP95 -le $beforeP95 * (1 + $MaxRegressionPercent / 100)
    }
}
$binaryEvidence = foreach ($variant in @('baseline', 'candidate')) {
    [ordered]@{ variant = $variant; path = $executables[$variant]
        sha256 = (Get-FileHash -LiteralPath $executables[$variant] -Algorithm SHA256).Hash.ToLowerInvariant() }
}
$sourceEvidence = foreach ($path in @('crates/jadix/src/layout/collect_cache.rs', 'crates/jadix/src/layout/layout_tree.rs', 'crates/jadix/src/crate_tests/layout/collect_skip.rs', 'crates/jadix/src/app/secondary_window.rs')) {
    [ordered]@{ path = $path; sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() }
}
[ordered]@{
    captured_at = (Get-Date).ToString('o'); base_revision = (& git rev-parse HEAD)
    os = [Environment]::OSVersion.VersionString; rustc = @(& rustc -Vv)
    binaries = @($binaryEvidence); candidate_sources = @($sourceEvidence)
    rounds = $Rounds; measurements = $measurements; comparison = @($comparison)
    max_regression_percent = $MaxRegressionPercent
    scope = 'Same 256-row five-level CPU collect fixture. Median of per-round quantiles; not application FPS or a Kit/Fast comparison.'
} | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $outputRoot 'summary.json')
$comparison | ForEach-Object { [pscustomobject]$_ } | Format-Table | Out-Host
if (@($comparison | Where-Object { -not $_.within_budget }).Count) {
    throw "CPU collect regression exceeds $MaxRegressionPercent percent; see summary.json"
}
