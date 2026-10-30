param(
    [ValidateSet('phone', 'desktop')][string]$Device = 'phone',
    [ValidateSet('arm64', 'x86_64')][string]$Architecture = 'arm64',
    [string]$OutputDirectory,
    [string]$ProjectDirectory,
    [switch]$SkipBuild,
    [string]$NativeLibrary,
    [switch]$Unsigned,
    [switch]$LocalTestSignature,
    [string]$SigningConfigFile,
    [string]$SigningConfigName = 'default'
)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$target = if ($Architecture -eq 'arm64') { 'aarch64-unknown-linux-ohos' } else { 'x86_64-unknown-linux-ohos' }
$sdkTarget = if ($Architecture -eq 'arm64') { 'aarch64-linux-ohos' } else { 'x86_64-linux-ohos' }
$abi = if ($Architecture -eq 'arm64') { 'arm64-v8a' } else { 'x86_64' }
if (!$OutputDirectory) { $OutputDirectory = Join-Path $PSScriptRoot "dist/ohos-$Device-$Architecture" }
if (!$env:DEVECO_SDK_HOME) { throw 'Set DEVECO_SDK_HOME to the DevEco SDK directory (API 22 or later).' }
$sdk = Join-Path $env:DEVECO_SDK_HOME 'default/openharmony'
$deveco = Split-Path -Parent $env:DEVECO_SDK_HOME
$signingModes = [int]$Unsigned.IsPresent + [int]$LocalTestSignature.IsPresent + [int][bool]$SigningConfigFile
if ($signingModes -gt 1) { throw 'Choose only one of -Unsigned, -LocalTestSignature or -SigningConfigFile.' }
if ($NativeLibrary -and !$SkipBuild) { throw '-NativeLibrary requires -SkipBuild.' }
$signing = $null
if ($SigningConfigFile) {
    . (Join-Path $PSScriptRoot 'ohos-signing.ps1')
    $signing = Read-GalleryOhosSigningConfig -Path $SigningConfigFile -Name $SigningConfigName -DevecoHome $deveco
}
if ($LocalTestSignature) {
    Write-Warning 'Local test certificates are not trusted by retail HarmonyOS devices. This HAP is for systems that explicitly trust the test root.'
} elseif (!$signing) {
    Write-Warning 'Producing an unsigned HAP for systems that allow unsigned installation. Retail devices require a valid Huawei developer profile and -SigningConfigFile.'
}
$env:OHOS_NDK_HOME = $sdk
$env:LIBCLANG_PATH = Join-Path $sdk 'native/llvm/bin'
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$project = if ($ProjectDirectory) { [IO.Path]::GetFullPath($ProjectDirectory) } else { Join-Path $OutputDirectory "project-$Device-$Architecture" }
New-Item -ItemType Directory -Force $OutputDirectory, $project | Out-Null
if (!$env:HVIGOR_USER_HOME) { $env:HVIGOR_USER_HOME = Join-Path $OutputDirectory 'hvigor-cache' }
if (!$SkipBuild) {
    $native = Join-Path $sdk 'native'
    $linker = Join-Path $OutputDirectory 'ohos-sdk-link.cmd'
    "@echo off`r`n`"$native/llvm/bin/clang.exe`" --target=$sdkTarget --sysroot=`"$native/sysroot`" %*" | Set-Content -LiteralPath $linker
    [Environment]::SetEnvironmentVariable(('CARGO_TARGET_' + $target.Replace('-', '_').ToUpperInvariant() + '_LINKER'), $linker, 'Process')
    Remove-Item Env:ZIG_LIBC -ErrorAction SilentlyContinue
    $features = if ($Device -eq 'desktop') { 'full,ohos-desktop' } else { 'full' }
    Push-Location $workspace
    try {
        cargo zigbuild -p gallery_mobile --lib --release --locked --no-default-features --features $features --target $target
        if ($LASTEXITCODE -ne 0) { throw 'OHOS native build failed.' }
    } finally { Pop-Location }
}
$library = if ($NativeLibrary) { [IO.Path]::GetFullPath($NativeLibrary) } else {
    Join-Path $workspace "target/$target/release/libgallery_mobile.so"
}
if (!(Test-Path -LiteralPath $library)) { throw "Missing $library" }
$header = [byte[]]::new(20)
$nativeStream = [IO.File]::OpenRead($library)
try { $headerLength = $nativeStream.Read($header, 0, $header.Length) } finally { $nativeStream.Dispose() }
$machine = if ($Architecture -eq 'arm64') { 183 } else { 62 }
if ($headerLength -ne 20 -or [Text.Encoding]::ASCII.GetString($header, 0, 4) -ne "`u{007f}ELF" -or
    $header[4] -ne 2 -or $header[5] -ne 1 -or [BitConverter]::ToUInt16($header, 18) -ne $machine) {
    throw "Native library does not match OHOS $Architecture (ELF64 little-endian). Choose the corresponding -NativeLibrary."
}
$libraryBackup = Join-Path $OutputDirectory "libgallery-$Device-$Architecture.so"
if ($library -ne $libraryBackup) { Copy-Item -LiteralPath $library -Destination $libraryBackup -Force }
$source = Join-Path $PSScriptRoot 'ohos'
foreach ($file in Get-ChildItem -LiteralPath $source -Recurse -File) {
    $relative = [IO.Path]::GetRelativePath($source, $file.FullName)
    if ($relative -match '(^|[\\/])(\.hvigor|oh_modules|build|libs)([\\/]|$)' -or $relative -eq 'local.properties') { continue }
    $destination = Join-Path $project $relative
    New-Item -ItemType Directory -Force ([IO.Path]::GetDirectoryName($destination)) | Out-Null
    Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
}
$buildProfilePath = Join-Path $project 'build-profile.json5'
$buildProfile = Get-Content -LiteralPath $buildProfilePath -Raw | ConvertFrom-Json
if ($signing) {
    $buildProfile.app | Add-Member -Force -NotePropertyName signingConfigs -NotePropertyValue @($signing)
    $buildProfile.app.products[0] | Add-Member -Force -NotePropertyName signingConfig -NotePropertyValue $SigningConfigName
} else {
    $buildProfile.app.PSObject.Properties.Remove('signingConfigs')
    $buildProfile.app.products[0].PSObject.Properties.Remove('signingConfig')
}
$buildProfile | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $buildProfilePath
if ($Device -eq 'desktop') {
    $modulePath = Join-Path $project 'entry/src/main/module.json5'
    $module = Get-Content -LiteralPath $modulePath -Raw | ConvertFrom-Json
    $module.module.deviceTypes = @('2in1')
    $module.module.abilities[0].srcEntry = './ets/entryability/DesktopAbility.ets'
    $module.module | Add-Member -Force -NotePropertyName extensionAbilities -NotePropertyValue @(@{
        name = 'GalleryStatusBarAbility'
        srcEntry = './ets/entryability/GalleryStatusBarAbility.ets'
        type = 'statusBarView'
        exported = $false
    })
    $module | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $modulePath
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'ohos-desktop/DesktopAbility.ets'), (Join-Path $PSScriptRoot 'ohos-desktop/GalleryStatusBarAbility.ets') -Destination (Join-Path $project 'entry/src/main/ets/entryability') -Force
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'ohos-desktop/StatusBar.ets') -Destination (Join-Path $project 'entry/src/main/ets/pages') -Force
    '{"src":["pages/Index","pages/StatusBar"]}' | Set-Content -LiteralPath (Join-Path $project 'entry/src/main/resources/base/profile/main_pages.json')
}
$libs = Join-Path $project "entry/libs/$abi"
New-Item -ItemType Directory -Force $libs | Out-Null
Copy-Item -LiteralPath $library -Destination (Join-Path $libs 'libgallery_mobile.so') -Force
Copy-Item -LiteralPath (Join-Path $sdk "native/llvm/lib/$sdkTarget/libc++_shared.so") -Destination $libs -Force
Push-Location $project
try {
    $env:NODE_HOME = Join-Path $deveco 'tools/node'
    $env:PATH = "$env:NODE_HOME;$env:PATH"
    & (Join-Path $deveco 'tools/ohpm/bin/ohpm.bat') install --all
    if ($LASTEXITCODE -ne 0) { throw 'OHOS HAR dependency installation failed.' }
    & (Join-Path $deveco 'tools/hvigor/bin/hvigorw.bat') --mode module -p product=default -p module=entry@default -p buildMode=release assembleHap --no-daemon
    if ($LASTEXITCODE -ne 0) { throw 'OHOS HAP assembly failed.' }
} finally { Pop-Location }
$hap = Get-ChildItem -LiteralPath (Join-Path $project 'entry/build') -Recurse -Filter '*-unsigned.hap' | Select-Object -First 1
if (!$hap) { throw 'Hvigor did not produce an unsigned HAP.' }
$unsignedHap = Join-Path $OutputDirectory "jadix-gallery-ohos-$Device-$Architecture-release-unsigned.hap"
Copy-Item -LiteralPath $hap.FullName -Destination $unsignedHap -Force
if ($signing) {
    $signed = Get-ChildItem -LiteralPath (Join-Path $project 'entry/build') -Recurse -Filter '*-signed.hap' | Select-Object -First 1
    if (!$signed) { throw 'Hvigor did not produce a developer-signed HAP.' }
    $signedHap = Join-Path $OutputDirectory "jadix-gallery-ohos-$Device-$Architecture-release-signed.hap"
    Copy-Item -LiteralPath $signed.FullName -Destination $signedHap -Force
    & (Join-Path $env:JAVA_HOME 'bin/java.exe') -jar (Join-Path $sdk 'toolchains/lib/hap-sign-tool.jar') verify-app -inFile $signedHap -outCertChain (Join-Path $OutputDirectory 'verified-chain.cer') -outProfile (Join-Path $OutputDirectory 'verified-profile.p7b')
    if ($LASTEXITCODE -ne 0) { throw 'OHOS developer HAP signature verification failed.' }
    Get-FileHash -LiteralPath $signedHap -Algorithm SHA256
} elseif ($LocalTestSignature) {
    $java = Join-Path $env:JAVA_HOME 'bin/java.exe'
    $signingTools = Join-Path $sdk 'toolchains/lib'
    $signer = Join-Path $signingTools 'hap-sign-tool.jar'
    $keystore = Join-Path $OutputDirectory 'local-gallery-release.p12'
    $keytool = Join-Path $env:JAVA_HOME 'bin/keytool.exe'
    $rootCertificate = Join-Path $OutputDirectory 'local-root.cer'
    $caCertificate = Join-Path $OutputDirectory 'local-ca.cer'
    $certificatePath = Join-Path $OutputDirectory 'local-app-chain.cer'
    $profileCertificate = Join-Path $OutputDirectory 'local-profile-chain.cer'
    $password = 'jadix-gallery-local'
    $rootSubject = 'C=CN,O=Jadix Local Build,CN=Jadix Local Root'
    $caSubject = 'C=CN,O=Jadix Local Build,CN=Jadix Local Signing CA'
    if (!(Test-Path -LiteralPath $profileCertificate)) {
        & $java -jar $signer generate-ca -keyAlias gallery-root -keyPwd $password -keyAlg ECC -keySize NIST-P-256 -subject $rootSubject -validity 3650 -basicConstraintsPathLen 1 -signAlg SHA256withECDSA -keystoreFile $keystore -keystorePwd $password -outFile $rootCertificate
        if ($LASTEXITCODE -ne 0) { throw 'Local test root generation failed.' }
        & $java -jar $signer generate-ca -keyAlias gallery-ca -keyPwd $password -keyAlg ECC -keySize NIST-P-256 -issuer $rootSubject -issuerKeyAlias gallery-root -issuerKeyPwd $password -subject $caSubject -validity 3650 -signAlg SHA256withECDSA -keystoreFile $keystore -keystorePwd $password -outFile $caCertificate
        if ($LASTEXITCODE -ne 0) { throw 'Local test CA generation failed.' }
        foreach ($alias in @('gallery-app', 'gallery-profile')) {
            & $keytool -genkeypair -alias $alias -keyalg EC -groupname secp256r1 -dname "CN=$alias,O=Jadix Local Build,C=CN" -validity 3650 -keystore $keystore -storepass $password -keypass $password
            if ($LASTEXITCODE -ne 0) { throw 'Local test signing key generation failed.' }
            $command = if ($alias -eq 'gallery-app') { 'generate-app-cert' } else { 'generate-profile-cert' }
            $certificate = if ($alias -eq 'gallery-app') { $certificatePath } else { $profileCertificate }
            & $java -jar $signer $command -keyAlias $alias -keyPwd $password -issuer $caSubject -issuerKeyAlias gallery-ca -issuerKeyPwd $password -subject "C=CN,O=Jadix Local Build,CN=$alias" -validity 365 -signAlg SHA256withECDSA -keystoreFile $keystore -keystorePwd $password -outForm certChain -rootCaCertFile $rootCertificate -subCaCertFile $caCertificate -outFile $certificate
            if ($LASTEXITCODE -ne 0) { throw 'Local test certificate generation failed.' }
        }
    }
    $profile = Get-Content -LiteralPath (Join-Path $signingTools 'UnsgnedReleasedProfileTemplate.json') -Raw | ConvertFrom-Json
    $certificates = Get-Content -LiteralPath $certificatePath -Raw
    $chain = [regex]::Matches($certificates, '(?s)-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----')
    if ($chain.Count -ne 3) { throw 'Local test certificate chain is incomplete.' }
    $profile.uuid = [guid]::NewGuid().ToString()
    $profile.validity.'not-before' = [DateTimeOffset]::UtcNow.AddDays(-1).ToUnixTimeSeconds()
    $profile.validity.'not-after' = [DateTimeOffset]::UtcNow.AddYears(1).ToUnixTimeSeconds()
    $profile.'bundle-info'.'bundle-name' = 'org.jadix.gallery'
    $profile.'bundle-info'.'distribution-certificate' = $chain[0].Value + "`n"
    $profile.acls.'allowed-acls' = @()
    $profilePath = Join-Path $OutputDirectory 'local-test-profile.json'
    $profile | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $profilePath
    $signedProfile = Join-Path $OutputDirectory 'local-test-profile.p7b'
    & $java -jar $signer sign-profile -mode localSign -keyAlias gallery-profile -keyPwd $password -profileCertFile $profileCertificate -inFile $profilePath -signAlg SHA256withECDSA -keystoreFile $keystore -keystorePwd $password -outFile $signedProfile
    if ($LASTEXITCODE -ne 0) { throw 'Local test profile signing failed.' }
    $signedHap = Join-Path $OutputDirectory "jadix-gallery-ohos-$Device-$Architecture-release-local-test-signed.hap"
    & $java -jar $signer sign-app -mode localSign -keyAlias gallery-app -keyPwd $password -appCertFile $certificatePath -profileFile $signedProfile -inFile $unsignedHap -signAlg SHA256withECDSA -keystoreFile $keystore -keystorePwd $password -compatibleVersion 22 -outFile $signedHap
    if ($LASTEXITCODE -ne 0) { throw 'Local test HAP signing failed.' }
    & $java -jar $signer verify-app -inFile $signedHap -outCertChain (Join-Path $OutputDirectory 'verified-chain.cer') -outProfile (Join-Path $OutputDirectory 'verified-profile.p7b')
    if ($LASTEXITCODE -ne 0) { throw 'OHOS HAP signature verification failed.' }
    Get-FileHash -LiteralPath $signedHap -Algorithm SHA256
} else {
    Get-FileHash -LiteralPath $unsignedHap -Algorithm SHA256
}
