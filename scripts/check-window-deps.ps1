# Winit/OHOS trait objects must originate from one core and one DPI package.
$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $repo "Cargo.toml"
$json = & cargo metadata --locked --format-version 1 --manifest-path $manifest
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
$metadata = $json | ConvertFrom-Json
$expectedSource = "git+https://github.com/rust-windowing/winit.git?rev=758a50e35a928dbf72bf05ae91f12fee7a540a83#758a50e35a928dbf72bf05ae91f12fee7a540a83"
foreach ($name in @("winit-core", "dpi")) {
    $packages = @($metadata.packages | Where-Object name -eq $name)
    if ($packages.Count -ne 1 -or $packages[0].source -ne $expectedSource) {
        throw "Expected exactly one $name package from the pinned official source"
    }
}
$official = @("winit-android", "winit-appkit", "winit-common", "winit-orbital", "winit-uikit", "winit-web", "winit-x11")
foreach ($name in $official) {
    $packages = @($metadata.packages | Where-Object name -eq $name)
    if ($packages.Count -ne 1 -or $packages[0].source -ne $expectedSource) {
        throw "Official backend $name is missing or has drifted from the core revision"
    }
}
foreach ($name in @("winit-wayland", "winit-win32")) {
    $packages = @($metadata.packages | Where-Object name -eq $name)
    $expectedManifest = [IO.Path]::GetFullPath((Join-Path $repo "vendor/$name/Cargo.toml"))
    if ($packages.Count -ne 1 -or $null -ne $packages[0].source -or
        [IO.Path]::GetFullPath($packages[0].manifest_path) -ne $expectedManifest) {
        throw "Expected exactly one local $name backend from vendor/$name"
    }
    $provenance = Get-Content -Raw (Join-Path $repo "vendor/$name/JADIX_PATCH.md")
    if (-not $provenance.Contains("758a50e35a928dbf72bf05ae91f12fee7a540a83")) {
        throw "Local $name provenance has drifted from the shared core revision"
    }
}
$ohos = @($metadata.packages | Where-Object name -eq "winit-ohos")
if ($ohos.Count -ne 1 -or $ohos[0].source -ne "git+https://github.com/ohos-rs/winit-ohos.git?rev=af4063b4e75b62b152a315d2606d799ca65bda84#af4063b4e75b62b152a315d2606d799ca65bda84") {
    throw "OHOS backend is missing or has drifted from its reviewed revision"
}
if (@($metadata.packages | Where-Object name -eq "winit").Count -ne 0) {
    throw "An upstream facade was resolved alongside jadix_winit"
}
Write-Output "Window dependency identities verified: one core, one DPI, pinned backends and two documented local extensions."
