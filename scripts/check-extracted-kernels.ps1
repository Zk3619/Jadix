param([switch]$Offline)

$ErrorActionPreference = 'Stop'
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$destination = Join-Path $repositoryRoot ("target/extracted-kernel-contracts/" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path (Join-Path $destination 'tests') | Out-Null
Push-Location $repositoryRoot
try {
    $metadata = & cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Workspace metadata failed' }
    $names = @('jadix_lsp', 'jadix_document', 'jadix_core', 'jadix_layout', 'jadix_animation', 'jadix_theme', 'jadix_style_tokens', 'jadix_window_materials', 'jadix_hot_reload')
    $sourceManifest = @()
    foreach ($name in $names) {
        $package = $metadata.packages | Where-Object name -EQ $name
        if (-not $package) { throw "Missing extracted package: $name" }
        $source = Split-Path $package.manifest_path -Parent
        $copy = Join-Path $destination $name
        New-Item -ItemType Directory -Force -Path $copy | Out-Null
        foreach ($entry in @('src', 'tests', 'assets', 'README.md', 'LICENSE', 'NOTICE')) {
            $original = Join-Path $source $entry
            if (Test-Path -LiteralPath $original) {
                Copy-Item -LiteralPath $original -Destination $copy -Recurse
            }
        }
        $manifest = @('[package]', "name = `"$name`"", "version = `"$($package.version)`"", "edition = `"$($package.edition)`"", "license = `"$($package.license)`"", '', '[features]')
        foreach ($feature in $package.features.psobject.Properties) {
            $values = @($feature.Value | ForEach-Object { ConvertTo-Json -Compress -InputObject $_ }) -join ', '
            $manifest += "$($feature.Name) = [$values]"
        }
        $manifest += @('', '[dependencies]')
        foreach ($dependency in $package.dependencies) {
            if ($dependency.kind -or $dependency.target -or ($dependency.source -notlike 'registry+*' -and $dependency.name -notin $names)) {
                throw "Unexpected dependency in kernel package: $name/$($dependency.name)"
            }
            $features = @($dependency.features | ForEach-Object { ConvertTo-Json -Compress -InputObject $_ }) -join ', '
            $optional = $dependency.optional.ToString().ToLowerInvariant()
            $defaults = $dependency.uses_default_features.ToString().ToLowerInvariant()
            $local = if ($dependency.name -in $names) { "path = `"../$($dependency.name)`", " } else { "" }
            $manifest += "$($dependency.name) = { ${local}version = `"$($dependency.req)`", optional = $optional, default-features = $defaults, features = [$features] }"
        }
        [System.IO.File]::WriteAllLines((Join-Path $copy 'Cargo.toml'), $manifest)
        foreach ($original in Get-ChildItem -LiteralPath $source -Recurse -File | Where-Object { $_.FullName -notmatch '[\\/]Cargo.toml$' }) {
            $relative = $original.FullName.Substring($source.Length + 1)
            $copied = Join-Path $copy $relative
            if (-not (Test-Path -LiteralPath $copied)) { throw "Missing copied input: $name/$relative" }
            $originalHash = (Get-FileHash -LiteralPath $original.FullName).Hash
            $copiedHash = (Get-FileHash -LiteralPath $copied).Hash
            $sourceManifest += [pscustomobject]@{ package = $name; path = $relative; original_sha256 = $originalHash; copied_sha256 = $copiedHash }
            if ($originalHash -ne $copiedHash) {
                throw "Copied source or resource differs: $name/$relative"
            }
        }
    }
    $sourceManifest | ConvertTo-Json | Set-Content (Join-Path $destination 'source-manifest.json')
    $consumerManifest = @"
[package]
name = "extracted-kernel-consumer"
version = "0.0.0"
edition = "2021"

[workspace]

[features]
expanded = ["jadix_document/diff", "jadix_document/syntax", "jadix_document/markdown", "jadix_animation/animation-path", "jadix_theme/themes"]

[dependencies]
jadix_document = { path = "jadix_document" }
jadix_lsp = { path = "jadix_lsp" }
jadix_core = { path = "jadix_core", default-features = false }
jadix_layout = { path = "jadix_layout", default-features = false }
jadix_animation = { path = "jadix_animation", default-features = false }
jadix_theme = { path = "jadix_theme", default-features = false }
jadix_window_materials = { path = "jadix_window_materials", default-features = false }
jadix_hot_reload = { path = "jadix_hot_reload", default-features = false }
"@
    [System.IO.File]::WriteAllText((Join-Path $destination 'Cargo.toml'), $consumerManifest)
    Copy-Item -LiteralPath (Join-Path $repositoryRoot 'scripts/fixtures/extracted-kernel-contracts.rs') -Destination (Join-Path $destination 'tests/contracts.rs')
    Write-Host "Independent crate/source kernel workspace: $destination"
    $options = @('--manifest-path', (Join-Path $destination 'Cargo.toml'), '--target-dir', (Join-Path $repositoryRoot 'target/extracted-kernel-build'))
    if ($Offline) { $options += '--offline' }
    foreach ($distribution in @('crate', 'source')) {
        $selectedManifest = $consumerManifest
        if ($distribution -eq 'crate') {
            foreach ($name in $names) {
                $path = (Join-Path $repositoryRoot "crates/$name").Replace('\', '/')
                $selectedManifest = $selectedManifest.Replace("path = `"$name`"", "path = `"$path`"")
            }
        }
        [System.IO.File]::WriteAllText((Join-Path $destination 'Cargo.toml'), $selectedManifest)
        & cargo clean @options -p extracted-kernel-consumer
        if ($LASTEXITCODE -ne 0) { throw 'Consumer cache invalidation failed' }
        foreach ($selection in @('', 'expanded')) {
            $features = @()
            if ($selection) { $features += @('--features', $selection) }
            & cargo test @options @features -- --test-threads=1
            if ($LASTEXITCODE -ne 0) { throw "Independent kernel contracts failed: $selection" }
            $treeOptions = @('--manifest-path', (Join-Path $destination 'Cargo.toml'))
            if ($Offline) { $treeOptions += '--offline' }
            $tree = & cargo tree @treeOptions @features --edges normal,build --prefix none --format '{p}'
            if ($LASTEXITCODE -ne 0) { throw 'Independent kernel dependency tree failed' }
            if ($tree | Select-String '^(jadix|jadix_macros|wgpu|winit|jadix_winit|glyphon) v') {
                throw 'Extracted kernels leak the UI runtime or authoring parser'
            }
            $tree | Set-Content (Join-Path $destination "dependencies-$distribution-$selection.txt")
        }
    }
} finally { Pop-Location }
