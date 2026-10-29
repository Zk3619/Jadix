param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$options = @('metadata', '--locked', '--all-features', '--format-version', '1', '--manifest-path', (Join-Path $repo 'Cargo.toml'))
if ($Offline) { $options += '--offline' }
$json = & cargo @options
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$metadata = $json | ConvertFrom-Json
$core = @($metadata.packages | Where-Object name -eq 'jadix_winit_core')
$dpi = @($metadata.packages | Where-Object name -eq 'dpi')
if ($core.Count -ne 1 -or $null -ne $core[0].source -or $core[0].version -ne '0.31.0-beta.3-jadix.1') { throw 'Expected one maintained window core' }
if ($dpi.Count -ne 1 -or $dpi[0].version -ne '0.1.2' -or $dpi[0].source -notlike 'registry+*') { throw 'Expected one published DPI package' }
$forks = (Get-Content -Raw (Join-Path $repo 'vendor/RELEASE_FORKS.json') | ConvertFrom-Json).forks
foreach ($fork in $forks | Where-Object upstream_package -Like 'winit-*') {
    $packages = @($metadata.packages | Where-Object name -eq $fork.package)
    $manifest = [IO.Path]::GetFullPath((Join-Path $repo "$($fork.directory)/Cargo.toml"))
    if ($packages.Count -ne 1 -or $null -ne $packages[0].source -or [IO.Path]::GetFullPath($packages[0].manifest_path) -ne $manifest) { throw "Window source drift: $($fork.package)" }
    $node = $metadata.resolve.nodes | Where-Object id -eq $packages[0].id
    foreach ($dependency in $node.deps | Where-Object name -In @('winit_core', 'dpi')) {
        $expected = if ($dependency.name -eq 'dpi') { $dpi[0].id } else { $core[0].id }
        if ($dependency.pkg -ne $expected) { throw "Window type identity split: $($fork.package)/$($dependency.name)" }
    }
}
if (@($metadata.packages | Where-Object name -In @('winit', 'winit-core', 'winit-ohos')).Count) { throw 'An upstream window identity was resolved alongside the maintained chain' }
Write-Output 'Window dependency identities verified: one maintained core, one registry DPI and explicit versioned backend sources.'
