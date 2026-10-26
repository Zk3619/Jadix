param([switch]$Offline)

$ErrorActionPreference = 'Stop'
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $repositoryRoot
try {
    $cargoOptions = @('build', '-p', 'jadix', '--no-default-features', '--features', 'drag', '--message-format=json-render-diagnostics')
    if ($Offline) { $cargoOptions += '--offline' }
    $buildOutput = & cargo @cargoOptions
    if ($LASTEXITCODE -ne 0) { throw 'Minimal runtime build failed' }
    $libraries = @{}
    foreach ($line in $buildOutput) {
        $message = $line | ConvertFrom-Json
        if ($message.reason -eq 'compiler-artifact' -and $message.target.name -in @('jadix', 'taffy')) {
            foreach ($filename in $message.filenames) {
                if ($filename.EndsWith('.rlib')) { $libraries[$message.target.name] = $filename }
            }
        }
    }
    if (-not $libraries['jadix'] -or -not $libraries['taffy']) { throw 'Runtime or layout dependency artifact missing' }
    $profileDirectory = Split-Path $libraries['jadix'] -Parent
    $dependencyDirectory = if ((Split-Path $profileDirectory -Leaf) -eq 'deps') { $profileDirectory } else { Join-Path $profileDirectory 'deps' }
    $outputDirectory = Join-Path (Join-Path $profileDirectory 'dnd-source-contracts') ([guid]::NewGuid().ToString('N'))
    Write-Host "External DnD source output: $outputDirectory"
    $sourceDirectory = Join-Path $outputDirectory 'utils/drag'
    New-Item -ItemType Directory -Force -Path $sourceDirectory | Out-Null
    $sourceManifest = @()
    foreach ($source in Get-ChildItem 'crates/jadix/src/utils/drag' -Filter '*.rs') {
        if ($source.BaseName -in @('pointer', 'routing', 'runtime')) { continue }
        $destination = Join-Path $sourceDirectory $source.Name
        Copy-Item -LiteralPath $source.FullName -Destination $destination
        if ($source.Name -eq 'mod.rs') {
            $text = Get-Content -LiteralPath $destination -Raw
            foreach ($module in @('pointer', 'routing', 'runtime')) {
                $text = $text.Replace("pub mod $module;", "pub use ui_runtime::utils::drag::$module;")
            }
            [System.IO.File]::WriteAllText($destination, $text)
        }
        $sourceManifest += [pscustomobject]@{
            source = $source.Name
            original_sha256 = (Get-FileHash -LiteralPath $source.FullName -Algorithm SHA256).Hash
            copied_sha256 = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
            protocol_import_rewrite = $source.Name -eq 'mod.rs'
        }
    }
    $sourceManifest | ConvertTo-Json | Set-Content (Join-Path $outputDirectory 'source-manifest.json')
    $fixtures = Join-Path $repositoryRoot 'crates/jadix/tests/fixtures/dnd'
    Copy-Item -LiteralPath (Join-Path $fixtures 'provider.rs') -Destination (Join-Path $outputDirectory 'lib.rs')
    $providerLibrary = Join-Path $outputDirectory 'libexternal_dnd.rlib'
    & rustc --edition=2021 --crate-name external_dnd --crate-type rlib --cfg 'feature="dnd"' `
        (Join-Path $outputDirectory 'lib.rs') --extern "ui_runtime=$($libraries['jadix'])" `
        --extern "taffy=$($libraries['taffy'])" -L "dependency=$dependencyDirectory" -o $providerLibrary
    if ($LASTEXITCODE -ne 0) { throw 'Copied DnD product has unresolved external dependencies' }
    $consumerBinary = Join-Path $outputDirectory 'dnd_consumer.exe'
    & rustc --edition=2021 --crate-name dnd_consumer --test `
        (Join-Path $fixtures 'consumer.rs') --extern "ui_runtime=$($libraries['jadix'])" `
        --extern "external_dnd=$providerLibrary" -L "dependency=$dependencyDirectory" -o $consumerBinary
    if ($LASTEXITCODE -ne 0) { throw 'External DnD consumer failed to compile' }
    & $consumerBinary --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'External DnD contracts failed' }
}
finally { Pop-Location }
