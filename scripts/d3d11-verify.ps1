param(
    [Parameter(Mandatory)][string]$AngleLibrary,
    [string]$OutputDirectory = '.tmp/d3d11-20261018',
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath (Split-Path $PSScriptRoot -Parent)
$library = (Resolve-Path -LiteralPath $AngleLibrary).Path
$output = [IO.Path]::GetFullPath((Join-Path $PWD $OutputDirectory))
New-Item -ItemType Directory -Path $output -Force | Out-Null
if (-not $SkipBuild) {
    cargo build --locked -p d3d11_probe --features angle,native-hal -j 2
    if ($LASTEXITCODE) { throw 'D3D11 probe build failed' }
}
$previous = $env:WGPU_ANGLE_LIBRARY
try {
    $env:WGPU_ANGLE_LIBRARY = $library
    foreach ($mode in @('native', 'native-hal', 'angle')) {
        & ./target/debug/d3d11_probe.exe $mode > "$output/probe-$mode.json" 2> "$output/probe-$mode.log"
        if ($LASTEXITCODE) { throw "$mode probe failed; inspect $output/probe-$mode.log" }
    }
    $native = Get-Content -Raw "$output/probe-native.json" | ConvertFrom-Json
    $angle = Get-Content -Raw "$output/probe-angle.json" | ConvertFrom-Json
    $hal = Get-Content -Raw "$output/probe-native-hal.json" | ConvertFrom-Json
    if ($native.pixel_errors -ne 0 -or $angle.pixel_errors -ne 0) { throw 'Pixel oracle failed' }
    if ($hal.pixel_errors -ne 0 -or ($hal.dxgi.adapter_luid -join ',') -ne ($native.dxgi.adapter_luid -join ',') -or
        $hal.dxgi.vendor_id -ne $native.dxgi.vendor_id -or $hal.dxgi.device_id -ne $native.dxgi.device_id) {
        throw 'Native HAL offscreen pixels or hardware identity mismatch'
    }
    if (-not $hal.hal_resources_and_draw -or -not $hal.uniform_write_discard) {
        throw 'Native HAL probe did not exercise resource and draw commands'
    }
    if ($native.dxgi.vendor_id -ne $angle.dxgi.vendor_id -or $native.dxgi.device_id -ne $angle.dxgi.device_id -or
        ($native.dxgi.adapter_luid -join ',') -ne ($angle.dxgi.adapter_luid -join ',')) {
        throw 'The native and ANGLE probes selected different GPUs'
    }
    [ordered]@{
        status = 'native-offscreen-resources-and-angle-surface-smoke'
        native_wgpu_ui_ready = $false
        limitations = 'Native HAL uniform resources, draw and readback are exercised; crate::Api registration, texture/storage bindings and UI presentation remain incomplete.'
        native_gpu = $native.device_name
        angle_gpu = $angle.device_name
        pixel_errors = @($native.pixel_errors, $hal.pixel_errors, $angle.pixel_errors)
        angle_surface_pixels = $angle.surface_pixel
        angle_sdf_polygon_clip_draw = $angle.sdf_draw_with_polygon_clip
        hashes = @(Get-FileHash -Algorithm SHA256 -LiteralPath './target/debug/d3d11_probe.exe', $library,
            (Join-Path (Split-Path $library -Parent) 'libGLESv2.dll') | Select-Object Path,Hash)
    } | ConvertTo-Json -Depth 5 | Set-Content "$output/probe-summary.json" -Encoding utf8
} finally {
    $env:WGPU_ANGLE_LIBRARY = $previous
}
