$ErrorActionPreference = 'Stop'
$repository = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$evidence = [IO.Path]::GetFullPath((Join-Path $repository 'development-docs/ops/evidence/r1-runtime-20261025'))

function Read-Json([string]$Path) {
    $document = Get-Content -LiteralPath $Path -Raw -Encoding utf8 | ConvertFrom-Json
    $result = @{}
    foreach ($property in $document.PSObject.Properties) { $result[$property.Name] = $property.Value }
    return $result
}

function Assert-Digests([string]$Root, $Manifest) {
    foreach ($entry in $Manifest.GetEnumerator()) {
        $path = Join-Path $Root $entry.Key
        if (!(Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing input: $($entry.Key)" }
        $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash
        if ($actual -ne $entry.Value) { throw "Changed input: $($entry.Key)" }
    }
}

$manifest = Read-Json (Join-Path $evidence 'sha256-manifest.json')
Assert-Digests $evidence $manifest
Write-Output "Sealed evidence: $($manifest.Count) recorded file digests match."
$inputs = Read-Json (Join-Path $evidence 'review-source-inputs.json')
Assert-Digests $repository $inputs
Write-Output "Runtime validation inputs: $($inputs.Count) unchanged."
$ci = Read-Json (Join-Path $evidence 'ci-review-inputs.json')
Assert-Digests $repository @{ '.github/workflows/ci.yml' = $ci.after_sha256 }

$commands = Read-Json (Join-Path $evidence 'review-command-results.json')
foreach ($entry in $commands.GetEnumerator()) {
    $result = $entry.Value
    $status = [int](Get-Content -LiteralPath (Join-Path $evidence ($entry.Key + '.exit')) -Raw)
    if ($result.exit -ne 0 -or $status -ne 0) { throw "Command failed: $($entry.Key)" }
    Assert-Digests $evidence @{ $result.log = $result.sha256 }
}
$summary = Read-Json (Join-Path $evidence 'review-validation-summary.json')
if (!$summary.frozen_inputs_unchanged -or $summary.feature_selections.Count -ne 40) { throw 'Invalid final summary' }
foreach ($configuration in $summary.configurations) {
    foreach ($group in $configuration.comparisons.PSObject.Properties.Value) {
        if ($group.lost.Count -ne 0) { throw "Lost test identities: $($configuration.name)" }
        foreach ($result in $group.result) {
            if ($result.status -ne 'ok' -or $result.failed -ne 0) { throw 'Invalid test result' }
        }
    }
    Write-Output "$($configuration.name): $($configuration.comparisons.library.result[0].passed) library tests passed; no baseline test identity lost."
}
if ($summary.native.Count -ne 3) { throw 'Native test inventory mismatch' }
foreach ($test in $summary.native) {
    if ($test.exit -ne 0 -or $test.passed -ne 1) { throw "Native test not exactly one pass: $($test.name)" }
}
Write-Output "Recorded commands: $($commands.Count) zero exits; isolated features: 40; native tests: 3 exact passes."
$documentation = Read-Json (Join-Path $evidence 'review-documentation-inputs.json')
$subsequent = @($documentation.Keys | Where-Object { (Get-FileHash -LiteralPath (Join-Path $repository $_) -Algorithm SHA256).Hash -ne $documentation[$_] })
if ($subsequent.Count -gt 0) {
    Write-Output "Subsequent documentation amendments (sealed documentation archive preserved): $($subsequent -join ', ')"
}
Write-Output 'PASS: recorded evidence and current runtime inputs match. This does not rerun tests, certify manual Gallery behavior, or sign off R1.'
