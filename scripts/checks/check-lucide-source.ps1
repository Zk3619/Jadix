param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$catalog = Get-FileHash -LiteralPath (Join-Path $root 'crates/jadix_lucide/assets/lucide.json') -Algorithm SHA256
$watcherCatalog = Get-FileHash -LiteralPath (Join-Path $root 'crates/jadix_hot_reload/assets/lucide.json') -Algorithm SHA256
if ($catalog.Hash -ne $watcherCatalog.Hash) { throw 'Hot reload and Lucide must share the same glyph catalog contents' }
& (Join-Path $PSScriptRoot 'check-product-distributions.ps1') -Offline:$Offline
if ($LASTEXITCODE -ne 0) { throw 'Unified Lucide product distribution checks failed' }
