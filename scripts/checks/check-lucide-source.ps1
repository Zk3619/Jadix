param([switch]$Offline)
$ErrorActionPreference = 'Stop'
& (Join-Path $PSScriptRoot 'check-product-distributions.ps1') -Offline:$Offline
if ($LASTEXITCODE -ne 0) { throw 'Unified Lucide product distribution checks failed' }
