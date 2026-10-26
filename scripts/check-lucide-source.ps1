param([switch]$Offline)

$ErrorActionPreference = 'Stop'
$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location $repositoryRoot
try {
    $options = @('build', '-p', 'jadix', '--no-default-features', '--features', 'svg,animation', '--message-format=json-render-diagnostics')
    if ($Offline) { $options += '--offline' }
    $output = & cargo @options
    if ($LASTEXITCODE -ne 0) { throw 'Framework-only icon runtime build failed' }
    $libraries = @{}
    foreach ($line in $output) {
        $message = $line | ConvertFrom-Json
        if ($message.reason -eq 'compiler-artifact' -and $message.target.name -in @('jadix', 'serde', 'serde_json')) {
            foreach ($filename in $message.filenames) {
                if ($filename.EndsWith('.rlib')) { $libraries[$message.target.name] = $filename }
            }
        }
    }
    foreach ($name in @('jadix', 'serde', 'serde_json')) {
        if (-not $libraries[$name]) { throw "Missing library: $name" }
    }
    $libraryDirectory = Split-Path $libraries['jadix'] -Parent
    $dependencies = if ((Split-Path $libraryDirectory -Leaf) -eq 'deps') { $libraryDirectory } else { Join-Path $libraryDirectory 'deps' }
    $destination = Join-Path (Join-Path $dependencies 'lucide-source-contracts') ([guid]::NewGuid().ToString('N'))
    Write-Host "External Lucide source output: $destination"
    New-Item -ItemType Directory -Force -Path (Join-Path $destination 'src/utils'), (Join-Path $destination 'assets') | Out-Null
    $manifest = @()
    foreach ($relative in @('src/utils/lucide.rs', 'src/utils/lucide_animated.rs', 'assets/lucide.json', 'assets/lucide_paths.json', 'assets/lucide_animated.json', 'assets/lucide.ttf', 'NOTICE', 'LICENSE')) {
        $original = Join-Path 'crates/jadix' $relative
        $copied = Join-Path $destination $relative
        Copy-Item -LiteralPath $original -Destination $copied
        $manifest += [pscustomobject]@{ path = $relative; sha256 = (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash; copied_sha256 = (Get-FileHash -LiteralPath $copied -Algorithm SHA256).Hash }
    }
    $manifest | ConvertTo-Json | Set-Content (Join-Path $destination 'source-manifest.json')
    $fixtures = Join-Path $repositoryRoot 'crates/jadix/tests/fixtures/lucide'
    Copy-Item -LiteralPath (Join-Path $fixtures 'provider.rs') -Destination (Join-Path $destination 'src/lib.rs')
    $provider = Join-Path $destination 'libexternal_lucide.rlib'
    & rustc --edition=2021 --crate-name external_lucide --crate-type rlib --cfg 'feature="svg"' --cfg 'feature="bundled-lucide"' --cfg 'feature="lucide-animated"' `
        (Join-Path $destination 'src/lib.rs') --extern "ui_runtime=$($libraries['jadix'])" `
        --extern "serde=$($libraries['serde'])" --extern "serde_json=$($libraries['serde_json'])" -L "dependency=$dependencies" -o $provider
    if ($LASTEXITCODE -ne 0) { throw 'Copied Lucide source cannot use the public runtime' }
    $consumer = Join-Path $destination 'lucide_consumer.exe'
    & rustc --edition=2021 --crate-name lucide_consumer --test (Join-Path $fixtures 'consumer.rs') `
        --extern "ui_runtime=$($libraries['jadix'])" --extern "external_lucide=$provider" -L "dependency=$dependencies" -o $consumer
    if ($LASTEXITCODE -ne 0) { throw 'External Lucide consumer failed to compile' }
    & $consumer --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'External Lucide contracts failed' }
} finally { Pop-Location }
