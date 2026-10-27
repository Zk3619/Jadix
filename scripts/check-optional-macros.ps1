param([switch]$Offline)

$ErrorActionPreference = 'Stop'
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$destination = Join-Path $repositoryRoot ("target/optional-macro-contracts/" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path (Join-Path $destination 'tests') | Out-Null
$runtimePath = (Join-Path $repositoryRoot 'crates/jadix').Replace('\', '/')
$workspaceManifest = Get-Content (Join-Path $repositoryRoot 'Cargo.toml') -Raw
$patchStart = $workspaceManifest.IndexOf('[patch.crates-io]')
$patchEnd = $workspaceManifest.IndexOf('[workspace.package]')
if ($patchStart -lt 0 -or $patchEnd -le $patchStart) { throw 'Workspace patch boundaries not found' }
$patches = $workspaceManifest.Substring($patchStart, $patchEnd - $patchStart)
$patches = [regex]::Replace($patches, 'path = "([^"\r\n]+)"', {
    param($match)
    $path = [System.IO.Path]::GetFullPath((Join-Path $repositoryRoot $match.Groups[1].Value)).Replace('\', '/')
    'path = "' + $path + '"'
})
@"
[package]
name = "jadix_optional_macro_consumer"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[features]
default = []
macros = ["ui_runtime/macros"]

[dependencies]
ui_runtime = { package = "jadix", path = "$runtimePath", default-features = false }

$patches
"@ | Set-Content (Join-Path $destination 'Cargo.toml')
Copy-Item -LiteralPath (Join-Path $repositoryRoot 'crates/jadix/tests/fixtures/optional_macros/consumer.rs') -Destination (Join-Path $destination 'tests/consumer.rs')
Copy-Item -LiteralPath (Join-Path $repositoryRoot 'Cargo.lock') -Destination (Join-Path $destination 'Cargo.lock')
$options = @('--manifest-path', (Join-Path $destination 'Cargo.toml'), '--no-default-features')
if ($Offline) { $options += '--offline' }
Write-Host "Standalone renamed-runtime consumer: $destination"
foreach ($mode in @('builder', 'macros')) {
    $selection = @()
    if ($mode -eq 'macros') { $selection = @('--features', 'macros') }
    $tree = & cargo tree @options @selection --edges normal,build --prefix none --format '{p}'
    if ($LASTEXITCODE -ne 0) { throw "Dependency inspection failed: $mode" }
    $tree | Set-Content (Join-Path $destination "dependencies-$mode.txt")
    $hasUiMacros = @($tree | Select-String '^jadix_macros v').Count -gt 0
    if ($hasUiMacros -ne ($mode -eq 'macros')) { throw "UI macro dependency mismatch: $mode" }
    Write-Host "UI macro dependency present: $hasUiMacros (mode=$mode)"
    & cargo test @options @selection --target-dir (Join-Path $repositoryRoot 'target') --test consumer -- --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Standalone consumer failed: $mode" }
}
