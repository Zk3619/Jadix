param(
    [ValidateSet('gl', 'dx12')][string]$Backend = 'gl',
    [ValidateRange(1, 8)][int]$Rounds = 3,
    [ValidateRange(20, 800)][int]$Samples = 50,
    [switch]$BuildOnly,
    [switch]$SkipBuild,
    [string]$Adapter = '',
    [ValidatePattern('^[a-z0-9-]*$')][string]$Label = '',
    [ValidatePattern('^[a-z0-9_,]*$')][string]$Scenarios = ''
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$lab = Join-Path $repo 'target/glyphon-ab'
$registry = Join-Path $env:USERPROFILE '.cargo/registry/src'
$upstream = Get-ChildItem $registry -Directory | ForEach-Object {
    Get-ChildItem $_.FullName -Directory -Filter 'glyphon-0.12.0'
} | Select-Object -First 1
if (!$upstream) { throw 'glyphon 0.12.0 must already exist in the Cargo registry cache.' }
New-Item -ItemType Directory -Force $lab | Out-Null

if (!$SkipBuild) {
# All instrumentation is applied to disposable copies, never to production sources.
foreach ($variant in @('base', 'patch')) {
    $source = if ($variant -eq 'base') { $upstream.FullName } else { Join-Path $repo 'vendor/glyphon' }
    $destination = Join-Path $lab $variant
    New-Item -ItemType Directory -Force $destination | Out-Null
    Copy-Item (Join-Path $source 'src') $destination -Recurse -Force
    $manifest = Get-Content (Join-Path $repo 'vendor/glyphon/Cargo.toml') -Raw
    $manifest = $manifest.Replace('"name" = "jadix_glyphon"', "`"name`" = `"glyphon_bench_$variant`"")
    $gpuPath = (Join-Path $repo "vendor/wgpu").Replace("\", "/")
    $manifest = $manifest.Replace('"path" = "../wgpu"', "`"path`" = `"$gpuPath`"")
    $manifest += "`n[features]`nbench-metrics = []`n"
    Set-Content (Join-Path $destination 'Cargo.toml') $manifest -Encoding utf8
    $libPath = Join-Path $destination 'src/lib.rs'
    $lib = Get-Content $libPath -Raw
    $lib += @'

#[cfg(feature = "bench-metrics")]
pub mod bench_metrics {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    static CALLS: AtomicU64 = AtomicU64::new(0);
    static BYTES: AtomicU64 = AtomicU64::new(0);
    pub fn record(bytes: u64) { CALLS.fetch_add(1, Relaxed); BYTES.fetch_add(bytes, Relaxed); }
    pub fn take() -> (u64, u64) { (CALLS.swap(0, Relaxed), BYTES.swap(0, Relaxed)) }
}
'@
    Set-Content $libPath $lib -Encoding utf8
    foreach ($file in @('text_atlas.rs', 'text_render.rs')) {
        $path = Join-Path $destination "src/$file"
        $code = Get-Content $path -Raw
        if ($variant -eq 'base') {
            $bytes = if ($file -eq 'text_atlas.rs') { 'width as u64 * height as u64 * self.kind.num_channels() as u64' } else { 'image.width as u64 * image.height as u64 * inner.num_channels() as u64' }
            $needle = 'state.queue.write_texture('
        } else {
            $bytes = 'width as u64 * height as u64 * channels as u64'
            $needle = 'queue.write_texture('
        }
        if ($code.Contains($needle)) {
            $code = $code.Replace($needle, "#[cfg(feature = `"bench-metrics`")]`ncrate::bench_metrics::record($bytes);`n$needle")
        }
        if ($file -eq 'text_atlas.rs') {
            $mirror = if ($variant -eq 'patch') { 'self.mask_atlas.pixels.data.len() + self.color_atlas.pixels.data.len()' } else { '0' }
            $code += @"

#[cfg(feature = "bench-metrics")]
impl TextAtlas {
    pub fn bench_memory(&self) -> (usize, u64, u32, u32) {
        ($mirror,
         self.mask_atlas.size as u64 * self.mask_atlas.size as u64 + 4 * self.color_atlas.size as u64 * self.color_atlas.size as u64,
         self.mask_atlas.size, self.color_atlas.size)
    }
}
"@
        }
        Set-Content $path $code -Encoding utf8
    }
}
$project = Join-Path $lab 'harness'
New-Item -ItemType Directory -Force (Join-Path $project 'src') | Out-Null
Copy-Item (Join-Path $PSScriptRoot 'main.rs') (Join-Path $project 'src/main.rs') -Force
@'
[package]
name = "glyphon-ab"
version = "0.0.0"
edition = "2021"
[workspace]
[features]
default = []
patched = ["dep:glyphon_patch"]
baseline = ["dep:glyphon_base"]
metrics = ["glyphon_patch?/bench-metrics", "glyphon_base?/bench-metrics"]
[dependencies]
glyphon_base = { package = "glyphon_bench_base", path = "../base", optional = true }
glyphon_patch = { package = "glyphon_bench_patch", path = "../patch", optional = true }
wgpu = { package = "jadix_wgpu", path = "GPU_PATH", version = "=30.0.0", default-features = false, features = ["std", "parking_lot", "wgsl", "gles", "dx12"] }
[profile.release]
opt-level = 2
lto = false
codegen-units = 16
'@.Replace('GPU_PATH', (Join-Path $repo 'vendor/wgpu').Replace('\', '/')) | Set-Content (Join-Path $project 'Cargo.toml') -Encoding utf8
Copy-Item (Join-Path $repo 'Cargo.lock') (Join-Path $project 'Cargo.lock') -Force
foreach ($variant in @('baseline', 'patched')) {
    foreach ($mode in @('timing', 'metrics')) {
        $features = if ($mode -eq 'metrics') { "$variant,metrics" } else { $variant }
        & cargo build --release --offline --manifest-path (Join-Path $project 'Cargo.toml') --features $features --target-dir (Join-Path $lab 'build')
        if ($LASTEXITCODE) { throw "Build failed: $features" }
        Copy-Item (Join-Path $lab 'build/release/glyphon-ab.exe') (Join-Path $lab "$variant-$mode.exe") -Force
    }
}
}
if ($BuildOnly) { return }
if (!$Label) { $Label = $Backend }
$metadata = [ordered]@{
    date = (Get-Date).ToString('o')
    backend = $Backend
    adapter_filter = $Adapter
    scenarios = $Scenarios
    rounds = $Rounds
    samples = $Samples
    rustc = (& rustc --version)
    commit = (& git -C $repo rev-parse HEAD)
    cpu = (Get-CimInstance Win32_Processor | Select-Object -ExpandProperty Name)
    graphics = @(Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion)
    upstream_source = $upstream.FullName
    font_sha256 = (Get-FileHash (Join-Path $repo 'crates/jadix/assets/NotoSansCJK.ttc')).Hash
    patched_atlas_sha256 = (Get-FileHash (Join-Path $repo 'vendor/glyphon/src/text_atlas.rs')).Hash
}
$metadata | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $lab "$Label-metadata.json") -Encoding utf8
$font = Join-Path $repo 'crates/jadix/assets/NotoSansCJK.ttc'
foreach ($round in 0..($Rounds - 1)) {
    $order = if ($round % 2 -eq 0) { @('baseline', 'patched') } else { @('patched', 'baseline') }
    foreach ($variant in $order) {
        $output = Join-Path $lab "$Label-$variant-$round.csv"
        & (Join-Path $lab "$variant-timing.exe") $Backend $font $Samples $Adapter $Scenarios 2> "$output.adapter.txt" | Set-Content $output -Encoding utf8
        if ($LASTEXITCODE) { throw "Benchmark failed: $variant" }
    }
}
foreach ($variant in @('baseline', 'patched')) {
    $output = Join-Path $lab "$Label-$variant-metrics.csv"
    & (Join-Path $lab "$variant-metrics.exe") $Backend $font 8 $Adapter $Scenarios 2> "$output.adapter.txt" | Set-Content $output -Encoding utf8
    if ($LASTEXITCODE) { throw "Metrics failed: $variant" }
}
& node (Join-Path $PSScriptRoot 'summarize.mjs') $lab $Label
if ($LASTEXITCODE) { throw 'Pixel comparison or result summary failed.' }
Write-Host "Results: $lab"
