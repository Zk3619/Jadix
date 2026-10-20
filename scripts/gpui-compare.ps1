param(
    [ValidateSet('Build', 'Run')][string]$Mode = 'Run',
    [string]$OutputDirectory = '.tmp/d3d11-compare-20261018',
    [string]$AngleLibrary,
    [int]$Seconds = 10,
    [int]$Rounds = 3
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $repo
$output = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    [IO.Path]::GetFullPath($OutputDirectory)
} else {
    [IO.Path]::GetFullPath((Join-Path $repo $OutputDirectory))
}
New-Item -ItemType Directory -Path $output -Force | Out-Null
if ($Mode -eq 'Build') {
    $env:CARGO_INCREMENTAL = '0'
    cargo build --release --locked -p jadix-frame-compare --features dx11-angle -j 2
    if ($LASTEXITCODE) { throw 'Jadix build failed' }
    Copy-Item -LiteralPath target/release/jadix-frame-compare.exe -Destination "$output/jadix.exe"
    cargo build --release --locked --manifest-path scripts/gpui-compare/Cargo.toml -j 2
    if ($LASTEXITCODE) { throw 'Kit build failed' }
    Copy-Item -LiteralPath scripts/gpui-compare/target/release/gpui-compare.exe -Destination "$output/gpui-kit.exe"
    cargo build --release --locked --manifest-path scripts/gpui-compare/Cargo.toml --features fast -j 2
    if ($LASTEXITCODE) { throw 'Fast build failed' }
    Copy-Item -LiteralPath scripts/gpui-compare/target/release/gpui-compare.exe -Destination "$output/gpui-fast.exe"
    $manifest = [ordered]@{
        built_at = (Get-Date -Format o)
        profile = 'release: opt-level=3, codegen-units=1, lto=fat, debug=false, strip=symbols, panic=abort'
        variants = 'Jadix a11y/ANGLE hardware D3D11; Kit Base-only/native D3D11; identical Kit adapter with official gpui-fast feature/native D3D11'
        native_wgpu_dx11_ready = $false
        hashes = @(Get-FileHash -Algorithm SHA256 -LiteralPath "$output/jadix.exe", "$output/gpui-kit.exe", "$output/gpui-fast.exe", 'Cargo.lock', 'scripts/gpui-compare/Cargo.lock', 'scripts/gpui-compare/workload.rs', 'examples/frame_compare/src/main.rs', 'scripts/gpui-compare/src/main.rs', 'scripts/gpui-compare/gpu_identity.rs', 'scripts/gpui-compare/angle_identity.rs', 'vendor/wgpu-hal/src/gles/egl.rs', 'vendor/wgpu-hal/src/gles/mod.rs', 'vendor/gpui-pre-windows-0.3.9/src/directx_renderer.rs', 'vendor/gpui-fast-windows-0.1.9/src/directx_renderer.rs' | Select-Object Path,Hash)
        cpu = @(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors)
        gpus = @(Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,CurrentRefreshRate)
        os = @(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber)
    }
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath "$output/build-manifest.json" -Encoding utf8
    return
}
# Rotate order to expose drift; only one visible GUI process runs at a time.
if (-not $AngleLibrary) { throw 'Run requires -AngleLibrary with an absolute libEGL.dll path' }
$library = (Resolve-Path -LiteralPath $AngleLibrary).Path
@(Get-FileHash -Algorithm SHA256 -LiteralPath $library,
    (Join-Path (Split-Path $library -Parent) 'libGLESv2.dll') | Select-Object Path,Hash) |
    ConvertTo-Json | Set-Content -LiteralPath "$output/angle-libraries.json" -Encoding utf8
$previousBackend = $env:JADIX_GPU_BACKEND
$previousLibrary = $env:WGPU_ANGLE_LIBRARY
$env:JADIX_GPU_BACKEND = 'dx11-angle'
$env:WGPU_ANGLE_LIBRARY = $library
try {
$engines = @('jadix', 'gpui-kit', 'gpui-fast')
for ($round = 0; $round -lt $Rounds; $round++) {
    for ($slot = 0; $slot -lt $engines.Count; $slot++) {
        $engine = $engines[($round + $slot) % $engines.Count]
        $name = "$engine-$($round + 1)"
        python scripts/cpu-bench.py --exe "$output/$engine.exe" --calibration --client-size --width 1280 --height 800 --sections ordinary,dense,nested --seconds $Seconds --rounds 1 --warmup 5 --no-gate --screenshots --output "$output/$name.json" *> "$output/$name.log"
        if ($LASTEXITCODE) { throw "Capture failed: $name" }
        $capture = Get-Content -Raw -LiteralPath "$output/$name.json" | ConvertFrom-Json
        if (@($capture.results | Where-Object { -not $_.valid }).Count) { throw "Invalid capture: $name; inspect raw evidence" }
        Write-Output "Completed $name"
    }
}
python scripts/gpui-compare/summarize.py $output --rounds $Rounds
if ($LASTEXITCODE) { throw 'Comparison incomplete or invalid' }
} finally {
    $env:JADIX_GPU_BACKEND = $previousBackend
    $env:WGPU_ANGLE_LIBRARY = $previousLibrary
}
