param([string]$OutputDirectory = (Join-Path $PSScriptRoot '../dist/signing-tests'))
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '../ohos-signing.ps1')
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
Add-Type -AssemblyName System.Security.Cryptography.Pkcs
$key = [Security.Cryptography.RSA]::Create(2048)
$request = [Security.Cryptography.X509Certificates.CertificateRequest]::new(
    'CN=Gallery signing preflight fixture', $key,
    [Security.Cryptography.HashAlgorithmName]::SHA256,
    [Security.Cryptography.RSASignaturePadding]::Pkcs1)
$certificate = $request.CreateSelfSigned([DateTimeOffset]::UtcNow.AddDays(-1), [DateTimeOffset]::UtcNow.AddDays(1))
try {
    [IO.File]::WriteAllBytes((Join-Path $OutputDirectory 'fixture.cer'), $certificate.RawData)
    [IO.File]::WriteAllBytes((Join-Path $OutputDirectory 'fixture.p12'), $certificate.Export([Security.Cryptography.X509Certificates.X509ContentType]::Pfx))
    $configurationPath = Join-Path $OutputDirectory 'build-profile.json5'
    $configuration = @{ app = @{ signingConfigs = @(@{
        name = 'default'
        material = @{ profile = './fixture.p7b'; certpath = './fixture.cer'; storeFile = './fixture.p12' }
    }) } }
    ('/* DevEco JSON5 signing configuration */' + "`n" + ($configuration | ConvertTo-Json -Depth 10)) |
        Set-Content -LiteralPath $configurationPath
    $now = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
    $cases = @(
        @{ name = 'authorized debug profile'; bundle = 'org.jadix.gallery'; before = $now - 60; after = $now + 3600; devices = @('fixture-device'); error = $null },
        @{ name = 'wrong bundle'; bundle = 'com.example.other'; before = $now - 60; after = $now + 3600; devices = @('fixture-device'); error = '*must authorize bundle*' },
        @{ name = 'expired profile'; bundle = 'org.jadix.gallery'; before = $now - 3600; after = $now - 60; devices = @('fixture-device'); error = '*expired or not yet valid*' },
        @{ name = 'future profile'; bundle = 'org.jadix.gallery'; before = $now + 60; after = $now + 3600; devices = @('fixture-device'); error = '*expired or not yet valid*' },
        @{ name = 'no device'; bundle = 'org.jadix.gallery'; before = $now - 60; after = $now + 3600; devices = @(); error = '*does not authorize a device*' },
        @{ name = 'missing device IDs'; bundle = 'org.jadix.gallery'; before = $now - 60; after = $now + 3600; devices = $null; error = '*does not authorize a device*' }
    )
    foreach ($case in $cases) {
        $profile = @{
            type = 'debug'
            'bundle-info' = @{ 'bundle-name' = $case.bundle }
            validity = @{ 'not-before' = $case.before; 'not-after' = $case.after }
            'debug-info' = @{ 'device-ids' = $case.devices }
        }
        $content = [Text.Encoding]::UTF8.GetBytes(($profile | ConvertTo-Json -Depth 10))
        $cms = [Security.Cryptography.Pkcs.SignedCms]::new([Security.Cryptography.Pkcs.ContentInfo]::new($content))
        $cms.ComputeSignature([Security.Cryptography.Pkcs.CmsSigner]::new($certificate))
        [IO.File]::WriteAllBytes((Join-Path $OutputDirectory 'fixture.p7b'), $cms.Encode())
        $failure = $null
        try {
            $result = Read-GalleryOhosSigningConfig -Path $configurationPath -DevecoHome (Split-Path -Parent $env:DEVECO_SDK_HOME)
        } catch { $failure = $_.Exception.Message }
        if ($case.error) {
            if ($failure -notlike $case.error) { throw "FAIL $($case.name): expected '$($case.error)', got '$failure'." }
        } elseif ($failure -or ![IO.Path]::IsPathRooted($result.material.profile)) {
            throw "FAIL $($case.name): $failure"
        }
        "PASS $($case.name)"
    }
    $failure = $null
    try {
        $null = Read-GalleryOhosSigningConfig -Path $configurationPath -Name absent -DevecoHome (Split-Path -Parent $env:DEVECO_SDK_HOME)
    } catch { $failure = $_.Exception.Message }
    if ($failure -notlike '*exactly one signing configuration*') { throw 'FAIL missing signing configuration' }
    'PASS missing signing configuration'
    $configuration.app.signingConfigs[0].material.certpath = './missing.cer'
    $configuration | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $configurationPath
    $failure = $null
    try {
        $null = Read-GalleryOhosSigningConfig -Path $configurationPath -DevecoHome (Split-Path -Parent $env:DEVECO_SDK_HOME)
    } catch { $failure = $_.Exception.Message }
    if ($failure -notlike '*missing.cer*') { throw 'FAIL missing signing certificate' }
    'PASS missing signing certificate'
} finally {
    $certificate.Dispose()
    $key.Dispose()
}
