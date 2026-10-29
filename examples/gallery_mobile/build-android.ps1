param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist/android-arm64'),
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$sdk = $env:ANDROID_HOME
if (!$sdk) { throw 'Set ANDROID_HOME to the Android SDK directory.' }
$ndk = $env:ANDROID_NDK_HOME
if (!$ndk) { $ndk = $env:NDK_HOME }
if (!$ndk) { throw 'Set ANDROID_NDK_HOME or NDK_HOME to the Android NDK directory.' }
$env:ANDROID_NDK_HOME = $ndk
$toolchain = Join-Path $ndk 'toolchains/llvm/prebuilt/windows-x86_64'
$sysroot = Join-Path $toolchain 'sysroot'
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$staging = Join-Path $OutputDirectory 'staging'
New-Item -ItemType Directory -Force $staging | Out-Null
$libc = Join-Path $staging 'android-libc.txt'
@"
include_dir=$sysroot/usr/include
sys_include_dir=$sysroot/usr/include/aarch64-linux-android
crt_dir=$sysroot/usr/lib/aarch64-linux-android/28
msvc_lib_dir=
kernel32_lib_dir=
gcc_dir=
"@ | Set-Content -LiteralPath $libc
if (!$SkipBuild) {
    $env:ZIG_LIBC = $libc
    $zigbuild = (Get-Command cargo-zigbuild).Source
    foreach ($compiler in @('cc', 'c++')) {
        $wrapper = Join-Path $staging ('jadix-' + $compiler.Replace('+', 'x') + '.bat')
        "@echo off`r`n`"$zigbuild`" zig $compiler -- -target aarch64-linux-android %*" | Set-Content -LiteralPath $wrapper
        $variable = if ($compiler -eq 'cc') { 'CC_aarch64_linux_android' } else { 'CXX_aarch64_linux_android' }
        [Environment]::SetEnvironmentVariable($variable, $wrapper, 'Process')
    }
    Push-Location $workspace
    try {
        cargo zigbuild -p gallery_mobile --lib --release --locked --target aarch64-linux-android
        if ($LASTEXITCODE -ne 0) { throw 'Android native build failed.' }
    } finally { Pop-Location }
}
$library = Join-Path $workspace 'target/aarch64-linux-android/release/libgallery_mobile.so'
if (!(Test-Path -LiteralPath $library)) { throw "Missing $library" }
$buildTools = Get-ChildItem (Join-Path $sdk 'build-tools') -Directory | Sort-Object Name -Descending | Select-Object -First 1
$platformDirectory = Get-ChildItem (Join-Path $sdk 'platforms') -Directory |
    Where-Object { $_.Name -match '^android-\d+$' -and [int]$_.Name.Substring(8) -ge 35 } |
    Sort-Object { [int]$_.Name.Substring(8) } -Descending | Select-Object -First 1
if (!$platformDirectory) { throw 'Install Android SDK platform 35 or later.' }
$platform = Join-Path $platformDirectory.FullName 'android.jar'
$aapt = Join-Path $buildTools.FullName 'aapt2.exe'
$resources = Join-Path $staging 'res/drawable'
New-Item -ItemType Directory -Force $resources | Out-Null
Copy-Item -LiteralPath (Join-Path $workspace 'assets/icons/desktop/window.png') -Destination (Join-Path $resources 'app_icon.png') -Force
& $aapt compile --dir (Join-Path $staging 'res') -o (Join-Path $staging 'resources.zip')
if ($LASTEXITCODE -ne 0) { throw 'Android resource compilation failed.' }
$unsigned = Join-Path $staging 'unsigned.apk'
& $aapt link --manifest (Join-Path $PSScriptRoot 'android/AndroidManifest.xml') -I $platform -o $unsigned (Join-Path $staging 'resources.zip')
if ($LASTEXITCODE -ne 0) { throw 'Android resource linking failed.' }
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::Open($unsigned, [IO.Compression.ZipArchiveMode]::Update)
try {
    $entry = $archive.GetEntry('lib/arm64-v8a/libgallery_mobile.so')
    if ($entry) { $entry.Delete() }
    [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archive, $library, 'lib/arm64-v8a/libgallery_mobile.so') | Out-Null
} finally { $archive.Dispose() }
$aligned = Join-Path $staging 'aligned.apk'
& (Join-Path $buildTools.FullName 'zipalign.exe') -f -P 16 4 $unsigned $aligned
if ($LASTEXITCODE -ne 0) { throw 'Android zip alignment failed.' }
$keystore = Join-Path $OutputDirectory 'local-gallery-release.p12'
if (!(Test-Path -LiteralPath $keystore)) {
    & (Join-Path $env:JAVA_HOME 'bin/keytool.exe') -genkeypair -keystore $keystore -storetype PKCS12 -alias gallery -storepass jadix-gallery-local -keypass jadix-gallery-local -keyalg RSA -keysize 2048 -validity 3650 -dname 'CN=Jadix Gallery Local Build'
    if ($LASTEXITCODE -ne 0) { throw 'Local Android signing key generation failed.' }
}
$apk = Join-Path $OutputDirectory 'jadix-gallery-android-arm64-release.apk'
$signer = Join-Path $buildTools.FullName 'apksigner.bat'
& $signer sign --ks $keystore --ks-key-alias gallery --ks-pass pass:jadix-gallery-local --out $apk $aligned
if ($LASTEXITCODE -ne 0) { throw 'Android signing failed.' }
& $signer verify --verbose $apk
if ($LASTEXITCODE -ne 0) { throw 'Android signature verification failed.' }
& $aapt dump badging $apk
Get-FileHash -LiteralPath $apk -Algorithm SHA256
