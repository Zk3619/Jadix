param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$options = @('metadata', '--locked', '--all-features', '--format-version', '1', '--manifest-path', (Join-Path $repo 'Cargo.toml'))
if ($Offline) { $options += '--offline' }
$json = & cargo @options
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed' }
$metadata = $json | ConvertFrom-Json
$core = @($metadata.packages | Where-Object name -eq 'jadix_winit')
$dpi = @($metadata.packages | Where-Object name -eq 'dpi')
if ($core.Count -ne 1 -or $null -ne $core[0].source -or $core[0].version -ne '0.31.0-beta.3-jadix.1') { throw 'Expected one consolidated window package' }
if ($dpi.Count -ne 1 -or $dpi[0].version -ne '0.1.2' -or $dpi[0].source -notlike 'registry+*') { throw 'Expected one published DPI package' }
if (@($metadata.packages | Where-Object name -Like 'jadix_winit_*').Count) { throw 'Window implementation packages must be bundled into jadix_winit' }
if (@($metadata.packages | Where-Object name -In @('winit', 'winit-core', 'winit-ohos')).Count) { throw 'An upstream window identity was resolved alongside the maintained chain' }
Write-Output 'Window dependency identities verified: one consolidated window package and one registry DPI.'
