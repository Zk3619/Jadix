param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$destination = Join-Path $root ("target/product-distributions/" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path (Join-Path $destination 'tests') | Out-Null
Push-Location $root
try {
    $metadata = & cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Workspace metadata failed' }
    $names = @('jadix_components', 'jadix_dnd', 'jadix_lucide')
    $hashes = @()
    foreach ($name in $names) {
        $package = $metadata.packages | Where-Object name -EQ $name
        $source = Split-Path $package.manifest_path -Parent
        $copy = Join-Path $destination $name
        New-Item -ItemType Directory -Force -Path $copy | Out-Null
        foreach ($entry in @('src', 'assets', 'LICENSE', 'NOTICE', 'README.md')) {
            $original = Join-Path $source $entry
            if (Test-Path -LiteralPath $original) { Copy-Item -LiteralPath $original -Destination $copy -Recurse }
        }
        foreach ($file in Get-ChildItem -LiteralPath $copy -Recurse -File) {
            $relative = $file.FullName.Substring($copy.Length + 1)
            $before = (Get-FileHash -LiteralPath (Join-Path $source $relative)).Hash
            $after = (Get-FileHash -LiteralPath $file.FullName).Hash
            if ($before -ne $after) { throw "Copied source differs: $name/$relative" }
            $hashes += [pscustomobject]@{ package = $name; path = $relative; sha256 = $before }
        }
        $manifest = @('[package]', "name = `"$name`"", "version = `"$($package.version)`"", "edition = `"$($package.edition)`"", "license = `"$($package.license)`"", '', '[lib]', 'test = false', '', '[features]')
        foreach ($feature in $package.features.psobject.Properties) {
            $values = @($feature.Value | ForEach-Object { ConvertTo-Json -Compress -InputObject $_ }) -join ', '
            $manifest += "$($feature.Name) = [$values]"
        }
        $manifest += @('', '[dependencies]')
        foreach ($dependency in $package.dependencies | Where-Object { -not $_.kind }) {
            if ($dependency.target) { throw "Unexpected target-specific product dependency: $name" }
            $key = if ($dependency.rename) { $dependency.rename } else { $dependency.name }
            $selection = @("package = `"$($dependency.name)`"")
            if ($dependency.path) {
                $path = if ($dependency.name -in $names) { "../$($dependency.name)" } else { $dependency.path.Replace('\', '/') }
                $selection += "path = `"$path`""
            } elseif ($dependency.source -like 'registry+*') {
                $selection += "version = `"$($dependency.req)`""
            } else { throw "Unsupported product dependency: $name/$key" }
            $features = @($dependency.features | ForEach-Object { ConvertTo-Json -Compress -InputObject $_ }) -join ', '
            $selection += "default-features = $($dependency.uses_default_features.ToString().ToLowerInvariant())", "optional = $($dependency.optional.ToString().ToLowerInvariant())", "features = [$features]"
            $manifest += "$key = { $($selection -join ', ') }"
        }
        $manifest += @('', '[lints.rust]', 'unexpected_cfgs = { level = "warn", check-cfg = [''cfg(feature, values("product-tests", "gallery-tests"))''] }')
        [IO.File]::WriteAllLines((Join-Path $copy 'Cargo.toml'), $manifest)
    }
    $hashes | ConvertTo-Json | Set-Content (Join-Path $destination 'source-manifest.json')
    Copy-Item -LiteralPath (Join-Path $root 'scripts/checks/fixtures/product-distributions.rs') -Destination (Join-Path $destination 'tests/contracts.rs')
    $runtime = (Join-Path $root 'crates/jadix').Replace('\', '/')
    foreach ($distribution in @('crate', 'source')) {
        $paths = @{}
        foreach ($name in $names) {
            $paths[$name] = if ($distribution -eq 'source') { $name } else { (Join-Path $root "crates/$name").Replace('\', '/') }
        }
        @"
[package]
name = "product-consumer-$distribution"
version = "0.0.0"
edition = "2021"
[workspace]
[features]
macros = ["widgets/macros", "icons/macros", "drag_product/macros"]
animation-path = ["icons/animation-path"]
[dependencies]
ui_runtime = { package = "jadix", path = "$runtime", default-features = false, features = ["svg", "animation-base"] }
widgets = { package = "jadix_components", path = "$($paths['jadix_components'])", default-features = false }
icons = { package = "jadix_lucide", path = "$($paths['jadix_lucide'])", default-features = false, features = ["animated", "svg"] }
drag_product = { package = "jadix_dnd", path = "$($paths['jadix_dnd'])", default-features = false }
"@ | Set-Content (Join-Path $destination 'Cargo.toml')
        $options = @('--manifest-path', (Join-Path $destination 'Cargo.toml'), '--target-dir', (Join-Path $root 'target'))
        if ($Offline) { $options += '--offline' }
        foreach ($mode in @('builder', 'macros', 'macros-path')) {
            $features = if ($mode -eq 'macros-path') { @('--features', 'macros,animation-path') } elseif ($mode -eq 'macros') { @('--features', 'macros') } else { @() }
            $treeOptions = @('--manifest-path', (Join-Path $destination 'Cargo.toml'))
            if ($Offline) { $treeOptions += '--offline' }
            $tree = & cargo tree @treeOptions @features --edges normal,build --prefix none --format '{p}'
            if ($LASTEXITCODE -ne 0) { throw 'Product dependency inspection failed' }
            $hasMacros = @($tree | Select-String '^jadix_macros v').Count -gt 0
            if ($hasMacros -ne ($mode -ne 'builder')) { throw "Product macro dependency mismatch: $distribution/$mode" }
            $tree | Set-Content (Join-Path $destination "dependencies-$distribution-$mode.txt")
            Write-Host "Product distribution: $distribution/$mode; jadix_macros=$hasMacros"
            & cargo test @options @features --test contracts -- --test-threads=1
            if ($LASTEXITCODE -ne 0) { throw "Product consumer failed: $distribution/$mode" }
        }
        Move-Item -LiteralPath (Join-Path $destination "Cargo.lock") -Destination (Join-Path $destination "Cargo-$distribution.lock")
    }
    Write-Host "Product distribution evidence: $destination"
} finally { Pop-Location }
