param(
    [ValidateSet('dx12', 'gl')]
    [string[]]$Backends = @('dx12', 'gl'),
    [string]$OutputDirectory = "development-docs/engine/perf-$(Get-Date -Format yyyyMMdd)"
)

$ErrorActionPreference = 'Stop'
$previousBackend = $env:JADIX_UPLOAD_BACKEND
$previousRetained = $env:JADIX_RETAINED_UPLOADS
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
    New-Item -ItemType Directory -Force -Path $outputRoot | Out-Null
    Remove-Item Env:JADIX_RETAINED_UPLOADS -ErrorAction SilentlyContinue
    $rows = @()
    $adapters = @()
    foreach ($backend in $Backends) {
        $env:JADIX_UPLOAD_BACKEND = $backend
        # Keep the GPU/CPU workloads sequential: concurrent test suites bias timings.
        foreach ($filter in @('scene_submission_', 'retained_scene_upload_history_and_budget')) {
            $kind = if ($filter -eq 'scene_submission_') { 'pixels' } else { 'upload' }
            $log = Join-Path $outputRoot "$kind-$backend.log"
            & cargo test -p jadix --lib $filter -- --ignored --nocapture --test-threads=1 2>&1 |
                Tee-Object -FilePath $log | Out-Host
            if ($LASTEXITCODE -ne 0) { throw "$filter failed on $backend; see $log" }
        }
        $content = Get-Content (Join-Path $outputRoot "upload-$backend.log")
        $adapters += [ordered]@{ backend = $backend; observed = @($content | Where-Object { $_ -match '^adapter=' }) }
        foreach ($line in $content) {
            if ($line -match '^upload-budget workload=(\S+) retained=(true|false) p50_ms=([\d.]+) p95_ms=([\d.]+) source_bytes=(\d+) uploaded_bytes=(\d+) writes=(\d+)') {
                $rows += [ordered]@{
                    backend = $backend; workload = $Matches[1]; retained = $Matches[2] -eq 'true'
                    p50_ms = [double]::Parse($Matches[3], [cultureinfo]::InvariantCulture)
                    p95_ms = [double]::Parse($Matches[4], [cultureinfo]::InvariantCulture)
                    source_bytes = [long]$Matches[5]; uploaded_bytes = [long]$Matches[6]
                    write_calls = [long]$Matches[7]
                }
            }
        }
    }
    if ($rows.Count -ne 6 * $Backends.Count) { throw 'Incomplete benchmark output' }
    $sources = @(
        'crates/jadix/src/renderer/buffer_upload.rs', 'crates/jadix/src/renderer/pipeline.rs',
        'crates/jadix/src/app/state.rs', 'crates/jadix/src/app/secondary_window.rs',
        'crates/jadix/src/app/frame_stats.rs', 'crates/jadix/src/app/state_scene_submission_tests.rs'
    ) | ForEach-Object { [ordered]@{ path = $_; sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() } }
    $summary = [ordered]@{
        captured_at = (Get-Date).ToString('o'); base_revision = (& git rev-parse HEAD)
        modified_files = @(& git diff --name-only); source_files = @($sources)
        rustc = @(& rustc -Vv); os = [Environment]::OSVersion.VersionString
        profile = 'cargo test default features; workspace dev package jadix opt-level=2'
        source_bytes_per_sample = 4194304; warmup = 20; samples = 120
        timing = 'CPU upload+submit; queue completion waited outside timing; not GPU execution time'
        comparison_scope = 'Jadix full versus retained upload; no Kit/Fast runtime measurement'
        adapters = $adapters; measurements = $rows
    }
    $summary | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $outputRoot 'summary.json')
    Write-Host "Saved benchmark evidence to $outputRoot"
}
finally {
    $env:JADIX_UPLOAD_BACKEND = $previousBackend
    $env:JADIX_RETAINED_UPLOADS = $previousRetained
    Pop-Location
}
