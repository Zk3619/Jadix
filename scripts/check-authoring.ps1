param([switch]$Offline)

$ErrorActionPreference = 'Stop'
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $repositoryRoot
try {
    $cargoOptions = @('build', '-p', 'jadix', '--no-default-features', '--features', 'drag', '--message-format=json-render-diagnostics')
    if ($Offline) { $cargoOptions += '--offline' }
    $buildOutput = & cargo @cargoOptions
    if ($LASTEXITCODE -ne 0) { throw 'Minimal authoring runtime build failed' }
    $runtimeLibrary = $null
    foreach ($line in $buildOutput) {
        $message = $line | ConvertFrom-Json
        if ($message.reason -eq 'compiler-artifact' -and $message.target.name -eq 'jadix') {
            foreach ($filename in $message.filenames) {
                if ($filename.EndsWith('.rlib')) { $runtimeLibrary = $filename }
            }
        }
    }
    if (-not $runtimeLibrary) { throw 'Cargo did not report the jadix runtime library' }
    $libraryDirectory = Split-Path $runtimeLibrary -Parent
    $dependencyDirectory = if ((Split-Path $libraryDirectory -Leaf) -eq 'deps') {
        $libraryDirectory
    } else {
        Join-Path $libraryDirectory 'deps'
    }
    $outputDirectory = Join-Path (Split-Path $dependencyDirectory -Parent) 'authoring-contracts'
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
    $providerLibrary = Join-Path $outputDirectory 'libauthoring_provider.rlib'
    $consumerBinary = Join-Path $outputDirectory 'authoring_consumer.exe'
    $fixtures = Join-Path $repositoryRoot 'crates/jadix/tests/fixtures/authoring'
    & rustc --edition=2021 --crate-name authoring_provider --crate-type rlib `
        (Join-Path $fixtures 'provider.rs') --extern "ui_runtime=$runtimeLibrary" `
        -L "dependency=$dependencyDirectory" -o $providerLibrary
    if ($LASTEXITCODE -ne 0) { throw 'Renamed-runtime component provider failed to compile' }
    & rustc --edition=2021 --crate-name authoring_consumer --test `
        (Join-Path $fixtures 'consumer.rs') --extern "ui_runtime=$runtimeLibrary" `
        --extern "authoring_provider=$providerLibrary" -L "dependency=$dependencyDirectory" `
        -o $consumerBinary
    if ($LASTEXITCODE -ne 0) { throw 'Cross-crate component consumer failed to compile' }
    & $consumerBinary --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Cross-crate authoring contracts failed' }
}
finally {
    Pop-Location
}
